//! Compatibility facade for the former onboarding CLI. The daemon initializes itself.
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
pub fn request(store: &Store, _exclusive: bool) -> Result<Value> {
    let mut i = if recovery::present(&store.root.join("onboarding.json"))? {
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
    // Compatibility flag no longer claims exclusive ownership across machines.
    i.requested = true;
    i.next_attempt = 0;
    i.failures = 0;
    store.write("onboarding.json", &i)?;
    if recovery::present(&store.root.join("deployment.json"))? {
        let mut d: rt::Deployment = store.read("deployment.json")?;
        if d.schema != 1 {
            bail!("deployment_invalid / 状态无效");
        }
        let r = rt::runtime(store)?;
        if let Ok(id) = crate::identity(&r.codex_home.join("auth.json")) {
            if d.account != id.fingerprint {
                bail!("account_changed / 账号变化，请检查账号");
            }
            d.enabled = true;
            store.write("deployment.json", &d)?;
        }
    }
    Ok(json!({"automatic_initialization_requested":true}))
}
pub fn credentials_changed(store: &Store) -> Result<()> {
    event(store, "credentials_updated", 0, None)?;
    Ok(())
}
fn event(store: &Store, status: &str, next: i64, reason: Option<&str>) -> Result<Value> {
    let v = json!({"status":status,"next_attempt":next,"reason":reason,"at":chrono::Utc::now().timestamp(),"forced_model_requests":0});
    store.write("onboarding-status.json", &v)?;
    Ok(v)
}
pub(crate) fn execution_paused(store: &Store) -> Result<bool> {
    execution_paused_path(&store.root)
}
pub(crate) fn execution_paused_path(root: &Path) -> Result<bool> {
    if !recovery::present(&root.join("onboarding.json"))? {
        return Ok(false);
    }
    let i: Intent = serde_json::from_slice(&crate::private_bytes(
        &root.join("onboarding.json"),
        1024 * 1024,
    )?)?;
    i.validate()?;
    Ok(!i.requested)
}
/// Explicit manual production approval may resume a previously paused intent.
pub(crate) fn resume_manual(store: &Store) -> Result<()> {
    if recovery::present(&store.root.join("onboarding.json"))? {
        let mut i: Intent = store.read("onboarding.json")?;
        i.validate()?;
        if !i.requested {
            i.requested = true;
            store.write("onboarding.json", &i)?;
        }
    }
    Ok(())
}
pub fn pause(store: &Store) -> Result<()> {
    let mut i = if recovery::present(&store.root.join("onboarding.json"))? {
        let i: Intent = store.read("onboarding.json")?;
        i.validate()?;
        i
    } else {
        Intent {
            schema: 1,
            requested: false,
            exclusive_account: false,
            account: None,
            next_attempt: 0,
            failures: 0,
        }
    };
    i.requested = false;
    store.write("onboarding.json", &i)
}
pub fn pause_confirmed(store: &Store) -> Result<()> {
    event(store, "paused", 0, None)?;
    Ok(())
}
pub(crate) fn fatal_cutover(error: &anyhow::Error) -> bool {
    error.chain().any(|e| {
        let s = e.to_string();
        s.contains("cutover_rollback_incomplete") || s.contains("observer_restart_failed")
    })
}
pub(crate) fn block_failed_cutover(store: &Store) -> Result<Value> {
    pause(store)?;
    event(
        store,
        "manual_review_required",
        0,
        Some("cutover_rollback_incomplete"),
    )
}
pub(crate) fn cutover_succeeded(store: &Store) -> Result<()> {
    if recovery::present(&store.root.join("onboarding.json"))? {
        event(store, "enabled", 0, None)?;
    }
    Ok(())
}

// Older scripts can retain onboard/enable commands; neither provisions a timer.
pub fn prepare(_root: &Path) -> Result<()> {
    Ok(())
}
pub fn tick(root: &Path) -> Result<Value> {
    if crate::legacy_service::retire(root)? {
        return Ok(json!({"status":"daemon_managed_initialization","forced_model_requests":0}));
    }
    crate::automatic::ensure(&Store::open(root)?)
}
pub(crate) fn require_intent(store: &Store, account: &str) -> Result<()> {
    if execution_paused(store)? {
        bail!("paused / 用户已暂停");
    }
    let r = rt::runtime(store)?;
    if crate::identity(&r.codex_home.join("auth.json"))?.fingerprint != account {
        bail!("account_changed / 账号变化");
    }
    Ok(())
}
