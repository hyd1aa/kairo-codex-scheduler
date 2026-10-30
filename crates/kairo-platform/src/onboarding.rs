//! Durable installation intent. The privileged timer coordinates existing safety gates;
//! it never executes a model or infers a fresh account from missing history.
use crate::{recovery, runtime as rt, Store};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Intent {
    pub schema: u32,
    pub requested: bool,
    pub exclusive_account: bool,
    pub account: Option<String>,
    pub next_attempt: i64,
    pub failures: u32,
}
impl Intent {
    fn validate(&self) -> Result<()> {
        if self.schema != 1
            || self.next_attempt < 0
            || self
                .account
                .as_ref()
                .is_some_and(|a| a.len() != 64 || !a.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            bail!("onboarding_intent_invalid / 自动初始化授权记录无效");
        }
        Ok(())
    }
}
pub fn request(store: &Store, exclusive: bool) -> Result<Value> {
    let mut intent = if recovery::present(&store.root.join("onboarding.json"))? {
        let i: Intent = store.read("onboarding.json")?;
        i.validate()?;
        i
    } else {
        Intent {
            schema: 1,
            requested: true,
            exclusive_account: false,
            account: None,
            next_attempt: 0,
            failures: 0,
        }
    };
    let id = rt::runtime(store)
        .and_then(|r| crate::identity(&r.codex_home.join("auth.json")))
        .ok();
    // A different account always requires a new one-time risk confirmation.
    if let Some(id) = id {
        if intent
            .account
            .as_ref()
            .is_some_and(|a| a != &id.fingerprint)
        {
            intent.exclusive_account = false;
        }
        intent.account = Some(id.fingerprint);
    }
    intent.requested = true;
    intent.exclusive_account |= exclusive;
    intent.next_attempt = 0;
    store.write("onboarding.json", &intent)?;
    Ok(
        json!({"automatic_initialization_requested":true,"exclusive_account_confirmed":intent.exclusive_account}),
    )
}
pub fn pause(store: &Store) -> Result<()> {
    if recovery::present(&store.root.join("onboarding.json"))? {
        let mut i: Intent = store.read("onboarding.json")?;
        i.validate()?;
        i.requested = false;
        store.write("onboarding.json", &i)?;
    }
    Ok(())
}
#[derive(Debug, PartialEq)]
enum Step {
    Paused,
    Login,
    Confirm,
    Changed,
    Running,
    Recover,
    Check,
    Enable,
    Blocked,
}
fn plan(i: &Intent, health: &Value, account: Option<&str>) -> Step {
    if !i.requested {
        return Step::Paused;
    }
    let Some(account) = account else {
        return Step::Login;
    };
    if i.account.as_deref().is_some_and(|a| a != account) {
        return Step::Changed;
    }
    if !i.exclusive_account {
        return Step::Confirm;
    }
    if health["production_authorized"] == true {
        return Step::Running;
    }
    match health["classification"].as_str() {
        Some("history_unknown") => Step::Recover,
        Some("recovery_waiting") => Step::Check,
        Some("not_authorized" | "fresh_bound_not_authorized") => Step::Enable,
        _ => Step::Blocked,
    }
}
fn event(store: &Store, status: &str, next: i64, reason: Option<&str>) -> Result<Value> {
    let v = json!({"status":status,"next_attempt":next,"reason":reason,"at":chrono::Utc::now().timestamp(),"forced_model_requests":0});
    store.write("onboarding-status.json", &v)?;
    Ok(v)
}
pub fn tick(root: &Path) -> Result<Value> {
    // This command is deliberately root-only on the real managed installation.
    // Tests call the pure planner and existing mocked recovery/cutover adapters.
    if std::env::var_os("KAIRO_OFFLINE").as_deref() == Some(std::ffi::OsStr::new("1")) {
        bail!("offline_mode / 离线模式禁止真实后台初始化");
    }
    if unsafe { libc::geteuid() } != 0
        || std::fs::canonicalize(root)? != Path::new("/var/lib/kairo")
    {
        bail!("managed_root_required / 后台初始化仅支持标准安装和 root");
    }
    // Serialize timer/manual invocations, including while Store is released for cutover.
    let _lease = crate::runtime::daemon_lease(&root.join("onboarding-lock"))?;
    let store = Store::open(root)?;
    if !recovery::present(&store.root.join("onboarding.json"))? {
        return Ok(json!({"status":"manual_policy_preserved"}));
    }
    let mut i: Intent = store.read("onboarding.json")?;
    i.validate()?;
    let now = chrono::Utc::now().timestamp();
    if i.next_attempt > now && i.requested {
        return Ok(json!({"status":"backoff","next_attempt":i.next_attempt}));
    }
    let id = rt::runtime(&store)
        .and_then(|r| crate::identity(&r.codex_home.join("auth.json")))
        .ok();
    let health = recovery::inspect(&store, &rt::guard_root())?;
    let step = plan(&i, &health, id.as_ref().map(|id| id.fingerprint.as_str()));
    let status = match step {
        Step::Paused => Some("paused"),
        Step::Login => Some("login_required"),
        Step::Confirm => Some("exclusive_account_confirmation_required"),
        Step::Changed => Some("account_changed_confirmation_required"),
        Step::Running => Some("authorized"),
        Step::Blocked => Some("repair_required"),
        _ => None,
    };
    if let Some(status) = status {
        if step == Step::Running && crate::activation::status(root)["observation_override"] == true
        {
            drop(store);
            return crate::activation::enable_automatic(root);
        }
        return event(&store, status, 0, None);
    }
    if i.account.is_none() {
        i.account = id.map(|id| id.fingerprint);
        store.write("onboarding.json", &i)?;
    }
    let result = match step {
        Step::Recover => recovery::begin(&store, true),
        Step::Check => recovery::check(&store),
        Step::Enable => Ok(json!({"ready_for_explicit_approval":true})),
        _ => unreachable!(),
    };
    let ready = match result {
        Ok(v) => {
            if let Ok(q) = rt::verify(&store) {
                store.write("quota-cache.json", &q)?;
            }
            i.failures = 0;
            // Recheck at least once a minute: activity can invalidate an old deadline.
            i.next_attempt = now + 60;
            store.write("onboarding.json", &i)?;
            event(&store, "waiting_safety_recovery", i.next_attempt, None)?;
            v["ready_for_explicit_approval"] == true
        }
        Err(e) => {
            i.failures = i.failures.saturating_add(1);
            i.next_attempt = now + 30_i64.saturating_mul(1_i64 << i.failures.min(6)).min(1800);
            store.write("onboarding.json", &i)?;
            // No HTTP body, credentials or raw subprocess output in status.
            let code = e
                .to_string()
                .split(" / ")
                .next()
                .unwrap_or("initialization_failed")
                .chars()
                .take(100)
                .collect::<String>();
            return event(&store, "retry_wait", i.next_attempt, Some(&code));
        }
    };
    drop(store);
    if ready {
        // enable rechecks official quota and all guards after stopping daemon.
        match crate::activation::enable_automatic(root) {
            Ok(_) => event(&Store::open(root)?, "enabled", 0, None),
            Err(_) => event(
                &Store::open(root)?,
                "enable_failed",
                now + 60,
                Some("enable_failed / 启用失败，保留安全保护并自动重试"),
            ),
        }
    } else {
        Ok(json!({"status":"waiting_safety_recovery","next_attempt":i.next_attempt}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lifecycle_never_infers_fresh_or_overrides_pause_or_account_change() {
        let a = "a".repeat(64);
        let b = "b".repeat(64);
        let mut i = Intent {
            schema: 1,
            requested: true,
            exclusive_account: false,
            account: None,
            next_attempt: 0,
            failures: 0,
        };
        assert_eq!(plan(&i, &json!({}), None), Step::Login);
        assert_eq!(plan(&i, &json!({}), Some(&a)), Step::Confirm);
        i.exclusive_account = true;
        assert_eq!(
            plan(&i, &json!({"classification":"history_unknown"}), Some(&a)),
            Step::Recover
        );
        assert_eq!(
            plan(&i, &json!({"classification":"recovery_waiting"}), Some(&a)),
            Step::Check
        );
        assert_eq!(
            plan(&i, &json!({"classification":"data_corrupt"}), Some(&a)),
            Step::Blocked
        );
        i.account = Some(a.clone());
        assert_eq!(
            plan(&i, &json!({"production_authorized":true}), Some(&b)),
            Step::Changed
        );
        assert_eq!(
            plan(&i, &json!({"production_authorized":true}), Some(&a)),
            Step::Running
        );
        i.requested = false;
        assert_eq!(
            plan(&i, &json!({"production_authorized":true}), Some(&a)),
            Step::Paused
        );
    }
}

pub(crate) fn require_intent(store: &Store, account: &str) -> Result<()> {
    let i: Intent = store.read("onboarding.json")?;
    i.validate()?;
    if !i.requested || !i.exclusive_account || i.account.as_deref() != Some(account) {
        bail!("automatic_enable_cancelled / 自动启用已暂停或账号确认不匹配");
    }
    Ok(())
}
