//! One-time handoff from v0.2.5's cached installer/root onboarding unit.
//! No OAuth or model execution: the user daemon owns all initialization afterwards.
use anyhow::{bail, Result};
use std::{
    fs,
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::Path,
};
fn present(path: &Path) -> Result<bool> {
    crate::recovery::present(path)
}
fn owned(path: &Path, owner: u32, directory: bool) -> Result<()> {
    let m = fs::symlink_metadata(path)?;
    if m.file_type().is_symlink()
        || m.uid() != owner
        || m.mode() & 0o022 != 0
        || if directory {
            !m.is_dir()
        } else {
            !m.is_file() || m.nlink() != 1
        }
    {
        bail!("legacy_service_unsafe / 旧服务文件权限或所属用户无法验证");
    }
    Ok(())
}
fn template(data: &Path, prefix: &Path) -> Result<String> {
    let user = unsafe { libc::getpwuid(fs::metadata(data)?.uid()) };
    if user.is_null() {
        bail!("runtime_user_missing / 服务所属用户不存在");
    }
    let name = unsafe { std::ffi::CStr::from_ptr((*user).pw_name) }
        .to_str()?
        .to_string();
    let home = unsafe { std::ffi::CStr::from_ptr((*user).pw_dir) }
        .to_str()?
        .to_string();
    let group = unsafe { libc::getgrgid((*user).pw_gid) };
    if group.is_null() {
        bail!("runtime_group_missing / 服务所属组不存在");
    }
    let group = unsafe { std::ffi::CStr::from_ptr((*group).gr_name) }.to_str()?;
    Ok(include_str!("../../../packaging/kairo.service")
        .replace("@USER@", &name)
        .replace("@GROUP@", group)
        .replace("@HOME@", &home)
        .replace("@PREFIX@", &prefix.to_string_lossy()))
}
fn install_helper(prefix: &Path, name: &str, body: &str, owner: u32) -> Result<()> {
    let dir = prefix.join("installer");
    owned(&dir, owner, true)?;
    let target = dir.join(name);
    if present(&target)? {
        owned(&target, owner, false)?;
    }
    let temp = dir.join(format!(".{name}.handoff"));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o755)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&temp)?;
    let m = file.metadata()?;
    if !m.is_file() || m.uid() != owner || m.nlink() != 1 || m.mode() & 0o022 != 0 {
        bail!("legacy_helper_unsafe / 更新辅助文件不安全");
    }
    file.set_len(0)?;
    file.write_all(body.as_bytes())?;
    file.set_permissions(fs::Permissions::from_mode(0o755))?;
    file.sync_all()?;
    fs::rename(temp, target)?;
    fs::File::open(dir)?.sync_all()?;
    Ok(())
}
fn handoff(
    prefix: &Path,
    units: &Path,
    marker: &Path,
    data: &Path,
    owner: u32,
    mut system: impl FnMut(&[&str]) -> Result<()>,
) -> Result<bool> {
    let tasks = [
        (
            "kairo-onboarding.timer",
            include_str!("../../../packaging/kairo-onboarding.timer"),
        ),
        (
            "kairo-onboarding.service",
            include_str!("../../../packaging/kairo-onboarding.service"),
        ),
    ];
    if !tasks.iter().any(|(name, _)| units.join(name).exists()) {
        return Ok(false);
    }
    for dir in [prefix, units, marker.parent().unwrap()] {
        owned(dir, owner, true)?;
    }
    owned(marker, owner, false)?;
    if fs::read_to_string(marker)? != "schema=1\n" {
        bail!("managed_marker_invalid / 安装标记无效");
    }
    let lockpath = marker.parent().unwrap().join("online.lock");
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&lockpath)?;
    owned(&lockpath, owner, false)?;
    fs2::FileExt::lock_exclusive(&lock)?;
    if !tasks.iter().any(|(name, _)| units.join(name).exists()) {
        return Ok(false);
    }
    let unit = units.join("kairo.service");
    owned(&unit, owner, false)?;
    if fs::read_to_string(unit)? != template(data, prefix)? {
        bail!("custom_service_unit / 保留自定义服务配置，请检查诊断");
    }
    let drop = units.join("kairo.service.d");
    let mode = drop.join("20-mode.conf");
    if present(&drop)? {
        owned(&drop, owner, true)?;
        for entry in fs::read_dir(&drop)? {
            if entry?.file_name() != "20-mode.conf" {
                bail!("custom_service_dropin / 保留自定义服务配置");
            }
        }
    }
    let observing = present(&mode)?;
    if observing {
        owned(&mode, owner, false)?;
        let expected=format!("[Service]\nExecStart=\nExecStart={}/current/bin/kairo --data-dir /var/lib/kairo daemon --observe\n",prefix.display());
        if fs::read_to_string(&mode)? != expected {
            bail!("custom_service_dropin / 保留自定义服务配置");
        }
    }
    for (name, body) in tasks {
        let p = units.join(name);
        if present(&p)? {
            owned(&p, owner, false)?;
            if fs::read_to_string(p)? != body {
                bail!("custom_initialization_unit / 保留自定义初始化任务");
            }
        }
    }
    install_helper(
        prefix,
        "install.sh",
        include_str!("../../../install.sh"),
        owner,
    )?;
    install_helper(
        prefix,
        "system-uninstall.sh",
        include_str!("../../../uninstall.sh"),
        owner,
    )?;
    system(&["disable", "--now", "kairo-onboarding.timer"])?;
    for (name, _) in tasks {
        let p = units.join(name);
        if present(&p)? {
            fs::remove_file(p)?;
        }
    }
    if observing {
        fs::remove_file(mode)?;
    }
    fs::File::open(units)?.sync_all()?;
    system(&["daemon-reload"])?;
    system(&["enable", "--now", "kairo.service"])?;
    if observing {
        system(&["restart", "kairo.service"])?;
    }
    Ok(true)
}
pub(crate) fn retire(root: &Path) -> Result<bool> {
    if unsafe { libc::geteuid() } != 0
        || root != Path::new("/var/lib/kairo")
        || std::env::var_os("KAIRO_OFFLINE").as_deref() == Some(std::ffi::OsStr::new("1"))
    {
        return Ok(false);
    }
    if fs::canonicalize(root)? != root {
        bail!("managed_root_required / 服务目录不安全");
    }
    handoff(
        Path::new("/opt/kairo"),
        Path::new("/etc/systemd/system"),
        Path::new("/var/lib/kairo-installer/managed-user"),
        root,
        0,
        |args| {
            let result = crate::process::output(
                std::process::Command::new("/usr/bin/systemctl").args(args),
                30,
                65536,
            )?;
            if !result.status.success() {
                bail!("legacy_service_handoff_failed / 旧服务自动迁移失败，请检查 systemd");
            }
            Ok(())
        },
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_installer_handoff_retires_only_exact_legacy_units_and_is_idempotent() {
        let root =
            std::env::temp_dir().join(format!("kairo-service-handoff-{}", std::process::id()));
        let prefix = root.join("program");
        let units = root.join("units");
        let marker = root.join("stamp/managed-user");
        let data = root.join("data");
        for dir in [
            &prefix,
            &units,
            marker.parent().unwrap(),
            &data,
            &prefix.join("installer"),
            &units.join("kairo.service.d"),
        ] {
            fs::create_dir_all(dir).unwrap();
            fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).unwrap();
        }
        fs::write(&marker, "schema=1\n").unwrap();
        fs::write(
            units.join("kairo.service"),
            template(&data, &prefix).unwrap(),
        )
        .unwrap();
        fs::write(
            units.join("kairo-onboarding.timer"),
            include_str!("../../../packaging/kairo-onboarding.timer"),
        )
        .unwrap();
        fs::write(
            units.join("kairo-onboarding.service"),
            include_str!("../../../packaging/kairo-onboarding.service"),
        )
        .unwrap();
        fs::write(units.join("kairo.service.d/20-mode.conf"),format!("[Service]\nExecStart=\nExecStart={}/current/bin/kairo --data-dir /var/lib/kairo daemon --observe\n",prefix.display())).unwrap();
        for name in [
            "kairo.service",
            "kairo-onboarding.timer",
            "kairo-onboarding.service",
            "kairo.service.d/20-mode.conf",
        ] {
            fs::set_permissions(units.join(name), fs::Permissions::from_mode(0o644)).unwrap();
        }
        fs::set_permissions(&marker, fs::Permissions::from_mode(0o600)).unwrap();
        let mut calls = Vec::new();
        let owner = unsafe { libc::geteuid() };
        assert!(handoff(&prefix, &units, &marker, &data, owner, |args| {
            calls.push(args.join(" "));
            Ok(())
        })
        .unwrap());
        assert!(calls.contains(&"restart kairo.service".to_string()));
        assert!(!units.join("kairo-onboarding.timer").exists());
        assert!(!units.join("kairo.service.d/20-mode.conf").exists());
        assert_eq!(
            fs::read_to_string(prefix.join("installer/install.sh")).unwrap(),
            include_str!("../../../install.sh")
        );
        assert!(!handoff(&prefix, &units, &marker, &data, owner, |_| panic!(
            "must not repeat system changes"
        ))
        .unwrap());
        fs::write(
            units.join("kairo-onboarding.timer"),
            include_str!("../../../packaging/kairo-onboarding.timer"),
        )
        .unwrap();
        fs::set_permissions(
            units.join("kairo-onboarding.timer"),
            fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        fs::write(units.join("kairo.service"), "unrelated custom unit").unwrap();
        assert!(handoff(&prefix, &units, &marker, &data, owner, |_| panic!(
            "must preserve custom services"
        ))
        .is_err());
        assert_eq!(
            fs::read_to_string(units.join("kairo.service")).unwrap(),
            "unrelated custom unit"
        );
        fs::remove_dir_all(root).unwrap();
    }
}
