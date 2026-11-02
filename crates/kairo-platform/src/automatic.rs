//! One automatic startup path. Missing history is not evidence of recent usage.
//! Server observations establish a local baseline, never cross-machine exclusivity.
use crate::{codex::AccountJournal, recovery::present, runtime as rt, Store};
use anyhow::{bail, Result};
use kairo_core::{Cursor, Quota, State};
use serde_json::{json, Value};
use std::path::Path;

/// Uses verified server data; also used by offline acceptance without credentials.
#[allow(clippy::too_many_arguments)]
pub fn initialize(
    store: &Store,
    guards: &Path,
    account: &str,
    five: Quota,
    week: Quota,
    denied: bool,
    now: i64,
    tz: chrono_tz::Tz,
) -> Result<Value> {
    if !five.valid(now, 18000) || !week.valid(now, 604800) {
        bail!("quota_invalid / 无法验证服务端额度");
    }
    if account.len() != 64 || !account.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("account_invalid / 账号无效");
    }
    // Parse every surviving record before writing anything. Corruption is not absence.
    let old_state = if present(&store.root.join("state.json"))? {
        Some(store.state()?)
    } else {
        None
    };
    let deployment = if present(&store.root.join("deployment.json"))? {
        let d: rt::Deployment = store.read("deployment.json")?;
        if d.schema != 1 || d.account != account {
            bail!("account_mismatch / 账号变化，已暂停");
        }
        Some(d)
    } else {
        None
    };
    if old_state
        .as_ref()
        .is_some_and(|s| s.account != account || s.last_clock > now)
    {
        bail!("account_or_clock_mismatch / 账号变化或时钟回拨");
    }
    let config = store.config()?;
    let mut cursor = if present(&store.root.join("cursor.json"))? {
        let c: Cursor = store.read("cursor.json")?;
        if c.schema != 1 || c.observed > now {
            bail!("cursor_invalid / 调度记录无效");
        }
        c
    } else {
        let mut c = Cursor::new(now, tz);
        c.arm(&config, tz, now)?;
        c
    };
    let gate = Store::open(&guards.join(account))?;
    let _authority = rt::authority_lease(&gate)?;
    let mut journal = if present(&gate.root.join("account.json"))? {
        let j: AccountJournal = gate.read("account.json")?;
        if j.schema != 1 || j.account != account || j.last_attempt < 0 {
            bail!("account_guard_invalid / 账号请求记录无效");
        }
        j
    } else {
        AccountJournal {
            schema: 1,
            account: account.into(),
            last_attempt: 0,
            anchors: Default::default(),
        }
    };
    // Retain valid old recovery evidence, but retire its mechanical quiet timer.
    for s in [store, &gate] {
        if present(&s.root.join("recovery.json"))? {
            let r: crate::recovery::Recovery = s.read("recovery.json")?;
            r.validate(account)?;
            if r.owner != std::fs::canonicalize(&store.root)? {
                bail!("account_execution_owned / 发现其他本机调度器");
            }
            // Retire only the recognizable synthetic recovery reservation.
            // Real local request timestamps are merged back below; anchors survive.
            let seed = r.not_before.saturating_sub(18000);
            if (journal.last_attempt == seed && seed > r.started_at + 90)
                || (journal.anchors.is_empty() && journal.last_attempt == r.started_at)
            {
                journal.last_attempt = 0;
            }
        }
    }
    if journal.last_attempt > now + 90 {
        bail!("account_guard_clock / 请求保护时间异常");
    }
    let mut state = old_state.unwrap_or_else(|| State {
        schema: 1,
        account: account.into(),
        last_clock: now,
        ..Default::default()
    });
    for a in &state.attempts {
        journal.anchors.insert(a.anchor.clone());
        journal.last_attempt = journal.last_attempt.max(a.completed_at.unwrap_or(a.at));
    }
    for a in &state.legacy_attempts {
        journal.anchors.insert(a.anchor.clone());
        journal.last_attempt = journal.last_attempt.max(a.at);
    }
    // Any surviving account reservation is retained even if local state vanished.
    cursor.finished.extend(journal.anchors.iter().cloned());
    state.weekly = Some(week.clone());
    state.server_denied = denied;
    kairo_core::observe(&mut state, now, five.clone());
    gate.write("account.json", &journal)?;
    store.write("state.json", &state)?;
    store.write("cursor.json", &cursor)?;
    for s in [store, &gate] {
        if present(&s.root.join("recovery.json"))? {
            let r: Value = s.read("recovery.json")?;
            s.write("retired-recovery.json", &r)?;
            std::fs::remove_file(s.root.join("recovery.json"))?;
            std::fs::File::open(&s.root)?.sync_all()?;
        }
    }
    store.write("quota-cache.json", &json!({"account":account,"account_fingerprint":account,"five_hour":five,"weekly":week,"server_denied":denied,"at":now}))?;
    // Old explicit pause wins. Old unapproved observation binding is initialized.
    let paused = crate::onboarding::execution_paused(store)?
        || deployment
            .as_ref()
            .is_some_and(|d| !d.enabled && d.approved_at.is_some());
    store.write(
        "deployment.json",
        &rt::Deployment {
            schema: 1,
            account: account.into(),
            enabled: !paused,
            approved_at: deployment.and_then(|d| d.approved_at).or(Some(now)),
        },
    )?;
    Ok(
        json!({"status":if paused {"paused"} else {"ready"},"next_check":cursor.due,"forced_model_requests":0}),
    )
}

pub fn ensure(store: &Store) -> Result<Value> {
    let now = chrono::Utc::now().timestamp();
    ensure_with(store, &rt::guard_root(), now, crate::timezone()?, |id| {
        rt::verified_usage(
            id,
            &rt::quota_with_renewal(store, &rt::runtime(store)?, id)?,
            now,
        )
    })
}
fn ensure_with(
    store: &Store,
    guards: &Path,
    now: i64,
    tz: chrono_tz::Tz,
    verify: impl FnOnce(&crate::Identity) -> Result<(Quota, Quota, bool)>,
) -> Result<Value> {
    if crate::onboarding::execution_paused(store)? {
        return Ok(json!({"status":"paused"}));
    }
    let r = rt::runtime(store)?;
    let path = r.codex_home.join("auth.json");
    if !present(&path)? {
        return Ok(json!({"status":"login_required"}));
    }
    use std::os::unix::fs::MetadataExt;
    if std::fs::symlink_metadata(&path)?.uid() != std::fs::metadata(&store.root)?.uid() {
        bail!("oauth_owner_mismatch / Codex 凭证所属用户与服务用户不一致");
    }
    let id = crate::identity(&path)?;
    if let Ok(i) = store.read::<crate::onboarding::Intent>("onboarding.json") {
        if i.account.as_deref().is_some_and(|a| a != id.fingerprint) {
            bail!("account_changed / 账号变化，已暂停");
        }
    }
    if let Ok(d) = store.read::<rt::Deployment>("deployment.json") {
        if d.schema != 1 {
            bail!("deployment_invalid / 状态无效");
        }
        if d.account != id.fingerprint {
            bail!("account_changed / 账号变化，已暂停");
        }
        if d.enabled
            && present(&store.root.join("state.json"))?
            && present(&store.root.join("cursor.json"))?
            && !present(&store.root.join("recovery.json"))?
            && present(&guards.join(&id.fingerprint).join("account.json"))?
            && !present(&guards.join(&id.fingerprint).join("recovery.json"))?
        {
            let fresh = store
                .read::<Value>("quota-cache.json")
                .ok()
                .is_some_and(|v| {
                    v["account"] == id.fingerprint
                        && v["at"]
                            .as_i64()
                            .is_some_and(|at| at <= now && now - at < 300)
                });
            if !fresh {
                let (f, w, denied) = verify(&id)?;
                store.write("quota-cache.json", &json!({"account":id.fingerprint,"five_hour":f,"weekly":w,"server_denied":denied,"at":now}))?;
            }
            return Ok(json!({"status":"ready"}));
        }
        if !d.enabled && d.approved_at.is_some() {
            return Ok(json!({"status":"paused"}));
        }
    }
    let (f, w, denied) = verify(&id)?;
    initialize(store, guards, &id.fingerprint, f, w, denied, now, tz)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{tick, Adapter};
    use std::sync::atomic::{AtomicU64, Ordering};
    struct Fixture {
        root: std::path::PathBuf,
        store: Store,
        guards: std::path::PathBuf,
        now: i64,
        account: String,
    }
    impl Fixture {
        fn new() -> Self {
            static N: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "kairo-auto-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            let store = Store::open(&root.join("data")).unwrap();
            let c = kairo_core::Config {
                buffer_seconds: 0,
                ..Default::default()
            };
            store.write("config.json", &c).unwrap();
            Self {
                guards: root.join("guards"),
                root,
                store,
                now: "2026-11-02T05:59:00Z"
                    .parse::<chrono::DateTime<chrono::Utc>>()
                    .unwrap()
                    .timestamp(),
                account: "a".repeat(64),
            }
        }
        fn init(&self, used: f64, remaining: i64) -> Result<Value> {
            initialize(
                &self.store,
                &self.guards,
                &self.account,
                q(self.now, 18000, used, remaining),
                q(self.now, 604800, 20., 604800),
                false,
                self.now,
                chrono_tz::UTC,
            )
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
    fn q(now: i64, duration: i64, used: f64, remaining: i64) -> Quota {
        Quota {
            duration,
            used,
            reset_at: now + remaining,
            remaining,
        }
    }
    struct Mock {
        now: i64,
        account: String,
        used: f64,
        reset: i64,
        week: f64,
        denied: bool,
        calls: u32,
        success: bool,
        network: bool,
    }
    impl Adapter for Mock {
        fn clock(&self) -> Option<i64> {
            Some(self.now)
        }
        fn account(&mut self) -> Result<String> {
            Ok(self.account.clone())
        }
        fn usage(&mut self) -> Result<(Quota, Quota, bool)> {
            if self.network {
                bail!("mock_network_failure");
            }
            Ok((
                q(self.now, 18000, self.used, (self.reset - self.now).max(0)),
                q(self.now, 604800, self.week, 604800),
                self.denied,
            ))
        }
        fn request(&mut self) -> Result<bool> {
            self.calls += 1;
            self.used = 1.;
            self.reset = self.now + 18000;
            Ok(self.success)
        }
    }
    fn mock(f: &Fixture) -> Mock {
        Mock {
            now: f.now + 60,
            account: f.account.clone(),
            used: 0.,
            reset: f.now + 60,
            week: 20.,
            denied: false,
            calls: 0,
            success: true,
            network: false,
        }
    }
    fn run(f: &Fixture, m: &mut Mock) -> Result<String> {
        let mut s = f.store.state()?;
        let mut c: Cursor = f.store.read("cursor.json")?;
        tick(
            &f.store,
            &f.store.config()?,
            &mut c,
            &mut s,
            chrono_tz::UTC,
            m.now,
            m,
        )
    }
    #[test]
    fn reinstall_oauth_ready_scheduled_request_and_durable_receipt_without_quiet_wait() {
        let f = Fixture::new();
        assert_eq!(f.init(0., 0).unwrap()["status"], "ready");
        let d: rt::Deployment = f.store.read("deployment.json").unwrap();
        assert!(d.enabled);
        let journal: AccountJournal = Store::open(&f.guards.join(&f.account))
            .unwrap()
            .read("account.json")
            .unwrap();
        assert_eq!(journal.last_attempt, 0);
        let mut m = mock(&f);
        assert!(run(&f, &mut m)
            .unwrap()
            .ends_with("five_hour_window_confirmed"));
        assert_eq!(m.calls, 1);
        m.now += 1;
        run(&f, &mut m).unwrap();
        assert_eq!(m.calls, 1);
        let s = f.store.state().unwrap();
        assert_eq!(s.attempts.len(), 1);
        assert_eq!(s.attempts[0].outcome, "five_hour_window_confirmed");
        crate::confirmation::sync(&f.store, &s).unwrap();
        assert!(f.store.root.join("confirmations.json").is_file());
    }
    #[test]
    fn active_window_skips_slot_and_uses_next_configured_opportunity() {
        let f = Fixture::new();
        f.init(3., 7200).unwrap();
        let mut m = mock(&f);
        m.used = 3.;
        m.reset = f.now + 7200;
        assert_eq!(run(&f, &mut m).unwrap(), "current_window_active");
        assert_eq!(m.calls, 0);
        let c: Cursor = f.store.read("cursor.json").unwrap();
        assert_eq!(c.anchor.as_deref(), Some("2026-11-02/11:00"));
        m.now = c.due.unwrap();
        m.used = 0.;
        m.reset = m.now;
        assert!(run(&f, &mut m).unwrap().contains("expired"));
        assert_eq!(m.calls, 1);
    }
    #[test]
    fn quota_exhaustion_external_activation_network_and_account_change_never_request() {
        for condition in 0..5 {
            let f = Fixture::new();
            f.init(0., 0).unwrap();
            let mut m = mock(&f);
            match condition {
                0 => {
                    m.used = 100.;
                    m.reset = m.now + 1000;
                }
                1 => m.week = 100.,
                2 => m.denied = true,
                3 => m.network = true,
                _ => m.account = "b".repeat(64),
            }
            let _ = run(&f, &mut m);
            assert_eq!(m.calls, 0, "condition {condition}");
        }
    }
    #[test]
    fn full_and_partial_upgrade_preserve_config_attempts_and_pause() {
        let mut f = Fixture::new();
        f.init(0., 0).unwrap();
        let mut m = mock(&f);
        run(&f, &mut m).unwrap();
        let before = f.store.state().unwrap();
        f.now += 61;
        std::fs::remove_file(f.store.root.join("cursor.json")).unwrap();
        f.init(0., 0).unwrap();
        assert_eq!(
            f.store.state().unwrap().attempts.len(),
            before.attempts.len()
        );
        assert_eq!(
            f.store.config().unwrap().anchors,
            vec!["06:00", "11:00", "16:00", "21:00"]
        );
        crate::onboarding::pause(&f.store).unwrap();
        assert_eq!(f.init(0., 0).unwrap()["status"], "paused");
        assert!(
            !f.store
                .read::<rt::Deployment>("deployment.json")
                .unwrap()
                .enabled
        );
    }
    #[test]
    fn invalid_state_and_conflicting_account_are_preserved() {
        let f = Fixture::new();
        f.store.write("state.json", &json!({"schema":999})).unwrap();
        let before = std::fs::read(f.store.root.join("state.json")).unwrap();
        assert!(f.init(0., 0).is_err());
        assert_eq!(
            std::fs::read(f.store.root.join("state.json")).unwrap(),
            before
        );
        std::fs::remove_file(f.store.root.join("state.json")).unwrap();
        f.init(0., 0).unwrap();
        let mut d: rt::Deployment = f.store.read("deployment.json").unwrap();
        d.account = "b".repeat(64);
        f.store.write("deployment.json", &d).unwrap();
        assert!(f.init(0., 0).is_err());
    }
    #[test]
    fn two_consecutive_scheduled_slots_and_restart_do_not_duplicate_a_window() {
        let f = Fixture::new();
        f.init(0., 0).unwrap();
        let mut m = mock(&f);
        run(&f, &mut m).unwrap();
        assert_eq!(m.calls, 1);
        let cursor: Cursor = f.store.read("cursor.json").unwrap();
        assert_eq!(cursor.anchor.as_deref(), Some("2026-11-02/11:00"));
        m.now = cursor.due.unwrap();
        m.used = 0.;
        m.reset = m.now;
        run(&f, &mut m).unwrap();
        assert_eq!(m.calls, 2);
        m.now += 1;
        run(&f, &mut m).unwrap();
        assert_eq!(m.calls, 2);
        assert_eq!(f.store.state().unwrap().attempts.len(), 2);
    }
    #[test]
    fn legacy_quiet_timer_seed_is_retired_without_waiting_or_losing_evidence() {
        let f = Fixture::new();
        let gate = Store::open(&f.guards.join(&f.account)).unwrap();
        let r = crate::recovery::Recovery {
            schema: 1,
            account: f.account.clone(),
            owner: std::fs::canonicalize(&f.store.root).unwrap(),
            started_at: f.now - 10,
            checked_at: f.now - 10,
            not_before: f.now - 10 + crate::recovery::QUIET_SECONDS,
            five: q(f.now - 10, 18000, 0., 18000),
            weekly: q(f.now - 10, 604800, 20., 604800),
            history_before_start: "unknown".into(),
            approved: false,
            boot_id: "synthetic-boot".into(),
            checked_uptime_ms: 1000,
            quiet_remaining_ms: crate::recovery::QUIET_SECONDS * 1000,
        };
        f.store.write("recovery.json", &r).unwrap();
        gate.write("recovery.json", &r).unwrap();
        gate.write(
            "account.json",
            &AccountJournal {
                schema: 1,
                account: f.account.clone(),
                last_attempt: r.not_before - 18000,
                anchors: Default::default(),
            },
        )
        .unwrap();
        drop(gate);
        assert_eq!(f.init(0., 0).unwrap()["status"], "ready");
        let gate = Store::open(&f.guards.join(&f.account)).unwrap();
        let j: AccountJournal = gate.read("account.json").unwrap();
        assert_eq!(j.last_attempt, 0);
        assert!(!f.store.root.join("recovery.json").exists());
        assert!(f.store.root.join("retired-recovery.json").exists());
        assert!(gate.root.join("retired-recovery.json").exists());
    }
    #[test]
    fn login_before_or_after_install_uses_same_automatic_path() {
        for login_first in [true, false] {
            let f = Fixture::new();
            let home = Store::open(&f.root.join("codex")).unwrap();
            let r = rt::Runtime {
                home: f.root.clone(),
                codex_home: home.root.clone(),
                ..Default::default()
            };
            f.store.write("runtime.json", &r).unwrap();
            if !login_first {
                assert_eq!(
                    ensure_with(&f.store, &f.guards, f.now, chrono_tz::UTC, |_| panic!(
                        "no login must not query quota"
                    ))
                    .unwrap()["status"],
                    "login_required"
                );
            }
            home.write("auth.json",&json!({"tokens":{"account_id":"synthetic-login-order","access_token":"synthetic-offline-placeholder"}})).unwrap();
            assert_eq!(
                ensure_with(&f.store, &f.guards, f.now, chrono_tz::UTC, |_| Ok((
                    q(f.now, 18000, 0., 0),
                    q(f.now, 604800, 2., 604800),
                    false
                )))
                .unwrap()["status"],
                "ready"
            );
            assert!(!f.store.root.join("recovery.json").exists());
            assert!(
                f.store
                    .read::<rt::Deployment>("deployment.json")
                    .unwrap()
                    .enabled
            );
        }
    }
}
