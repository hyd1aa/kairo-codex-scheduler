use crate::{
    runtime::{self, Runtime, ScheduleState},
    Store,
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
#[derive(Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Accounts {
    pub selected: Option<String>,
    pub profiles: std::collections::BTreeMap<String, PathBuf>,
}
pub fn profiles(store: &Store) -> Result<Accounts> {
    if store.root.join("accounts.json").exists() {
        store.read("accounts.json")
    } else {
        Ok(Accounts::default())
    }
}
pub fn account_action(
    store: &Store,
    action: &str,
    name: Option<&str>,
    home: Option<&Path>,
) -> Result<Value> {
    match action {
        "status" => runtime::verify(store),
        "verify" => runtime::verify(store),
        "list" => Ok(serde_json::to_value(profiles(store)?)?),
        "add" => {
            let name = name.context("profile_name_required / 需要账号名称")?;
            if name.is_empty()
                || name.len() > 64
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            {
                bail!("profile_name_invalid / 账号名称只能包含字母、数字、下划线和连字符")
            };
            let home = fs::canonicalize(home.context("profile_path_required / 需要认证目录路径")?)?;
            crate::identity(&home.join("auth.json"))?;
            let mut a = profiles(store)?;
            if a.profiles.contains_key(name) {
                bail!("profile_exists / 账号名称已存在")
            };
            a.profiles.insert(name.into(), home);
            store.write("accounts.json", &a)?;
            Ok(serde_json::to_value(a)?)
        }
        "select" => {
            let name = name.context("profile_name_required / 需要账号名称")?;
            let mut a = profiles(store)?;
            let home = a
                .profiles
                .get(name)
                .context("profile_missing / 账号不存在")?
                .clone();
            let mut r = runtime::runtime(store)?;
            r.codex_home = home;
            r.validate()?;

            store.write("runtime.json", &r)?;
            a.selected = Some(name.into());
            store.write("accounts.json", &a)?;
            Ok(json!({"selected":name,"execution":"scheduled"}))
        }
        "remove" => {
            let name = name.context("profile_name_required / 需要账号名称")?;
            let mut a = profiles(store)?;
            if a.selected.as_deref() == Some(name) {
                bail!("profile_selected / 不可删除正在使用的账号")
            };
            if a.profiles.remove(name).is_none() {
                bail!("profile_missing / 账号不存在")
            };
            store.write("accounts.json", &a)?;
            Ok(serde_json::to_value(a)?)
        }
        _ => bail!("auth_action_invalid / 账号操作无效"),
    }
}
pub(crate) fn account_command(store: &Store, binary: &Path) -> Result<Command> {
    use std::os::unix::fs::MetadataExt;
    let owner = fs::metadata(&store.root)?.uid();
    let command = if unsafe { libc::geteuid() } == 0 && owner != 0 {
        let user = unsafe { libc::getpwuid(owner) };
        if user.is_null() {
            bail!("runtime_invalid / 运行配置无效")
        }
        let name = unsafe { std::ffi::CStr::from_ptr((*user).pw_name) }.to_str()?;
        let mut c = Command::new("/usr/sbin/runuser");
        c.args(["-u", name, "--"]).arg(binary);
        c
    } else {
        Command::new(binary)
    };
    Ok(command)
}
pub fn login(store: &Store) -> Result<()> {
    let r = runtime::runtime(store)?;
    let mut command = account_command(store, &r.codex)?;
    let status = command
        .args(["login", "--device-auth"])
        .env("HOME", r.home)
        .env("CODEX_HOME", r.codex_home)
        .env_remove("OPENAI_API_KEY")
        .env_remove("OPENAI_BASE_URL")
        .status()?;
    if !status.success() {
        bail!("oauth_login_failed / OAuth 登录未完成")
    };
    Ok(())
}
pub fn service(action: &str) -> Result<Value> {
    if !["start", "stop", "restart", "status", "enable", "disable"].contains(&action) {
        bail!("service_action_invalid / 服务操作无效")
    };
    // Fixed unit name: never dispatch a user-supplied service name.
    let args = if action == "status" {
        vec![
            "show",
            "kairo.service",
            "--property=ActiveState",
            "--property=LoadState",
            "--property=MainPID",
        ]
    } else {
        vec![action, "kairo.service"]
    };
    let o = Command::new("systemctl").args(args).output()?;
    if !o.status.success() {
        bail!("service_failed / Kairo 服务操作失败；请检查权限与安装状态")
    };
    let mut status = serde_json::Map::new();
    for line in String::from_utf8_lossy(&o.stdout).lines() {
        if let Some((key, value)) = line.split_once('=') {
            let key = match key {
                "ActiveState" => "active_state",
                "LoadState" => "load_state",
                "MainPID" => "main_pid",
                _ => continue,
            };
            status.insert(key.into(), Value::String(value.into()));
        }
    }
    Ok(json!({"service":"kairo.service","action":action,"status":status,"success":true}))
}

pub fn validate(store: &Store) -> Result<()> {
    store.config()?;
    if let Some(r) = store.retained::<Runtime>("runtime.json")? {
        r.validate()?;
    }
    if crate::present(&store.root.join("schedule.json"))? {
        ScheduleState::load(store)?;
    }
    Ok(())
}
pub fn migrate(store: &Store) -> Result<()> {
    validate(store)?;
    if !crate::present(&store.root.join("runtime.json"))? {
        use std::os::unix::fs::MetadataExt;
        let owner = fs::metadata(&store.root)?.uid();
        let retained = store.retained::<Runtime>("runtime.json")?;
        let missing = retained.is_none();
        let mut r = retained.unwrap_or_default();
        // Existing paths belong to the installation, not the upgrader's environment.
        if missing
            && (fs::metadata(&r.home).map_or(true, |m| m.uid() != owner)
                || std::env::var_os("HOME").is_none())
        {
            let u = unsafe { libc::getpwuid(owner) };
            if u.is_null() {
                bail!("runtime_user_missing");
            }
            r.home = PathBuf::from(unsafe { std::ffi::CStr::from_ptr((*u).pw_dir) }.to_str()?);
            r.codex_home = r.home.join(".codex");
            r.codex = [
                r.home.join(".local/bin/codex"),
                PathBuf::from("/usr/local/bin/codex"),
                PathBuf::from("/usr/bin/codex"),
            ]
            .into_iter()
            .find(|p| p.is_file())
            .unwrap_or_else(|| PathBuf::from("/usr/local/bin/codex"));
        }
        store.write("runtime.json", &r)?;
    }
    for name in [
        "config.json",
        "state.json",
        "cursor.json",
        "deployment.json",
    ] {
        if !crate::present(&store.root.join(name))? {
            if let Some(v) = store.retained::<Value>(name).ok().flatten() {
                store.write(name, &v)?;
            }
        }
    }
    if !crate::present(&store.root.join("schedule.json"))? {
        ScheduleState::load(store)?.save(store)?;
    }
    Ok(())
}
pub fn doctor(store: &Store, repair: bool) -> Result<Value> {
    if repair {
        migrate(store)?;
    }
    let r = runtime::runtime(store)?;
    Ok(
        json!({"state_valid":validate(store).is_ok(),"oauth_present":crate::codex::logged_in(store,&r).unwrap_or(false),"codex":r.codex,"quota_optional":true,"deployment_approval_required":false,"recovery_wait_required":false}),
    )
}
pub fn repair_defaults(store: &Store) -> Result<()> {
    migrate(store)
}
