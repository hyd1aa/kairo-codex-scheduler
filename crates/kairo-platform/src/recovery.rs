//! Conservative recovery of unknown history. No model executor is available here.
use crate::{codex::AccountJournal, runtime as rt, Store};
use anyhow::{bail, Context, Result};
use kairo_core::{Cursor, Quota, State};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

// Covers a full five-hour window, maximum configurable buffer and executor timeout.
pub const QUIET_SECONDS: i64 = 18000 + 300 + 90;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recovery {
    pub schema: u32,
    pub account: String,
    pub owner: PathBuf,
    pub started_at: i64,
    pub checked_at: i64,
    pub not_before: i64,
    pub five: Quota,
    pub weekly: Quota,
    pub history_before_start: String,
    pub approved: bool,
}
impl Recovery {
    pub fn validate(&self, account: &str) -> Result<()> {
        if self.schema != 1
            || self.account != account
            || account.len() != 64
            || !account.bytes().all(|b| b.is_ascii_hexdigit())
            || !self.owner.is_absolute()
            || self.started_at < 0
            || self.checked_at < self.started_at
            || self.not_before < self.started_at.saturating_add(QUIET_SECONDS)
            || self.history_before_start != "unknown"
            || !self.five.valid(self.checked_at, 18000)
            || !self.weekly.valid(self.checked_at, 604800)
        {
            bail!("recovery_invalid / 恢复记录无效；保留证据并检查备份，不得当作新账号");
        }
        Ok(())
    }
}
pub(crate) fn present(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}
fn snapshot(store: &Store) -> Result<(String, Quota, Quota, bool)> {
    let v = rt::verify(store)?;
    Ok((
        v["account_fingerprint"]
            .as_str()
            .context("account_missing / 缺少已验证账号")?
            .into(),
        serde_json::from_value(v["five_hour"].clone())?,
        serde_json::from_value(v["weekly"].clone())?,
        v["server_denied"]
            .as_bool()
            .context("quota_invalid / 额度验证失败")?,
    ))
}
fn checked_quota(five: &Quota, weekly: &Quota, denied: bool, now: i64) -> Result<()> {
    if denied || !five.valid(now, 18000) || !weekly.valid(now, 604800) || weekly.used >= 100. {
        bail!(
            "recovery_quota_blocked / 额度未知、每周额度已用尽或服务端拒绝；保留观察，恢复未授权"
        );
    }
    Ok(())
}
fn archive(store: &Store, name: &str, session: i64) -> Result<()> {
    if !present(&store.root.join(name))? {
        return Ok(());
    }
    let bytes = crate::private_bytes(&store.root.join(name), 8 * 1024 * 1024)?;
    use sha2::{Digest, Sha256};
    let digest = format!("{:x}", Sha256::digest(&bytes));
    let path = store
        .root
        .join(format!("recovery-evidence-{session}-{digest}-{name}"));
    if present(&path)? {
        if crate::private_bytes(&path, 8 * 1024 * 1024)? != bytes {
            bail!("recovery_evidence_changed / 原始记录发生变化，禁止覆盖恢复证据");
        }
        return Ok(());
    }
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    crate::preserve_owner(&f, &store.root)?;
    f.write_all(&bytes)?;
    f.sync_all()?;
    fs::File::open(&store.root)?.sync_all()?;
    Ok(())
}
fn guard_journal(gate: &Store, account: &str, now: i64) -> Result<AccountJournal> {
    match gate.read::<AccountJournal>("account.json") {
        Ok(j) if j.schema == 1 && j.account == account && j.last_attempt >= 0 => Ok(j),
        Ok(_) => bail!("account_guard_mismatch / 账号保护记录不匹配，禁止跨账号恢复"),
        Err(_) if present(&gate.root.join("account.json"))? => {
            // A corrupt global request ledger cannot safely be reconstructed automatically.
            bail!("account_guard_corrupt / 全局账号保护记录损坏；保留原件并人工恢复备份，禁止覆盖");
        }
        Err(_) => Ok(AccountJournal {
            schema: 1,
            account: account.into(),
            last_attempt: now,
            anchors: Default::default(),
        }),
    }
}
/// Read-only classification. Absence is never evidence that the account is new.
pub fn inspect(store: &Store, guards: &Path) -> Result<Value> {
    let mut files = serde_json::Map::new();
    let mut corrupt = false;
    let mut missing = false;
    for name in ["state.json", "deployment.json", "cursor.json"] {
        let present = present(&store.root.join(name))?;
        let valid = match name {
            "state.json" => store.state().is_ok(),
            "deployment.json" => store
                .read::<rt::Deployment>(name)
                .is_ok_and(|d| d.schema == 1 && (!d.enabled || d.approved_at.is_some())),
            _ => store.read::<Cursor>(name).is_ok_and(|c| c.schema == 1),
        };
        files.insert(
            name.into(),
            json!(if !present {
                "missing"
            } else if valid {
                "valid"
            } else {
                "invalid_or_unreadable"
            }),
        );
        corrupt |= present && !valid;
        missing |= !present;
    }
    let id = rt::runtime(store)
        .ok()
        .and_then(|r| crate::identity(&r.codex_home.join("auth.json")).ok());
    let state = store.state().ok();
    let deployment = store.read::<rt::Deployment>("deployment.json").ok();
    let binding = id
        .as_ref()
        .zip(state.as_ref())
        .is_some_and(|(i, s)| i.fingerprint == s.account);
    let mut record = None;
    let mut gate_valid = false;
    let mut recovery_error = false;
    if let Some(id) = id.as_ref() {
        let path = guards.join(&id.fingerprint);
        if path.exists() {
            let gate = Store::open(&path)?;
            gate_valid = gate
                .read::<AccountJournal>("account.json")
                .is_ok_and(|j| j.schema == 1 && j.account == id.fingerprint && j.last_attempt >= 0);
            if present(&gate.root.join("account.json"))? && !gate_valid {
                corrupt = true;
            }
            if present(&gate.root.join("recovery.json"))? {
                match gate.read::<Recovery>("recovery.json") {
                    Ok(r) if r.validate(&id.fingerprint).is_ok() => record = Some(r),
                    _ => recovery_error = true,
                }
            }
        }
    }
    let owner = fs::canonicalize(&store.root)?;
    let recovery_matches = record
        .as_ref()
        .is_none_or(|r| r.owner == owner && r.approved);
    let local_recovery = if present(&store.root.join("recovery.json"))? {
        Some(store.read::<Recovery>("recovery.json"))
    } else {
        None
    };
    let mirrors_match = match (record.as_ref(), local_recovery.as_ref()) {
        (None, None) => true,
        (Some(r), Some(Ok(local))) => {
            local.validate(&r.account).is_ok()
                && local.owner == r.owner
                && local.started_at == r.started_at
                && local.approved == r.approved
        }
        _ => false,
    };
    let ready = mirrors_match
        && !missing
        && !corrupt
        && binding
        && gate_valid
        && recovery_matches
        && !recovery_error
        && crate::management::validate(store).is_ok();
    let authorized = ready
        && deployment
            .as_ref()
            .is_some_and(|d| d.enabled && d.approved_at.is_some());
    let fresh_declared = store
        .read::<Value>("account-origin.json")
        .ok()
        .is_some_and(|v| {
            v["origin"] == "administrator_declared_fresh"
                && id.as_ref().is_some_and(|id| v["account"] == id.fingerprint)
        });
    let class = if id.is_none() {
        "oauth_missing"
    } else if corrupt || recovery_error {
        "data_corrupt"
    } else if record.as_ref().is_some_and(|r| r.owner != owner) {
        "recovery_owned_elsewhere"
    } else if record.as_ref().is_some_and(|r| !r.approved) {
        "recovery_waiting"
    } else if !ready {
        "history_unknown"
    } else if !authorized && fresh_declared {
        "fresh_bound_not_authorized"
    } else if !authorized {
        "not_authorized"
    } else {
        "authorized"
    };
    Ok(
        json!({"classification":class,"files":files,"oauth_discovered":id.is_some(),"history_proves_new_account":false,
        "account_origin":if fresh_declared {"administrator_declared_fresh"}else if record.is_some(){"existing_unknown_history_recovery"}else{"unknown"},"local_binding_valid":binding,"account_guard_valid":gate_valid,"scheduler_state_ready":ready,"production_authorized":authorized,
        "recovery":record,"recovery_record_invalid":recovery_error,
        "next_step":if class=="history_unknown" {"recover begin --acknowledge-unknown-history"} else if class=="recovery_waiting" {"recover check"} else if class=="not_authorized" || class=="fresh_bound_not_authorized" {"enable --approve-production-cutover"} else {"recover status"}}),
    )
}
pub fn begin(store: &Store, ack: bool) -> Result<Value> {
    if !ack {
        bail!("unknown_history_ack_required / 请明确选择已有账号历史未知恢复；停止其他设备或调度器的自动调用，并在等待期间避免手动请求");
    }
    rt::ensure_legacy_inactive()?;
    let (account, f, w, denied) = snapshot(store)?;
    let now = chrono::Utc::now().timestamp();
    begin_verified(store, &rt::guard_root(), &account, f, w, denied, now)
}
#[allow(clippy::too_many_arguments)]
fn begin_verified(
    store: &Store,
    guards: &Path,
    account: &str,
    f: Quota,
    w: Quota,
    denied: bool,
    now: i64,
) -> Result<Value> {
    checked_quota(&f, &w, denied, now)?;
    store.config()?;
    for name in ["state.json", "deployment.json", "recovery.json"] {
        if let Ok(v) = store.read::<Value>(name) {
            if v["account"].as_str().is_some_and(|a| a != account) {
                bail!("account_mismatch / 原始记录属于其他账号，禁止覆盖");
            }
        }
    }
    if let Ok(s) = store.state() {
        if s.account != account {
            bail!("account_mismatch / 本地状态属于其他账号，禁止覆盖");
        }
    }
    if let Ok(d) = store.read::<rt::Deployment>("deployment.json") {
        if d.account != account || d.enabled {
            bail!("pause_before_recovery / 账号不匹配或已有生产授权；请先暂停并检查状态");
        }
    }
    let gate = Store::open(&guards.join(account))?;
    let _authority = rt::authority_lease(&gate)?;
    // Never overwrite corrupt recovery evidence, even if another local file is absent.
    let old = if present(&gate.root.join("recovery.json"))? {
        let r: Recovery = gate.read("recovery.json")?;
        r.validate(account)?;
        Some(r)
    } else {
        None
    };
    let owner = fs::canonicalize(&store.root)?;
    if let Some(previous) = old.as_ref().filter(|r| !r.approved && r.owner == owner) {
        if now < previous.checked_at {
            bail!("recovery_clock_or_request_pending / 时钟回拨，禁止恢复");
        }
        let mut r = previous.clone();
        let missing = !present(&gate.root.join("account.json"))?;
        let mut journal = guard_journal(&gate, account, now)?;
        if missing {
            // Interrupted provisioning or lost protection restarts a conservative wait.
            r.not_before = r
                .not_before
                .max(now.saturating_add(QUIET_SECONDS))
                .max(f.reset_at.saturating_add(300));
            r.checked_at = now;
            r.five = f;
            r.weekly = w;
            gate.write("recovery.json", &r)?;
            journal.last_attempt = journal.last_attempt.max(r.not_before.saturating_sub(18000));
            gate.write("account.json", &journal)?;
        }
        store.write("recovery.json", &r)?;
        return Ok(view(&r, now));
    }
    let mut journal = guard_journal(&gate, account, now)?;
    if journal.last_attempt > now {
        bail!("recovery_clock_or_request_pending / 存在未来请求保护或时钟回拨；等待并检查其他调度实例");
    }
    // Existing global anchors and timestamps remain intact; no synthetic request is inserted.
    let r = Recovery {
        schema: 1,
        account: account.into(),
        owner,
        started_at: now,
        checked_at: now,
        not_before: now
            .saturating_add(QUIET_SECONDS)
            .max(f.reset_at.saturating_add(300))
            .max(journal.last_attempt.saturating_add(QUIET_SECONDS))
            .max(old.as_ref().map_or(0, |r| r.not_before)),
        five: f,
        weekly: w,
        history_before_start: "unknown".into(),
        approved: false,
    };
    r.validate(account)?;
    archive(&gate, "recovery.json", now)?;
    gate.write("recovery.json", &r)?; // Persistent account barrier first, before any local repair.
                                      // This is an unknown-risk reservation, not an invented model request.
                                      // Seed the old-format cooldown too so rollback binaries remain conservative.
    journal.last_attempt = journal.last_attempt.max(r.not_before.saturating_sub(18000));
    gate.write("account.json", &journal)?;
    archive(store, "recovery.json", now)?;
    store.write("recovery.json", &r)?;
    Ok(view(&r, now))
}
fn view(r: &Recovery, now: i64) -> Value {
    json!({"recovery":r,"remaining_seconds":r.not_before.saturating_sub(now).max(0),"history_before_start":"unknown",
        "ready_for_explicit_approval":!r.approved && now>=r.not_before,"automatic_warmup_enabled":false,
        "next_step":if now<r.not_before{"wait; then recover check"}else{"enable --approve-production-cutover"}})
}
pub fn check(store: &Store) -> Result<Value> {
    rt::ensure_legacy_inactive()?;
    let (account, f, w, denied) = snapshot(store)?;
    check_verified(
        store,
        &rt::guard_root(),
        &account,
        f,
        w,
        denied,
        chrono::Utc::now().timestamp(),
        false,
    )
}
pub(crate) fn approve(store: &Store) -> Result<Value> {
    rt::ensure_legacy_inactive()?;
    let (account, f, w, denied) = snapshot(store)?;
    check_verified(
        store,
        &rt::guard_root(),
        &account,
        f,
        w,
        denied,
        chrono::Utc::now().timestamp(),
        true,
    )
}
#[allow(clippy::too_many_arguments)]
fn check_verified(
    store: &Store,
    guards: &Path,
    account: &str,
    f: Quota,
    w: Quota,
    denied: bool,
    now: i64,
    approve: bool,
) -> Result<Value> {
    checked_quota(&f, &w, denied, now)?;
    for name in ["state.json", "deployment.json", "recovery.json"] {
        if let Ok(v) = store.read::<Value>(name) {
            if v["account"].as_str().is_some_and(|a| a != account) {
                bail!("account_mismatch / 原始记录属于其他账号，禁止覆盖");
            }
        }
    }
    let gate = Store::open(&guards.join(account))?;
    let _authority = rt::authority_lease(&gate)?;
    let mut r: Recovery = gate
        .read("recovery.json")
        .context("recovery_not_started / 尚未开始恢复；请先运行 recover begin")?;
    r.validate(account)?;
    if r.owner != fs::canonicalize(&store.root)? || now < r.checked_at {
        bail!("recovery_owner_or_clock / 恢复目录不匹配或时钟回拨；禁止授权");
    }
    let mut journal = guard_journal(&gate, account, now)?;
    if !gate.root.join("account.json").exists() {
        bail!("recovery_guard_missing / 等待期间账号保护文件丢失；请重新核查，不得直接授权");
    }
    if journal.last_attempt > now && journal.last_attempt > r.not_before {
        bail!("recovery_clock_or_request_pending / 存在未来请求保护或时钟回拨");
    }
    // A reset moving with zero usage is normal for idle accounts. Positive usage or a
    // within-window counter increase is new evidence of activity and extends quarantine.
    let activity = (f.used > 0.
        && (r.five.used == 0.
            || (f.reset_at - r.five.reset_at).abs() > 5
            || f.used > r.five.used + 0.0001))
        || (w.used > 0.
            && (r.weekly.used == 0.
                || (w.reset_at - r.weekly.reset_at).abs() > 5
                || w.used > r.weekly.used + 0.0001));
    if activity {
        r.not_before = r
            .not_before
            .max(now.saturating_add(QUIET_SECONDS))
            .max(f.reset_at.saturating_add(300));
    }
    r.not_before = r.not_before.max(journal.last_attempt.saturating_add(18000));
    r.checked_at = now;
    r.five = f;
    r.weekly = w;
    journal.last_attempt = journal.last_attempt.max(r.not_before.saturating_sub(18000));
    gate.write("account.json", &journal)?;
    gate.write("recovery.json", &r)?;
    store.write("recovery.json", &r)?;
    if !approve {
        return Ok(view(&r, now));
    }
    if now < r.not_before {
        bail!("recovery_wait_required / 保守等待尚未结束或检测到新活动；请运行 recover check 查看新的截止时间");
    }
    if r.approved {
        let s = store.state()?;
        if s.account != account {
            bail!("account_mismatch / 账号不匹配");
        }
        crate::management::validate(store)?;
        return Ok(json!({"account_fingerprint":account,"recovery_approved":true}));
    }
    let mut state = match store.state() {
        Ok(s) if s.account == account => s,
        Ok(_) => bail!("account_mismatch / 本地历史属于其他账号"),
        Err(_) => State {
            schema: 1,
            account: account.into(),
            ..Default::default()
        },
    };
    // Evidence is durable before replacing an invalid/missing baseline. Valid requests
    // and legacy reservations are retained verbatim. Unknown history stays unknown.
    for name in ["state.json", "cursor.json", "deployment.json"] {
        archive(store, name, r.started_at)?;
    }
    if state.last_clock > now {
        bail!("recovery_clock_or_request_pending / 本地历史时钟超前，禁止恢复");
    }
    state.last_clock = now;
    state.pending = None;
    state.weekly = Some(r.weekly.clone());
    state.server_denied = false;
    kairo_core::observe(&mut state, now, r.five.clone());
    state.validate()?;
    let tz = crate::timezone()?;
    let mut cursor = Cursor::new(now, tz);
    cursor.arm(&store.config()?, tz, now)?;
    store.write("state.json", &state)?;
    store.write("cursor.json", &cursor)?;
    store.write(
        "deployment.json",
        &rt::Deployment {
            schema: 1,
            account: account.into(),
            enabled: false,
            approved_at: None,
        },
    )?;
    crate::management::validate(store)?;
    r.approved = true;
    gate.write("recovery.json", &r)?;
    store.write("recovery.json", &r)?;
    Ok(
        json!({"account_fingerprint":account,"recovery_approved":true,"history_before_start":"unknown"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        os::unix::fs::PermissionsExt,
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture {
        base: PathBuf,
        store: Store,
        guards: PathBuf,
        account: String,
        start: i64,
    }
    impl Fixture {
        fn new() -> Self {
            let base = std::env::temp_dir().join(format!(
                "kairo-recovery-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let store = Store::open(&base.join("local")).unwrap();
            store
                .write("config.json", &kairo_core::Config::default())
                .unwrap();
            let auth = Store::open(&base.join("auth")).unwrap();
            auth.write("auth.json",&json!({"tokens":{"account_id":"synthetic-recovery-account","access_token":"synthetic-test-credential"}})).unwrap();
            let account = crate::identity(&auth.root.join("auth.json"))
                .unwrap()
                .fingerprint;
            store
                .write(
                    "runtime.json",
                    &rt::Runtime {
                        schema: 1,
                        codex: base.join("missing-mock-codex"),
                        home: base.clone(),
                        codex_home: auth.root.clone(),
                        interval_seconds: 1,
                    },
                )
                .unwrap();
            Self {
                guards: base.join("guards"),
                base,
                store,
                account,
                start: 1_000_000,
            }
        }
        fn quotas(&self, now: i64) -> (Quota, Quota) {
            (
                Quota {
                    duration: 18000,
                    used: 0.,
                    reset_at: now + 18000,
                    remaining: 18000,
                },
                Quota {
                    duration: 604800,
                    used: 20.,
                    reset_at: self.start + 604800,
                    remaining: (self.start + 604800 - now).max(0),
                },
            )
        }
        fn begin(&self, now: i64) -> Result<Value> {
            let (f, w) = self.quotas(now);
            begin_verified(&self.store, &self.guards, &self.account, f, w, false, now)
        }
        fn check(&self, now: i64, approve: bool) -> Result<Value> {
            let (f, w) = self.quotas(now);
            check_verified(
                &self.store,
                &self.guards,
                &self.account,
                f,
                w,
                false,
                now,
                approve,
            )
        }
        fn record(&self) -> Recovery {
            let gate = Store::open(&self.guards.join(&self.account)).unwrap();
            gate.read("recovery.json").unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }
    #[test]
    fn missing_records_are_unknown_not_fresh_and_no_approval_is_implied() {
        let f = Fixture::new();
        let v = inspect(&f.store, &f.guards).unwrap();
        assert_eq!(v["classification"], "history_unknown");
        assert_eq!(v["scheduler_state_ready"], false);
        assert_eq!(v["history_proves_new_account"], false);
        let v = f.begin(f.start).unwrap();
        assert_eq!(v["automatic_warmup_enabled"], false);
        assert!(!f.store.root.join("state.json").exists());
        assert_eq!(f.record().history_before_start, "unknown");
        assert!(!f.record().approved);
        assert!(crate::codex::AccountGate::open_existing(&f.guards, &f.account).is_err());
        assert!(f.check(f.start + QUIET_SECONDS - 1, true).is_err());
        f.check(f.start + QUIET_SECONDS, true).unwrap();
        let s = f.store.state().unwrap();
        assert!(s.attempts.is_empty());
        assert!(s.legacy_attempts.is_empty());
        assert_eq!(s.account, f.account);
        assert!(s.pending.is_none());
        assert!(
            !f.store
                .read::<rt::Deployment>("deployment.json")
                .unwrap()
                .enabled
        );
        let v = inspect(&f.store, &f.guards).unwrap();
        assert_eq!(v["classification"], "not_authorized");
        assert_eq!(v["scheduler_state_ready"], true);
        let gate = crate::codex::AccountGate::open_existing(&f.guards, &f.account).unwrap();
        gate.check_local(&f.store.root).unwrap();
        assert!(gate.not_before(0) >= f.start + QUIET_SECONDS);
    }
    #[test]
    fn repeated_begin_resume_and_missing_guard_never_shorten_quarantine() {
        let f = Fixture::new();
        f.begin(f.start).unwrap();
        let original = f.record().not_before;
        f.begin(f.start + 100).unwrap();
        assert_eq!(f.record().not_before, original);
        fs::remove_file(f.store.root.join("recovery.json")).unwrap();
        f.begin(f.start + 200).unwrap();
        assert_eq!(f.record().not_before, original);
        fs::remove_file(f.guards.join(&f.account).join("account.json")).unwrap();
        assert!(f.check(f.start + 300, true).is_err());
        f.begin(f.start + 400).unwrap();
        assert!(f.record().not_before >= f.start + 400 + QUIET_SECONDS);
    }
    #[test]
    fn new_primary_and_weekly_activity_extend_wait_but_idle_rolling_resets_do_not() {
        let f = Fixture::new();
        f.begin(f.start).unwrap();
        f.check(f.start + 100, false).unwrap();
        assert_eq!(f.record().not_before, f.start + QUIET_SECONDS);
        let now = f.start + 200;
        let (mut five, week) = f.quotas(now);
        five.used = 1.;
        check_verified(
            &f.store, &f.guards, &f.account, five, week, false, now, false,
        )
        .unwrap();
        assert!(f.record().not_before >= now + QUIET_SECONDS);
        let now = now + 300;
        let (five, mut week) = f.quotas(now);
        week.used = 21.;
        check_verified(
            &f.store, &f.guards, &f.account, five, week, false, now, false,
        )
        .unwrap();
        assert!(f.record().not_before >= now + QUIET_SECONDS);
        assert!(f.check(now - 1, true).is_err());
        assert!(!f.record().approved);
    }
    #[test]
    fn denial_invalid_quota_and_identity_changes_never_create_authority() {
        let f = Fixture::new();
        let (five, week) = f.quotas(f.start);
        assert!(begin_verified(
            &f.store,
            &f.guards,
            &f.account,
            five.clone(),
            week.clone(),
            true,
            f.start
        )
        .is_err());
        assert!(!f.guards.join(&f.account).exists());
        let mut invalid = five;
        invalid.remaining = -1;
        assert!(
            begin_verified(&f.store, &f.guards, &f.account, invalid, week, false, f.start).is_err()
        );
        f.begin(f.start).unwrap();
        let now = f.start + QUIET_SECONDS;
        let (five, week) = f.quotas(now);
        assert!(
            check_verified(&f.store, &f.guards, &f.account, five, week, true, now, true).is_err()
        );
        let (five, week) = f.quotas(now);
        assert!(check_verified(
            &f.store,
            &f.guards,
            &"b".repeat(64),
            five,
            week,
            false,
            now,
            true
        )
        .is_err());
        assert!(!f.record().approved);
    }
    #[test]
    fn valid_requests_and_global_anchors_are_preserved_and_corrupt_local_bytes_are_archived() {
        let f = Fixture::new();
        let old = f.start - 50000;
        let (five, _) = f.quotas(old);
        let mut s = State {
            schema: 1,
            account: f.account.clone(),
            ..Default::default()
        };
        kairo_core::reserve(&mut s, "known-old-anchor", old, five);
        f.store.write("state.json", &s).unwrap();
        {
            let gate = Store::open(&f.guards.join(&f.account)).unwrap();
            gate.write(
                "account.json",
                &AccountJournal {
                    schema: 1,
                    account: f.account.clone(),
                    last_attempt: old,
                    anchors: ["known-old-anchor".into()].into_iter().collect(),
                },
            )
            .unwrap();
        }
        f.begin(f.start).unwrap();
        f.check(f.start + QUIET_SECONDS, true).unwrap();
        let restored = f.store.state().unwrap();
        assert_eq!(restored.attempts.len(), 1);
        assert_eq!(restored.attempts[0].outcome, "attempt_persisted_unknown");
        {
            let gate = Store::open(&f.guards.join(&f.account)).unwrap();
            assert!(gate
                .read::<AccountJournal>("account.json")
                .unwrap()
                .anchors
                .contains("known-old-anchor"));
        }
        let broken = Fixture::new();
        let raw = b"{broken-local-state";
        fs::write(broken.store.root.join("state.json"), raw).unwrap();
        fs::set_permissions(
            broken.store.root.join("state.json"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        assert_eq!(
            inspect(&broken.store, &broken.guards).unwrap()["classification"],
            "data_corrupt"
        );
        broken.begin(broken.start).unwrap();
        broken.check(broken.start + QUIET_SECONDS, true).unwrap();
        assert!(fs::read_dir(&broken.store.root)
            .unwrap()
            .flatten()
            .any(|e| e
                .file_name()
                .to_string_lossy()
                .starts_with("recovery-evidence-")
                && fs::read(e.path()).ok().as_deref() == Some(raw)));
        assert!(broken.store.state().unwrap().attempts.is_empty());
        assert_eq!(broken.record().history_before_start, "unknown");
    }
    #[test]
    fn partial_baseline_writes_are_closed_and_retryable() {
        let f = Fixture::new();
        f.begin(f.start).unwrap();
        let block = f.store.root.join(".cursor.json.tmp");
        fs::create_dir(&block).unwrap();
        assert!(f.check(f.start + QUIET_SECONDS, true).is_err());
        assert!(!f.record().approved);
        assert!(crate::codex::AccountGate::open_existing(&f.guards, &f.account).is_err());
        fs::remove_dir(block).unwrap();
        f.check(f.start + QUIET_SECONDS + 1, true).unwrap();
        assert!(f.record().approved);
        let mut local = f.record();
        local.approved = false;
        f.store.write("recovery.json", &local).unwrap();
        {
            let gate = crate::codex::AccountGate::open_existing(&f.guards, &f.account).unwrap();
            assert!(gate.check_local(&f.store.root).is_err());
        }
        f.check(f.start + QUIET_SECONDS + 2, true).unwrap();
        {
            let gate = crate::codex::AccountGate::open_existing(&f.guards, &f.account).unwrap();
            gate.check_local(&f.store.root).unwrap();
        }
        fs::remove_file(f.guards.join(&f.account).join("recovery.json")).unwrap();
        let gate = crate::codex::AccountGate::open_existing(&f.guards, &f.account).unwrap();
        assert!(gate.check_local(&f.store.root).is_err());
    }
    #[test]
    fn another_directory_or_daemon_cannot_approve_or_execute_this_recovery() {
        let f = Fixture::new();
        f.begin(f.start).unwrap();
        let other = Store::open(&f.base.join("other")).unwrap();
        other
            .write("config.json", &kairo_core::Config::default())
            .unwrap();
        let now = f.start + QUIET_SECONDS;
        let (five, week) = f.quotas(now);
        assert!(
            check_verified(&other, &f.guards, &f.account, five, week, false, now, true).is_err()
        );
        {
            let gate = Store::open(&f.guards.join(&f.account)).unwrap();
            let _authority = rt::authority_lease(&gate).unwrap();
            drop(gate);
            assert!(f.check(now, true).is_err());
        }
        f.check(now, true).unwrap();
        let gate = crate::codex::AccountGate::open_existing(&f.guards, &f.account).unwrap();
        assert!(gate.check_local(&other.root).is_err());
    }
    #[test]
    fn corrupt_or_foreign_global_journals_are_never_overwritten() {
        let f = Fixture::new();
        let gate = Store::open(&f.guards.join(&f.account)).unwrap();
        gate.write(
            "account.json",
            &json!({"unrecognized":"synthetic evidence"}),
        )
        .unwrap();
        drop(gate);
        let before = fs::read(f.guards.join(&f.account).join("account.json")).unwrap();
        assert!(f.begin(f.start).is_err());
        assert_eq!(
            before,
            fs::read(f.guards.join(&f.account).join("account.json")).unwrap()
        );
        let other = Fixture::new();
        other
            .store
            .write(
                "state.json",
                &json!({"account":"another-account","broken":true}),
            )
            .unwrap();
        assert!(other.begin(other.start).is_err());
    }
    #[test]
    fn recovered_mock_executor_has_durable_global_dedup_after_restart() {
        let f = Fixture::new();
        f.begin(f.start).unwrap();
        f.check(f.start + QUIET_SECONDS, true).unwrap();
        let work = f.base.join("work");
        fs::create_dir(&work).unwrap();
        let home = f.base.join("home");
        fs::create_dir(&home).unwrap();
        let binary = f.base.join("mock-codex");
        fs::write(&binary,"#!/bin/sh\nif [ \"$2\" = --help ]; then echo '--ignore-user-config --sandbox --skip-git-repo-check --ephemeral'; exit 0; fi\nprintf x >> \"$HOME/calls\"\n").unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
        let now = f.start + QUIET_SECONDS + 1000;
        let (five, _) = f.quotas(now);
        let mut s = f.store.state().unwrap();
        kairo_core::reserve(&mut s, "new-anchor", now, five);
        f.store.write("state.json", &s).unwrap();
        let executor = crate::codex::CodexExecutor::new(binary, home.clone(), work).unwrap();
        {
            let mut gate = crate::codex::AccountGate::open_existing(&f.guards, &f.account).unwrap();
            assert!(executor
                .execute(&mut gate, "new-anchor", now, 30, &f.store)
                .unwrap());
        }
        let mut gate = crate::codex::AccountGate::open_existing(&f.guards, &f.account).unwrap();
        assert!(executor
            .execute(&mut gate, "new-anchor", now, 30, &f.store)
            .is_err());
        assert_eq!(fs::read_to_string(home.join("calls")).unwrap(), "x");
    }
}
