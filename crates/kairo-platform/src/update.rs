use crate::Store;
use anyhow::{bail, Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
};
pub const DATA_FILES: [&str; 9] = [
    "config.json",
    "runtime.json",
    "state.json",
    "cursor.json",
    "deployment.json",
    "accounts.json",
    "status.json",
    "natural-acceptance.json",
    "confirmations.json",
];
#[derive(Serialize, Deserialize)]
pub struct Target {
    pub url: String,
    pub sha256: String,
}
#[derive(Serialize, Deserialize)]
pub struct Manifest {
    pub schema: u32,
    pub version: String,
    pub targets: std::collections::BTreeMap<String, Target>,
}
pub fn architecture() -> Result<&'static str> {
    match std::env::consts::ARCH {
        "x86_64" => Ok("x86_64-unknown-linux-gnu"),
        "aarch64" => Ok("aarch64-unknown-linux-gnu"),
        _ => bail!("architecture_unsupported / 不支持的架构"),
    }
}
pub fn sha(path: &Path) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}
fn version(s: &str) -> Result<(u64, u64, u64)> {
    let v: Vec<_> = s.split('.').collect();
    if v.len() != 3 {
        bail!("version_invalid / 版本格式无效")
    }
    Ok((v[0].parse()?, v[1].parse()?, v[2].parse()?))
}
pub fn check_manifest(bytes: &[u8]) -> Result<serde_json::Value> {
    let m: Manifest = serde_json::from_slice(bytes)?;
    if m.schema != 1 {
        bail!("manifest_schema_invalid / 更新清单版本无效")
    };
    let newer = version(&m.version)? > version(env!("CARGO_PKG_VERSION"))?;
    let t = m
        .targets
        .get(architecture()?)
        .context("release_target_missing / 更新缺少当前架构")?;
    validate_hash(&t.sha256)?;
    Ok(
        serde_json::json!({"version":m.version,"update_available":newer,"target":architecture()?,"sha256":t.sha256,"url":t.url}),
    )
}
fn validate_hash(hash: &str) -> Result<()> {
    if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("checksum_invalid / 校验值格式无效")
    }
    Ok(())
}
fn copy_sync(source: &Path, dest: &Path, mode: u32) -> Result<()> {
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .custom_flags(libc::O_NOFOLLOW)
        .open(dest)?;
    crate::preserve_owner(&f, dest.parent().context("unsafe_path / 路径不安全")?)?;
    f.write_all(&fs::read(source)?)?;
    f.sync_all()?;
    Ok(())
}
// Capture one immutable byte buffer, validate it, then verify the persisted copy.
// Callers keep destination directories protected and never execute on an error.
fn copy_verified(source: &Path, dest: &Path, expected: &str, mode: u32) -> Result<()> {
    validate_hash(expected)?;
    let bytes = fs::read(source)?;
    if format!("{:x}", Sha256::digest(&bytes)) != expected.to_ascii_lowercase() {
        bail!("checksum_mismatch / 校验失败")
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .custom_flags(libc::O_NOFOLLOW)
        .open(dest)?;
    crate::preserve_owner(&file, dest.parent().context("unsafe_path / 路径不安全")?)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    if sha(dest)? != expected.to_ascii_lowercase() {
        bail!("checksum_mismatch / 校验失败")
    }
    Ok(())
}
fn directory(path: &Path) -> Result<()> {
    let absolute = std::path::absolute(path)?;
    if absolute
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
        || [
            "/",
            "/tmp",
            "/opt",
            "/opt/Kairo",
            "/root",
            "/home",
            "/var",
            "/var/lib",
            "/usr",
            "/usr/local",
            "/etc",
            "/bin",
        ]
        .iter()
        .any(|p| absolute == Path::new(p))
    {
        bail!("private_directory_required / 必须指定独立的私有目录")
    }
    let existed = path.exists();
    fs::create_dir_all(path)?;
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        bail!("unsafe_path / 路径不安全")
    };
    if !existed {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    if fs::metadata(path)?.permissions().mode() & 0o022 != 0 {
        bail!("unsafe_permissions / 目录不可允许其他用户写入")
    }
    Ok(())
}
fn lease(data: &Path) -> Result<File> {
    directory(data)?;
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(data.join("daemon.lock"))?;
    f.try_lock_exclusive()
        .context("stop_kairo_before_update / 升级前请停止 Kairo 服务")?;
    Ok(f)
}
fn install_lease(prefix: &Path) -> Result<File> {
    directory(prefix)?;
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(prefix.join(".install.lock"))?;
    f.lock_exclusive()?;
    Ok(f)
}
fn versioned(prefix: &Path) -> bool {
    prefix.join("releases").is_dir()
}
fn switch_current(prefix: &Path, target: &Path) -> Result<()> {
    let parts: Vec<_> = target.components().collect();
    if parts.len() != 2
        || parts[0].as_os_str() != "releases"
        || !parts
            .iter()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
        || !prefix.join(target).join("bin/kairo").is_file()
    {
        bail!("release_path_invalid / 版本目录无效")
    }
    let tmp = prefix.join(".current.new");
    if fs::symlink_metadata(&tmp).is_ok() {
        fs::remove_file(&tmp)?;
    }
    std::os::unix::fs::symlink(target, &tmp)?;
    fs::rename(tmp, prefix.join("current"))?;
    File::open(prefix)?.sync_all()?;
    Ok(())
}
fn publish_candidate(prefix: &Path, candidate: &Path) -> Result<()> {
    let o = crate::process::output(Command::new(candidate).arg("--version"), 10, 4096)?;
    let text = std::str::from_utf8(&o.stdout)?;
    let release = text
        .trim()
        .strip_prefix("kairo ")
        .context("candidate_invalid / 候选程序校验失败")?;
    version(release)?;
    let hash = sha(candidate)?;
    let target = PathBuf::from(format!("releases/{release}-{}", &hash[..16]));
    let directory = prefix.join(&target);
    if directory.exists() {
        if sha(&directory.join("bin/kairo"))? != hash {
            bail!("checksum_mismatch / 校验失败")
        }
    } else {
        let tmp = prefix.join(format!("releases/.new-{}", std::process::id()));
        if tmp.exists() {
            fs::remove_dir_all(&tmp)?;
        }
        fs::create_dir_all(tmp.join("bin"))?;
        copy_verified(candidate, &tmp.join("bin/kairo"), &hash, 0o755)?;
        File::open(tmp.join("bin"))?.sync_all()?;
        File::open(&tmp)?.sync_all()?;
        fs::rename(tmp, &directory)?;
        File::open(prefix.join("releases"))?.sync_all()?;
    }
    switch_current(prefix, &target)?;
    let link = prefix.join("bin/kairo");
    if fs::symlink_metadata(&link).is_err() {
        std::os::unix::fs::symlink("../current/bin/kairo", link)?;
    }
    Ok(())
}
fn recover_locked(prefix: &Path, data: &Path) -> Result<()> {
    let marker = prefix.join(".upgrade-pending");
    if !marker.exists() {
        if data.join(".upgrade-lock").exists() {
            fs::remove_file(data.join(".upgrade-lock"))?;
            File::open(data)?.sync_all()?;
        }
        return Ok(());
    }
    let snap = prefix.join(".upgrade-snapshot");
    let bin = prefix.join("bin/kairo");
    if versioned(prefix) {
        if snap.join("current-target").exists() {
            let target = fs::read_to_string(snap.join("current-target"))?;
            let old = prefix.join(&target).join("bin/kairo");
            if sha(&old)? != sha(&snap.join("binary"))? {
                bail!("checksum_mismatch / 校验失败")
            }
            switch_current(prefix, Path::new(&target))?;
        } else if fs::symlink_metadata(prefix.join("current")).is_ok() {
            fs::remove_file(prefix.join("current"))?;
        }
    } else if snap.join("binary").exists() {
        let temp = prefix.join("bin/.kairo.recover");
        if temp.exists() {
            fs::remove_file(&temp)?
        }
        copy_sync(&snap.join("binary"), &temp, 0o755)?;
        fs::rename(temp, &bin)?;
    } else if bin.exists() {
        fs::remove_file(&bin)?;
    }
    for name in DATA_FILES {
        let saved = snap.join(name);
        let dest = data.join(name);
        if saved.exists() {
            let tmp = data.join(format!(".{name}.rollback"));
            if tmp.exists() {
                fs::remove_file(&tmp)?
            }
            copy_sync(&saved, &tmp, 0o600)?;
            fs::rename(tmp, dest)?;
        } else if dest.exists() {
            fs::remove_file(dest)?;
        }
    }
    File::open(data)?.sync_all()?;
    fs::remove_file(marker)?;
    if data.join(".upgrade-lock").exists() {
        fs::remove_file(data.join(".upgrade-lock"))?;
    }
    File::open(data)?.sync_all()?;
    File::open(prefix)?.sync_all()?;
    Ok(())
}
pub fn recover(prefix: &Path, data: &Path) -> Result<()> {
    let _lease = lease(data)?;
    let _install = install_lease(prefix)?;
    let _store = Store::open(data)?;
    recover_locked(prefix, data)
}
pub fn install(source: &Path, expected: &str, prefix: &Path, data: &Path) -> Result<()> {
    validate_hash(expected)?;
    if sha(source)? != expected.to_ascii_lowercase() {
        bail!("checksum_mismatch / 校验失败")
    }
    let _lease = lease(data)?;
    let _install = install_lease(prefix)?;
    {
        let _store = Store::open(data)?;
        recover_locked(prefix, data)?;
    }
    directory(&prefix.join("bin"))?;
    let snap = prefix.join(".upgrade-snapshot");
    if snap.exists() {
        fs::remove_dir_all(&snap)?
    }
    directory(&snap)?;
    let bin = prefix.join("bin/kairo");
    if bin.exists() {
        copy_sync(&bin, &snap.join("binary"), 0o700)?;
        if versioned(prefix) {
            let target = fs::read_link(prefix.join("current"))?;
            let mut f = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(snap.join("current-target"))?;
            f.write_all(target.as_os_str().as_encoded_bytes())?;
            f.sync_all()?;
        }
    }
    {
        let store = Store::open(data)?;
        crate::management::validate(&store)?;
        for name in DATA_FILES {
            let path = data.join(name);
            if path.exists() {
                copy_sync(&path, &snap.join(name), 0o600)?;
            }
        }
        store.write(".upgrade-lock", &prefix)?;
    }
    File::open(&snap)?.sync_all()?;
    let mut marker = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(prefix.join(".upgrade-pending"))?;
    marker.write_all(b"schema=1\n")?;
    marker.sync_all()?;
    File::open(prefix)?.sync_all()?;
    let candidate = prefix.join("bin/.kairo.new");
    let result = (|| -> Result<()> {
        if candidate.exists() {
            fs::remove_file(&candidate)?;
        }
        copy_verified(source, &candidate, expected, 0o755)?;
        if !crate::process::check(Command::new(&candidate).arg("--version"))? {
            bail!("candidate_invalid / 候选程序校验失败")
        }
        // Validate/migrate while the daemon lease is held. Migration only touches known Kairo JSON.
        if !crate::process::check(
            Command::new(&candidate)
                .arg("--data-dir")
                .arg(data)
                .arg("migrate"),
        )? {
            bail!("migration_failed / 状态迁移失败")
        }
        if versioned(prefix) {
            publish_candidate(prefix, &candidate)?;
            fs::remove_file(&candidate)?;
        } else {
            fs::rename(&candidate, &bin)?;
        }
        File::open(prefix.join("bin"))?.sync_all()?;
        if sha(&bin)? != expected.to_ascii_lowercase() {
            bail!("checksum_mismatch / 校验失败")
        }
        if !crate::process::check(
            Command::new(&bin)
                .arg("--data-dir")
                .arg(data)
                .arg("validate"),
        )? {
            bail!("post_update_failed / 升级后校验失败")
        };
        Ok(())
    })();
    if let Err(e) = result {
        let _store = Store::open(data)?;
        recover_locked(prefix, data)?;
        if let Ok(metadata) = fs::symlink_metadata(&candidate) {
            if metadata.file_type().is_file() || metadata.file_type().is_symlink() {
                fs::remove_file(candidate)?;
            }
        }
        return Err(e);
    }
    fs::remove_file(prefix.join(".upgrade-pending"))?;
    fs::remove_file(data.join(".upgrade-lock"))?;
    File::open(data)?.sync_all()?;
    File::open(prefix)?.sync_all()?;
    Ok(())
}
pub fn rollback(prefix: &Path, data: &Path) -> Result<()> {
    let _lease = lease(data)?;
    let _install = install_lease(prefix)?;
    let snap = prefix.join(".upgrade-snapshot/binary");
    if !snap.exists() {
        bail!("rollback_missing / 没有可回滚版本")
    }
    // Never restore historical scheduling state on an ordinary binary rollback.
    let tmp = prefix.join("bin/.kairo.rollback");
    if tmp.exists() {
        fs::remove_file(&tmp)?
    }
    copy_sync(&snap, &tmp, 0o755)?;
    if !crate::process::check(
        Command::new(&tmp)
            .arg("--data-dir")
            .arg(data)
            .arg("validate"),
    )? {
        fs::remove_file(tmp)?;
        bail!("rollback_incompatible / 旧程序不兼容当前状态，禁止回滚")
    };
    if versioned(prefix) {
        publish_candidate(prefix, &tmp)?;
        fs::remove_file(&tmp)?;
    } else {
        fs::rename(tmp, prefix.join("bin/kairo"))?;
    }
    File::open(prefix.join("bin"))?.sync_all()?;
    Ok(())
}
pub fn uninstall(prefix: &Path, data: &Path) -> Result<()> {
    let _lease = lease(data)?;
    let _install = install_lease(prefix)?;
    for name in ["kairo", ".kairo.new", ".kairo.rollback"] {
        let p = prefix.join("bin").join(name);
        if p.exists() {
            fs::remove_file(p)?;
        }
    }
    let snap = prefix.join(".upgrade-snapshot");
    if snap.exists() {
        fs::remove_dir_all(snap)?;
    }
    if versioned(prefix) {
        if fs::symlink_metadata(prefix.join("current")).is_ok() {
            fs::remove_file(prefix.join("current"))?;
        }
        fs::remove_dir_all(prefix.join("releases"))?;
        if prefix.join("manager").exists() {
            fs::remove_dir_all(prefix.join("manager"))?;
        }
    }
    Ok(())
}
pub fn manifest_file(path: &Path) -> Result<Vec<u8>> {
    let bytes = fs::read(path)?;
    if bytes.len() > 1024 * 1024 {
        bail!("manifest_too_large / 更新清单过大")
    };
    Ok(bytes)
}
pub fn safe_prefix(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() || path == Path::new("/") {
        bail!("prefix_invalid / 安装路径无效")
    };
    directory(path)?;
    Ok(fs::canonicalize(path)?)
}
pub fn download(url: &str, dest: &Path) -> Result<()> {
    if std::env::var_os("KAIRO_OFFLINE").as_deref() == Some(std::ffi::OsStr::new("1")) {
        bail!("offline_mode / 离线模式禁止网络访问")
    }

    let allowed = |u: &reqwest::Url| {
        u.scheme() == "https"
            && u.username().is_empty()
            && u.password().is_none()
            && matches!(
                u.host_str(),
                Some(
                    "github.com"
                        | "objects.githubusercontent.com"
                        | "release-assets.githubusercontent.com"
                )
            )
    };
    let url = reqwest::Url::parse(url)?;
    if !allowed(&url) {
        bail!("download_host_denied / 下载地址不在允许的发布主机列表")
    }
    let client = reqwest::blocking::Client::builder()
        .https_only(true)
        .no_proxy()
        .timeout(std::time::Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() > 3 || !allowed(attempt.url()) {
                attempt.error("redirect denied")
            } else {
                attempt.follow()
            }
        }))
        .build()?;
    let mut r = client
        .get(url)
        .send()
        .map_err(|_| anyhow::anyhow!("download_failed / 下载失败"))?;
    if !r.status().is_success() {
        bail!("download_failed / 下载失败")
    };
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o700)
        .open(dest)?;
    let result = (|| -> Result<()> {
        use std::io::Read;
        let n = std::io::copy(&mut r.by_ref().take(104857601), &mut f)?;
        if n > 104857600 {
            bail!("download_too_large / 下载文件过大")
        };
        f.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(dest);
    }
    result
}
pub fn fetch(manifest: &Path, trusted: &str, prefix: &Path, data: &Path) -> Result<()> {
    validate_hash(trusted)?;
    if sha(manifest)? != trusted.to_ascii_lowercase() {
        bail!("manifest_checksum_mismatch / 发布清单校验失败")
    };
    let bytes = manifest_file(manifest)?;
    let value = check_manifest(&bytes)?;
    if value["update_available"] != true {
        bail!("no_newer_release / 没有较新的可用版本")
    };
    directory(prefix)?;
    let temp = prefix.join(format!(".download-{}", std::process::id()));
    download(
        value["url"]
            .as_str()
            .context("release_target_missing / 更新缺少当前架构")?,
        &temp,
    )?;
    let result = install(&temp, value["sha256"].as_str().unwrap(), prefix, data);
    let _ = fs::remove_file(temp);
    result
}

#[cfg(test)]
mod integrity_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "kairo-fixed-copy-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn fixed_bytes_are_verified_before_and_after_copy() {
        let f = Fixture::new();
        let source = f.0.join("source.txt");
        let dest = f.0.join("copy.txt");
        fs::write(&source, b"fixed harmless Kairo integrity fixture").unwrap();
        let expected = sha(&source).unwrap();
        copy_verified(&source, &dest, &expected, 0o600).unwrap();
        assert_eq!(fs::read(&source).unwrap(), fs::read(&dest).unwrap());
        assert_eq!(sha(&dest).unwrap(), expected);
    }
    #[test]
    fn fixed_hash_mismatch_never_creates_candidate() {
        let f = Fixture::new();
        let source = f.0.join("source.txt");
        let dest = f.0.join("copy.txt");
        fs::write(&source, b"fixed harmless different bytes").unwrap();
        let error = copy_verified(&source, &dest, &"0".repeat(64), 0o600).unwrap_err();
        assert!(error.to_string().contains("checksum_mismatch"));
        assert!(!dest.exists());
    }
    #[test]
    fn existing_destination_copy_failure_preserves_its_bytes() {
        let f = Fixture::new();
        let source = f.0.join("source.txt");
        let dest = f.0.join("copy.txt");
        fs::write(&source, b"fixed harmless source").unwrap();
        fs::write(&dest, b"existing fixed destination").unwrap();
        assert!(copy_verified(&source, &dest, &sha(&source).unwrap(), 0o600).is_err());
        assert_eq!(fs::read(&dest).unwrap(), b"existing fixed destination");
    }
    #[test]
    fn missing_source_copy_failure_creates_nothing() {
        let f = Fixture::new();
        let dest = f.0.join("copy.txt");
        assert!(copy_verified(&f.0.join("missing.txt"), &dest, &"0".repeat(64), 0o600).is_err());
        assert!(!dest.exists());
    }
}
