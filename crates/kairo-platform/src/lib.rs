use anyhow::{bail, Context, Result};
use fs2::FileExt;
use kairo_core::{Config, Quota, State};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};
pub struct Store {
    pub root: PathBuf,
    _lock: File,
}
impl Store {
    pub fn open(root: &Path) -> Result<Self> {
        let absolute = std::path::absolute(root)?;
        if absolute
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
            || [
                "/",
                "/tmp",
                "/opt",
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
            bail!("private_directory_required / 必须指定独立的私有数据目录")
        }
        let existed = root.exists();
        fs::create_dir_all(root)?;
        if !existed && unsafe { libc::geteuid() } == 0 {
            if let Some(parent) = root.parent() {
                let owner = fs::metadata(parent)?;
                let path = std::ffi::CString::new(root.as_os_str().as_encoded_bytes())?;
                if unsafe { libc::chown(path.as_ptr(), owner.uid(), owner.gid()) } != 0 {
                    return Err(std::io::Error::last_os_error().into());
                }
            }
        }
        let m = fs::symlink_metadata(root)?;
        if !m.is_dir()
            || m.file_type().is_symlink()
            || (unsafe { libc::geteuid() } != 0 && m.uid() != unsafe { libc::geteuid() })
        {
            bail!("Unsafe directory / 不安全的目录")
        }
        fs::set_permissions(root, fs::Permissions::from_mode(0o700))?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(root.join("store.lock"))?;
        single_owner(&lock)?;
        preserve_owner(&lock, root)?;
        lock.lock_exclusive()?;
        Ok(Self {
            root: root.into(),
            _lock: lock,
        })
    }
    pub fn read<T: DeserializeOwned>(&self, name: &str) -> Result<T> {
        validate_name(name)?;
        Ok(serde_json::from_slice(&private_bytes(
            &self.root.join(name),
            8 * 1024 * 1024,
        )?)?)
    }
    pub fn write<T: Serialize>(&self, name: &str, v: &T) -> Result<()> {
        validate_name(name)?;
        let data = serde_json::to_vec_pretty(v)?;
        let dst = self.root.join(name);
        let tmp = self.root.join(format!(".{name}.tmp"));
        let mut f = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&tmp)?;
        single_owner(&f)?;
        preserve_owner(&f, &self.root)?;
        f.set_len(0)?;
        f.write_all(&data)?;
        f.sync_all()?;
        if dst.exists() {
            let old = private_bytes(&dst, 8 * 1024 * 1024)?;
            let snap = self.root.join(format!(".{name}.previous"));
            let mut sf = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(false)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW)
                .open(snap)?;
            single_owner(&sf)?;
            preserve_owner(&sf, &self.root)?;
            sf.set_len(0)?;
            sf.write_all(&old)?;
            sf.sync_all()?;
        }
        fs::rename(tmp, dst)?;
        File::open(&self.root)?.sync_all()?;
        Ok(())
    }
    pub fn config(&self) -> Result<Config> {
        let c: Config = self.read("config.json")?;
        c.validate()?;
        Ok(c)
    }
    pub fn state(&self) -> Result<State> {
        let s: State = self
            .read("state.json")
            .context("State missing/corrupt: execution blocked / 状态丢失或损坏，执行已阻止")?;
        s.validate()?;
        Ok(s)
    }
}
fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 128
        || name == "."
        || name == ".."
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    {
        bail!("state_name_invalid / 状态文件名称无效")
    }
    Ok(())
}
/// Root administration preserves the service user's ownership and private modes.
pub(crate) fn preserve_owner(file: &File, directory: &Path) -> Result<()> {
    use std::os::fd::AsRawFd;
    if unsafe { libc::geteuid() } == 0 {
        let m = fs::metadata(directory)?;
        if unsafe { libc::fchown(file.as_raw_fd(), m.uid(), m.gid()) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    Ok(())
}
fn single_owner(file: &File) -> Result<()> {
    let m = file.metadata()?;
    if !m.is_file()
        || m.nlink() != 1
        || (unsafe { libc::geteuid() } != 0 && m.uid() != unsafe { libc::geteuid() })
    {
        bail!("unsafe_state_file / 状态文件类型、所有权或硬链接不安全")
    };
    Ok(())
}
pub fn private_bytes(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let f = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    single_owner(&f)?;
    if f.metadata()?.permissions().mode() & 0o077 != 0 {
        bail!("private_permissions_required / 私有文件必须仅允许当前用户访问")
    };
    let mut bytes = Vec::new();
    f.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        bail!("state_too_large / 状态文件过大")
    };
    Ok(bytes)
}
pub fn timezone() -> Result<chrono_tz::Tz> {
    let mut candidates = Vec::new();
    // A stalled D-Bus activation must not hold the store lock indefinitely.
    if let Ok(o) = crate::process::output(
        std::process::Command::new("timedatectl").args(["show", "--property=Timezone", "--value"]),
        2,
        4096,
    ) {
        if o.status.success() {
            candidates.push(String::from_utf8_lossy(&o.stdout).trim().to_string());
        }
    }
    if let Ok(p) = fs::read_link("/etc/localtime") {
        if let Some((_, s)) = p.to_string_lossy().split_once("zoneinfo/") {
            candidates.push(s.to_string());
        }
    }
    if let Ok(s) = fs::read_to_string("/etc/timezone") {
        let name = s.trim();
        let valid = name.parse::<chrono_tz::Tz>().is_ok();
        if valid
            && fs::read("/etc/localtime")
                .map(|local| {
                    fs::read(Path::new("/usr/share/zoneinfo").join(name))
                        .is_ok_and(|zone| zone == local)
                })
                .unwrap_or(true)
        {
            candidates.push(name.into());
        }
    }
    for s in candidates {
        if let Ok(t) = s.parse() {
            return Ok(t);
        }
    }
    // Copied /etc/localtime is common; compare standard zoneinfo content.
    if let Ok(local) = fs::read("/etc/localtime") {
        if let Ok(tab) = fs::read_to_string("/usr/share/zoneinfo/zone1970.tab") {
            for line in tab.lines().filter(|l| !l.starts_with('#')) {
                if let Some(zone) = line.split('\t').nth(2) {
                    if fs::read(Path::new("/usr/share/zoneinfo").join(zone))
                        .ok()
                        .as_ref()
                        == Some(&local)
                    {
                        return Ok(zone.parse()?);
                    }
                }
            }
        }
        if fs::read("/usr/share/zoneinfo/Etc/UTC").ok().as_ref() == Some(&local) {
            return Ok(chrono_tz::UTC);
        }
    }
    bail!("Unknown system timezone; scheduling blocked / 无法识别系统时区，调度已暂停")
}
#[derive(Serialize)]
pub struct Identity {
    pub fingerprint: String,
    #[serde(skip)]
    pub token: String,
    #[serde(skip)]
    pub account_id: String,
}
pub fn identity(path: &Path) -> Result<Identity> {
    use sha2::{Digest, Sha256};
    let v: Value = serde_json::from_slice(
        &private_bytes(path, 1024 * 1024).context("OAuth unavailable / OAuth 未授权")?,
    )?;
    if v.get("OPENAI_API_KEY").is_some_and(|v| !v.is_null()) {
        bail!("ChatGPT OAuth required / 需要 ChatGPT OAuth")
    };
    let t = &v["tokens"];
    let account = t["account_id"]
        .as_str()
        .filter(|s| !s.is_empty())
        .context("OAuth account missing / OAuth 账号缺失")?;
    let token = t["access_token"]
        .as_str()
        .filter(|s| !s.is_empty())
        .context("OAuth token missing / OAuth 令牌缺失")?;
    Ok(Identity {
        fingerprint: format!("{:x}", Sha256::digest(account.as_bytes())),
        token: token.into(),
        account_id: account.into(),
    })
}
pub fn parse_usage(v: &Value, now: i64) -> Result<(Quota, Quota, bool)> {
    let r = &v["rate_limit"];
    let allowed = r["allowed"]
        .as_bool()
        .context("Quota allowed flag missing / 缺少额度许可标志")?;
    let reached = r["limit_reached"]
        .as_bool()
        .context("Quota limit flag missing / 缺少额度限制标志")?;
    let denied = !allowed || reached;
    let mut five = None;
    let mut week = None;
    for key in ["primary_window", "secondary_window"] {
        let w = &r[key];
        if w.is_null() {
            continue;
        }
        let duration = w["limit_window_seconds"]
            .as_i64()
            .context("Quota duration missing / 额度时长缺失")?;
        let q = Quota {
            duration,
            used: w["used_percent"]
                .as_f64()
                .context("Quota usage missing / 额度用量缺失")?,
            reset_at: w["reset_at"]
                .as_i64()
                .context("Reset missing / 重置时间缺失")?,
            remaining: w["reset_after_seconds"]
                .as_i64()
                .context("Countdown missing / 倒计时缺失")?,
        };
        if !q.valid(now, duration) {
            bail!("Invalid quota / 额度数据无效")
        }
        let slot = match duration {
            18000 => &mut five,
            604800 => &mut week,
            _ => continue,
        };
        if slot.replace(q).is_some() {
            bail!("Duplicate window / 窗口重复")
        }
    }
    Ok((
        five.context("Five-hour quota unavailable / 无五小时额度")?,
        week.context("Weekly quota unavailable / 无每周额度")?,
        denied,
    ))
}
pub fn quota(id: &Identity) -> Result<Value> {
    if std::env::var_os("KAIRO_OFFLINE").as_deref() == Some(std::ffi::OsStr::new("1")) {
        bail!("offline_mode / 离线模式禁止网络访问")
    }

    let client = reqwest::blocking::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .timeout(Duration::from_secs(20))
        .build()?;
    let mut response = client
        .get("https://chatgpt.com/backend-api/wham/usage")
        .bearer_auth(&id.token)
        .header("ChatGPT-Account-Id", &id.account_id)
        .send()
        .map_err(|_| anyhow::anyhow!("Quota transport failed / 额度网络请求失败"))?;
    if !response.status().is_success() {
        bail!("Quota HTTP {} / 额度请求被拒绝", response.status())
    }
    let mut bytes = Vec::new();
    use std::io::Read;
    response.by_ref().take(1048577).read_to_end(&mut bytes)?;
    if bytes.len() > 1048576 {
        bail!("Response too large / 响应过大")
    }
    Ok(serde_json::from_slice(&bytes)?)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn store_roundtrip() {
        let p = std::env::temp_dir().join(format!("kairo-test-{}", std::process::id()));
        let s = Store::open(&p).unwrap();
        s.write("config.json", &Config::default()).unwrap();
        assert_eq!(s.config().unwrap().anchors.len(), 4);
        assert!(s.state().is_err());
        s.write("config.json", &Config::default()).unwrap();
        assert!(p.join(".config.json.previous").exists());
        drop(s);
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn malformed_quota() {
        assert!(parse_usage(&serde_json::json!({}), 0).is_err());
    }
}

pub mod engine;

pub mod codex;
pub mod confirmation;
#[cfg(test)]
mod usage_tests {
    use super::*;
    fn fixture() -> Value {
        let w = |d| serde_json::json!({"limit_window_seconds":d,"used_percent":0,"reset_at":100000+d,"reset_after_seconds":d});
        serde_json::json!({"rate_limit":{"allowed":true,"limit_reached":false,"primary_window":w(604800),"secondary_window":w(18000)}})
    }
    #[test]
    fn window_names_do_not_define_duration() {
        let (f, w, d) = parse_usage(&fixture(), 100000).unwrap();
        assert_eq!(f.duration, 18000);
        assert_eq!(w.duration, 604800);
        assert!(!d);
    }
    #[test]
    fn missing_weekly_or_flags_blocks() {
        let mut v = fixture();
        v["rate_limit"]["primary_window"] = Value::Null;
        assert!(parse_usage(&v, 100000).is_err());
        let mut v = fixture();
        v["rate_limit"].as_object_mut().unwrap().remove("allowed");
        assert!(parse_usage(&v, 100000).is_err());
    }
    #[test]
    fn server_denial_preserved() {
        let mut v = fixture();
        v["rate_limit"]["allowed"] = false.into();
        assert!(parse_usage(&v, 100000).unwrap().2);
    }
    #[test]
    fn strict_countdown_and_duration() {
        let mut v = fixture();
        v["rate_limit"]["secondary_window"]["limit_window_seconds"] = 17999.into();
        assert!(parse_usage(&v, 100000).is_err());
        let mut v = fixture();
        v["rate_limit"]["secondary_window"]["reset_after_seconds"] = 100.into();
        assert!(parse_usage(&v, 100000).is_err());
    }
}

pub mod activation;
pub mod recovery;
pub mod runtime;

pub mod update;

pub mod management;

pub mod replay;

pub mod process;
#[cfg(test)]
mod storage_security_tests {
    use super::*;
    use std::os::unix::fs::symlink;
    #[test]
    fn protected_directory_and_lock_symlink() {
        assert!(Store::open(Path::new("/tmp")).is_err());
        let p = std::env::temp_dir().join(format!("kairo-link-{}", std::process::id()));
        fs::create_dir_all(&p).unwrap();
        fs::write(p.join("target"), b"untouched").unwrap();
        symlink(p.join("target"), p.join("store.lock")).unwrap();
        assert!(Store::open(&p).is_err());
        assert_eq!(fs::read(p.join("target")).unwrap(), b"untouched");
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn temporary_state_symlink_preserves_fixed_target() {
        let p = std::env::temp_dir().join(format!("kairo-write-link-{}", std::process::id()));
        let store = Store::open(&p).unwrap();
        let target = p.join("fixed-target");
        fs::write(&target, b"untouched").unwrap();
        symlink(&target, p.join(".config.json.tmp")).unwrap();
        assert!(store
            .write("config.json", &serde_json::json!({"generation": 1}))
            .is_err());
        assert_eq!(fs::read(&target).unwrap(), b"untouched");
        assert!(!p.join("config.json").exists());
        drop(store);
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn previous_state_symlink_preserves_fixed_target_and_config() {
        let p = std::env::temp_dir().join(format!("kairo-previous-link-{}", std::process::id()));
        let store = Store::open(&p).unwrap();
        let original = serde_json::json!({"generation": 1});
        store.write("config.json", &original).unwrap();
        let target = p.join("fixed-target");
        fs::write(&target, b"untouched").unwrap();
        symlink(&target, p.join(".config.json.previous")).unwrap();
        assert!(store
            .write("config.json", &serde_json::json!({"generation": 2}))
            .is_err());
        assert_eq!(fs::read(&target).unwrap(), b"untouched");
        assert_eq!(
            store.read::<serde_json::Value>("config.json").unwrap(),
            original
        );
        drop(store);
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn http_scheme_rejected_before_transport() {
        let client = reqwest::blocking::Client::builder()
            .https_only(true)
            .no_proxy()
            .build()
            .unwrap();
        assert!(client.get("http://127.0.0.1:1/usage").send().is_err());
    }
}

#[cfg(test)]
mod hardlink_tests {
    use super::*;
    #[test]
    fn temporary_hardlink_cannot_truncate_another_file() {
        let root = std::env::temp_dir().join(format!("kairo-hardlink-{}", std::process::id()));
        let store = Store::open(&root).unwrap();
        let innocent = root.join("innocent");
        fs::write(&innocent, b"unchanged").unwrap();
        fs::hard_link(&innocent, root.join(".config.json.tmp")).unwrap();
        assert!(store
            .write("config.json", &kairo_core::Config::default())
            .is_err());
        assert_eq!(fs::read(innocent).unwrap(), b"unchanged");
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }
}

pub mod legacy;

pub mod guardian;

#[cfg(test)]
mod administration_tests {
    use super::*;
    #[test]
    fn root_management_preserves_service_owner_and_snapshot_hardlinks_do_not_truncate() {
        let root = std::env::temp_dir().join(format!("kairo-admin-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        if unsafe { libc::geteuid() } == 0 {
            let name = std::ffi::CString::new(root.as_os_str().as_encoded_bytes()).unwrap();
            assert_eq!(unsafe { libc::chown(name.as_ptr(), 65534, 65534) }, 0);
        }
        let child = Store::open(&root.join("account")).unwrap();
        assert_eq!(
            fs::metadata(&child.root).unwrap().uid(),
            fs::metadata(&root).unwrap().uid()
        );
        drop(child);
        let s = Store::open(&root).unwrap();
        s.write("config.json", &Config::default()).unwrap();
        assert_eq!(
            fs::metadata(root.join("config.json")).unwrap().uid(),
            fs::metadata(&root).unwrap().uid()
        );
        let sentinel = root.join("sentinel");
        fs::write(&sentinel, b"do not truncate").unwrap();
        fs::hard_link(&sentinel, root.join(".config.json.previous")).unwrap();
        assert!(s.write("config.json", &Config::default()).is_err());
        assert_eq!(fs::read(&sentinel).unwrap(), b"do not truncate");
        drop(s);
        fs::remove_dir_all(root).unwrap();
    }
}
