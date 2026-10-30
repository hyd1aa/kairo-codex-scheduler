use crate::{
    codex::AccountJournal,
    runtime::{self, Deployment, Runtime},
    Store,
};
use anyhow::{bail, Context, Result};
use kairo_core::{Cursor, State};
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
        "status" => {
            let r = runtime::runtime(store)?;
            let id = crate::identity(&r.codex_home.join("auth.json"))?;
            Ok(
                json!({"account_fingerprint":id.fingerprint,"oauth_present":true,"identity_server_verified":false}),
            )
        }
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
            if store.root.join("deployment.json").exists() {
                runtime::pause(store)?
            };
            store.write("runtime.json", &r)?;
            a.selected = Some(name.into());
            store.write("accounts.json", &a)?;
            Ok(json!({"selected":name,"execution":"observation_only"}))
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
    if store.root.join("deployment.json").exists() {
        runtime::pause(store)?
    };
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
    if store.root.join("accounts.json").exists() {
        let a = profiles(store)?;
        if a.profiles.values().any(|p| !p.is_absolute())
            || a.selected
                .as_ref()
                .is_some_and(|s| !a.profiles.contains_key(s))
        {
            bail!("profiles_invalid / 账号配置无效")
        }
    }

    if store.root.join("runtime.json").exists() {
        runtime::runtime(store)?;
    }
    if store.root.join("state.json").exists() {
        let state = store.state()?;
        crate::confirmation::read(store, &state)?;
    }
    if store.root.join("deployment.json").exists() {
        let d: Deployment = store.read("deployment.json")?;
        let s = store.state()?;
        let cursor: Cursor = store.read("cursor.json")?;
        if d.schema != 1
            || cursor.schema != 1
            || d.account != s.account
            || d.enabled && d.approved_at.is_none()
        {
            bail!("deployment_invalid / 部署授权记录无效")
        }
    }
    Ok(())
}
pub fn migrate(store: &Store) -> Result<()> {
    validate(store)?;
    if !store.root.join("runtime.json").exists() {
        store.write("runtime.json", &Runtime::default())?;
    }
    Ok(())
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transfer {
    pub schema: u32,
    pub source: String,
    pub state: State,
}
pub fn import(store: &Store, bundle: &Path, account: &str) -> Result<()> {
    runtime::ensure_legacy_inactive()?;
    let transfer: Transfer = serde_json::from_slice(&fs::read(bundle)?)?;
    import_transfer(store, transfer, account)
}
pub fn import_transfer(store: &Store, transfer: Transfer, account: &str) -> Result<()> {
    runtime::ensure_legacy_inactive()?;

    if transfer.schema != 1
        || transfer.source != "normalized-kairo-transfer"
        || transfer.state.account != account
        || account.len() != 64
    {
        bail!("transfer_invalid / 状态迁移包无效")
    };
    transfer.state.validate()?;
    let verified = runtime::verify(store)?;
    if verified["account_fingerprint"] != account {
        bail!("account_mismatch / 账号不匹配")
    }
    let current = if store.root.join("state.json").exists() {
        Some(store.state()?)
    } else {
        None
    };
    let mut state = merge_states(current, transfer.state)?;
    let now = chrono::Utc::now().timestamp();
    if state.last_clock > now {
        bail!("clock_backward / 时钟回拨")
    };
    state.pending = None;
    state.weekly = Some(serde_json::from_value(verified["weekly"].clone())?);
    state.server_denied = verified["server_denied"]
        .as_bool()
        .context("transfer_invalid / 状态迁移包无效")?;
    let five = serde_json::from_value(verified["five_hour"].clone())?;
    kairo_core::observe(&mut state, now, five);
    state.validate()?;
    let path = runtime::guard_root().join(account);
    let gate = Store::open(&path)?;
    let mut anchors = std::collections::BTreeSet::new();
    let mut latest = now;
    for a in &state.legacy_attempts {
        anchors.insert(a.anchor.clone());
        latest = latest.max(a.at);
    }
    for a in &state.attempts {
        anchors.insert(a.anchor.clone());
        latest = latest.max(a.at)
    }
    if path.join("account.json").exists() {
        let old: AccountJournal = gate.read("account.json")?;
        if old.schema != 1 || old.account != account {
            bail!("account_guard_invalid / 账号门禁无效")
        };
        anchors.extend(old.anchors);
        latest = latest.max(old.last_attempt);
    }
    gate.write(
        "account.json",
        &AccountJournal {
            schema: 1,
            account: account.into(),
            last_attempt: latest,
            anchors,
        },
    )?;
    store.write("state.json", &state)?;
    let mut cursor = Cursor::new(now, crate::timezone()?);
    cursor.arm(&store.config()?, crate::timezone()?, now)?;
    store.write("cursor.json", &cursor)?;
    store.write(
        "deployment.json",
        &Deployment {
            schema: 1,
            account: account.into(),
            enabled: false,
            approved_at: None,
        },
    )?;
    Ok(())
}
pub fn doctor(store: &Store, repair: bool) -> Result<Value> {
    if repair {
        migrate(store)?;
    }
    let r = runtime::runtime(store)?;
    let codex = account_command(store, &r.codex)?
        .args(["exec", "--help"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            let h = String::from_utf8_lossy(&o.stdout);
            [
                "--ignore-user-config",
                "--sandbox",
                "--ephemeral",
                "--skip-git-repo-check",
            ]
            .iter()
            .all(|f| h.contains(f))
        })
        .unwrap_or(false);
    let distro = fs::read_to_string("/etc/os-release").unwrap_or_default();
    let read = |key: &str| {
        distro
            .lines()
            .find_map(|l| {
                l.strip_prefix(&format!("{key}="))
                    .map(|s| s.trim_matches('"').to_string())
            })
            .unwrap_or_default()
    };
    let recovery = crate::recovery::inspect(store, &runtime::guard_root())?;
    Ok(
        json!({"distribution":read("ID"),"distribution_version":read("VERSION_ID"),"arch":std::env::consts::ARCH,"systemd":Path::new("/run/systemd/system").exists(),"timezone":crate::timezone().map(|t|t.to_string()).unwrap_or_else(|_|"timezone_unknown".into()),"codex_safety_flags":codex,"oauth_present":crate::identity(&r.codex_home.join("auth.json")).is_ok(),"state_valid":recovery["scheduler_state_ready"],"existing_records_valid":validate(store).is_ok(),"scheduler_state_ready":recovery["scheduler_state_ready"],"production_authorized":recovery["production_authorized"],"recovery_status":recovery,"repair_applied":repair}),
    )
}

pub fn merge_states(current: Option<State>, mut incoming: State) -> Result<State> {
    incoming.validate()?;
    if let Some(old) = current {
        old.validate()?;
        if old.account != incoming.account {
            bail!("account_mismatch / 账号不匹配")
        }
        for attempt in old.attempts {
            if let Some(i) = incoming
                .attempts
                .iter()
                .position(|x| x.anchor == attempt.anchor)
            {
                if incoming.attempts[i].at != attempt.at {
                    bail!("transfer_conflict / 状态迁移记录冲突")
                };
                incoming.attempts[i] = attempt;
            } else {
                incoming.attempts.push(attempt)
            }
        }
        for attempt in old.legacy_attempts {
            if let Some(existing) = incoming
                .legacy_attempts
                .iter()
                .find(|a| a.anchor == attempt.anchor)
            {
                if existing.at != attempt.at {
                    bail!("transfer_conflict / 状态迁移记录冲突")
                }
            } else {
                incoming.legacy_attempts.push(attempt)
            }
        }
        incoming.last_clock = incoming.last_clock.max(old.last_clock);
        incoming.positive_until = incoming.positive_until.max(old.positive_until);
        incoming.observations.extend(old.observations);
        incoming.observations.sort_by_key(|o| o.at);
        incoming.observations.dedup_by_key(|o| o.at);
        if incoming.observations.len() > 64 {
            incoming
                .observations
                .drain(..incoming.observations.len() - 64);
        }
    }
    incoming.pending = None;
    incoming.attempts.sort_by_key(|a| a.at);
    incoming.validate()?;
    Ok(incoming)
}
pub fn repair_defaults(store: &Store) -> Result<()> {
    if store.config().is_err() {
        let saved = store
            .read::<kairo_core::Config>(".config.json.previous")
            .ok()
            .filter(|c| c.validate().is_ok())
            .unwrap_or_default();
        store.write("config.json", &saved)?;
    }
    if runtime::runtime(store).is_err() {
        if store.root.join("deployment.json").exists() {
            runtime::pause(store)?
        };
        store.write("runtime.json", &Runtime::default())?;
    }
    validate(store)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state() -> State {
        State {
            schema: 1,
            account: "synthetic".into(),
            ..Default::default()
        }
    }
    #[test]
    fn migration_preserves_all_reservations() {
        let mut old = state();
        kairo_core::reserve(
            &mut old,
            "old",
            100000,
            kairo_core::Quota {
                duration: 18000,
                used: 0.,
                reset_at: 118000,
                remaining: 18000,
            },
        );
        old.positive_until = 120000;
        let mut new = state();
        new.pending = Some(140000);
        let merged = merge_states(Some(old), new).unwrap();
        assert_eq!(merged.attempts.len(), 1);
        assert_eq!(merged.attempts[0].outcome, "attempt_persisted_unknown");
        assert_eq!(merged.positive_until, 120000);
        assert!(merged.pending.is_none());
    }
    #[test]
    fn migration_rejects_account_change() {
        let mut other = state();
        other.account = "other".into();
        assert!(merge_states(Some(state()), other).is_err());
    }
}
