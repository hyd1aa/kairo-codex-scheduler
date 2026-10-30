//! Durable installation intent. The privileged timer coordinates existing safety gates;
//! it never executes a model or infers a fresh account from missing history.
use crate::{recovery, runtime as rt, Store};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
};

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
        .and_then(|r| service_identity(store, &r.codex_home.join("auth.json")))
        .ok();
    if exclusive && id.is_none() {
        bail!("oauth_unavailable / 请先完成服务账号的 Codex OAuth 登录，再确认独占调度");
    }
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
/// Successful official login only resets transport retry timing, never consent or recovery.
pub fn credentials_changed(store: &Store) -> Result<()> {
    if recovery::present(&store.root.join("onboarding.json"))? {
        let mut i: Intent = store.read("onboarding.json")?;
        i.validate()?;
        i.next_attempt = 0;
        i.failures = 0;
        store.write("onboarding.json", &i)?;
    }
    Ok(())
}
fn failure_status(code: &str) -> &'static str {
    if code.contains("HTTP 401") || code.contains("HTTP 403") || code.starts_with("OAuth") {
        "login_required"
    } else if code.contains("account_mismatch")
        || code.contains("_invalid")
        || code.contains("_unsafe")
    {
        "repair_required"
    } else {
        "retry_wait"
    }
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
    let lease = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open("/run/kairo-onboarding.lock")?;
    let meta = lease.metadata()?;
    if !meta.is_file() || meta.uid() != 0 || meta.nlink() != 1 || meta.mode() & 0o077 != 0 {
        bail!("onboarding_lock_unsafe / 初始化锁不安全");
    }
    fs2::FileExt::try_lock_exclusive(&lease)?;
    // Do not race an online installer, even between its daemon stop and update apply.
    let installer = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open("/var/lib/kairo-installer/online.lock")?;
    fs2::FileExt::try_lock_shared(&installer)?;
    if root.join(".upgrade-lock").try_exists()? {
        return Ok(json!({"status":"upgrade_in_progress"}));
    }
    advance(root, &mut Real, chrono::Utc::now().timestamp())
}
fn service_identity(store: &Store, path: &Path) -> Result<crate::Identity> {
    let owner = std::fs::metadata(&store.root)?.uid();
    let meta = std::fs::symlink_metadata(path)?;
    if meta.uid() != owner {
        bail!("service_oauth_unreadable / 服务账号无法读取此 OAuth，请在账号菜单登录");
    }
    let mut command = crate::management::account_command(store, Path::new("/usr/bin/test"))?;
    command.arg("-r").arg(path);
    let output = crate::process::output(&mut command, 3, 4096)?;
    if !output.status.success() {
        bail!("service_oauth_unreadable / 服务账号无法读取此 OAuth，请在账号菜单登录");
    }
    crate::identity(path)
}
trait Actions {
    fn account(&mut self, store: &Store) -> Option<String>;
    fn inspect(&mut self, store: &Store) -> Result<Value>;
    fn begin(&mut self, store: &Store) -> Result<Value>;
    fn check(&mut self, store: &Store) -> Result<Value>;
    fn needs_cutover(&mut self, root: &Path) -> bool;
    fn enable(&mut self, root: &Path) -> Result<Value>;
    fn refresh_quota(&mut self, _store: &Store, _now: i64) -> Result<()> {
        Ok(())
    }
}
struct Real;
impl Actions for Real {
    fn account(&mut self, store: &Store) -> Option<String> {
        let r = rt::runtime(store).ok()?;
        service_identity(store, &r.codex_home.join("auth.json"))
            .ok()
            .map(|id| id.fingerprint)
    }
    fn inspect(&mut self, store: &Store) -> Result<Value> {
        recovery::inspect(store, &rt::guard_root())
    }
    fn begin(&mut self, store: &Store) -> Result<Value> {
        recovery::begin(store, true)
    }
    fn check(&mut self, store: &Store) -> Result<Value> {
        recovery::check(store)
    }
    fn refresh_quota(&mut self, store: &Store, now: i64) -> Result<()> {
        let account = self.account(store);
        let fresh = store
            .read::<Value>("quota-cache.json")
            .ok()
            .is_some_and(|v| {
                v["account"].as_str() == account.as_deref()
                    && v["at"]
                        .as_i64()
                        .is_some_and(|at| at <= now && now - at < 300)
            });
        if !fresh {
            let mut q = rt::verify(store)?;
            q["at"] = json!(now);
            q["account"] = q["account_fingerprint"].clone();
            store.write("quota-cache.json", &q)?;
        }
        Ok(())
    }
    fn needs_cutover(&mut self, root: &Path) -> bool {
        let v = crate::activation::status(root);
        v["observation_override"] == true || v["service_active"] != true
    }
    fn enable(&mut self, root: &Path) -> Result<Value> {
        crate::activation::enable_automatic(root)
    }
}
fn advance(root: &Path, actions: &mut impl Actions, now: i64) -> Result<Value> {
    let store = Store::open(root)?;
    if !recovery::present(&store.root.join("onboarding.json"))? {
        return Ok(json!({"status":"manual_policy_preserved"}));
    }
    let mut i: Intent = store.read("onboarding.json")?;
    i.validate()?;
    if i.next_attempt > now && i.requested {
        return Ok(json!({"status":"backoff","next_attempt":i.next_attempt}));
    }
    let account = actions.account(&store);
    let health = actions.inspect(&store)?;
    let step = plan(&i, &health, account.as_deref());
    if step == Step::Running && i.account.is_none() {
        i.account = account.clone();
        store.write("onboarding.json", &i)?;
    }
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
        if step == Step::Running && actions.needs_cutover(root) {
            drop(store);
            return actions.enable(root);
        }
        if step == Step::Running {
            if let Err(e) = actions.refresh_quota(&store, now) {
                let code = e
                    .to_string()
                    .split(" / ")
                    .next()
                    .unwrap_or("quota_refresh_failed")
                    .chars()
                    .take(100)
                    .collect::<String>();
                return event(&store, failure_status(&code), now + 60, Some(&code));
            }
        }
        return event(&store, status, 0, None);
    }
    if i.account.is_none() {
        i.account = account;
        store.write("onboarding.json", &i)?;
    }
    let result = match step {
        Step::Recover => actions.begin(&store),
        Step::Check => actions.check(&store),
        Step::Enable => Ok(json!({"ready_for_explicit_approval":true})),
        _ => unreachable!(),
    };
    let ready = match result {
        Ok(v) => {
            if v["recovery"]["five"].is_object() {
                store.write("quota-cache.json", &json!({"five_hour":v["recovery"]["five"],"weekly":v["recovery"]["weekly"],"at":now,"account":i.account}))?;
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
            return event(&store, failure_status(&code), i.next_attempt, Some(&code));
        }
    };
    drop(store);
    if ready {
        // enable rechecks official quota and all guards after stopping daemon.
        match actions.enable(root) {
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

pub(crate) fn require_intent(store: &Store, account: &str) -> Result<()> {
    let i: Intent = store.read("onboarding.json")?;
    i.validate()?;
    if store.root.join(".upgrade-lock").try_exists()? {
        bail!("upgrade_in_progress / 升级进行中，推迟自动启用");
    }
    if !i.requested || !i.exclusive_account || i.account.as_deref() != Some(account) {
        bail!("automatic_enable_cancelled / 自动启用已暂停或账号确认不匹配");
    }
    Ok(())
}

/// Also provisions the timer when opting in after an upgrade from an older installer.
pub fn prepare(root: &Path) -> Result<()> {
    if std::fs::canonicalize(root)?.as_path() != Path::new("/var/lib/kairo") {
        return Ok(());
    }
    if unsafe { libc::geteuid() } != 0 {
        bail!("root_required / 请使用 sudo kairo 启动自动预热");
    }
    if std::env::var_os("KAIRO_OFFLINE").as_deref() == Some(std::ffi::OsStr::new("1")) {
        bail!("offline_mode / 离线模式禁止修改真实服务");
    }
    let units = Path::new("/etc/systemd/system");
    let meta = std::fs::symlink_metadata(units)?;
    if !meta.is_dir()
        || meta.file_type().is_symlink()
        || meta.uid() != 0
        || meta.mode() & 0o022 != 0
    {
        bail!("managed_service_unsafe / 服务目录不安全");
    }
    for (name, body) in [
        (
            "kairo-onboarding.service",
            include_str!("../../../packaging/kairo-onboarding.service"),
        ),
        (
            "kairo-onboarding.timer",
            include_str!("../../../packaging/kairo-onboarding.timer"),
        ),
    ] {
        let path = units.join(name);
        if recovery::present(&path)? {
            let m = std::fs::symlink_metadata(&path)?;
            if !m.is_file()
                || m.file_type().is_symlink()
                || m.uid() != 0
                || m.nlink() != 1
                || m.mode() & 0o022 != 0
                || std::fs::read_to_string(&path)? != body
            {
                bail!("custom_initialization_unit / 初始化任务已被修改，保留原件，请人工检查");
            }
        } else {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o644)
                .custom_flags(libc::O_NOFOLLOW)
                .open(&path)?;
            f.write_all(body.as_bytes())?;
            f.sync_all()?;
            std::fs::File::open(units)?.sync_all()?;
        }
    }
    for args in [
        &["daemon-reload"][..],
        &["enable", "--now", "kairo-onboarding.timer"][..],
    ] {
        let output = crate::process::output(
            std::process::Command::new("/usr/bin/systemctl").args(args),
            30,
            65536,
        )?;
        if !output.status.success() {
            bail!("initialization_timer_failed / 自动初始化定时任务未启动，请检查 systemd");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Mock {
        account: Option<String>,
        class: &'static str,
        ready: bool,
        offline: bool,
        observing: bool,
        fail_enable: bool,
        begins: u32,
        checks: u32,
        enables: u32,
    }
    impl Actions for Mock {
        fn account(&mut self, _: &Store) -> Option<String> {
            self.account.clone()
        }
        fn inspect(&mut self, _: &Store) -> Result<Value> {
            Ok(
                json!({"classification":self.class,"production_authorized":self.class=="authorized"}),
            )
        }
        fn begin(&mut self, _: &Store) -> Result<Value> {
            self.begins += 1;
            self.class = "recovery_waiting";
            Ok(json!({"ready_for_explicit_approval":false}))
        }
        fn check(&mut self, _: &Store) -> Result<Value> {
            self.checks += 1;
            if self.offline {
                bail!("quota_transport_failed / 网络失败");
            }
            Ok(json!({"ready_for_explicit_approval":self.ready}))
        }
        fn needs_cutover(&mut self, _: &Path) -> bool {
            self.observing
        }
        fn enable(&mut self, root: &Path) -> Result<Value> {
            let s = Store::open(root)?;
            require_intent(&s, self.account.as_deref().unwrap())?;
            if self.fail_enable {
                bail!("mock_cutover_failed / 模拟切换失败");
            }
            self.enables += 1;
            self.class = "authorized";
            self.observing = false;
            Ok(json!({"enabled":true}))
        }
    }
    fn fixture() -> (std::path::PathBuf, Mock) {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "kairo-onboard-{}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let s = Store::open(&p).unwrap();
        s.write(
            "onboarding.json",
            &Intent {
                schema: 1,
                requested: true,
                exclusive_account: true,
                account: None,
                next_attempt: 0,
                failures: 0,
            },
        )
        .unwrap();
        (
            p,
            Mock {
                account: Some("a".repeat(64)),
                class: "history_unknown",
                ready: false,
                offline: false,
                observing: true,
                fail_enable: false,
                begins: 0,
                checks: 0,
                enables: 0,
            },
        )
    }
    #[test]
    fn expired_oauth_is_login_required_and_transient_network_errors_retry() {
        assert_eq!(
            failure_status("Quota HTTP 401 Unauthorized"),
            "login_required"
        );
        assert_eq!(failure_status("Quota HTTP 403 Forbidden"), "login_required");
        assert_eq!(failure_status("Quota transport failed"), "retry_wait");
        assert_eq!(failure_status("account_mismatch"), "repair_required");
    }
    #[test]
    fn successful_login_resets_backoff_without_cancelling_pause_or_recovery() {
        let (root, _) = fixture();
        let s = Store::open(&root).unwrap();
        let mut i: Intent = s.read("onboarding.json").unwrap();
        i.requested = false;
        i.next_attempt = 9999;
        i.failures = 6;
        s.write("onboarding.json", &i).unwrap();
        s.write("recovery.json", &json!({"synthetic_wait_deadline":23456}))
            .unwrap();
        let before = std::fs::read(root.join("recovery.json")).unwrap();
        credentials_changed(&s).unwrap();
        let after: Intent = s.read("onboarding.json").unwrap();
        assert!(!after.requested);
        assert_eq!(after.next_attempt, 0);
        assert_eq!(after.failures, 0);
        assert_eq!(std::fs::read(root.join("recovery.json")).unwrap(), before);
        drop(s);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn service_credential_validation_rejects_symlinks_and_unprivate_files() {
        let (root, _) = fixture();
        let store = Store::open(&root).unwrap();
        let path = root.join("auth-fixture.json");
        store.write("auth-fixture.json",&json!({"tokens":{"account_id":"synthetic-readable-account","access_token":"synthetic-readable-placeholder"}})).unwrap();
        assert!(service_identity(&store, &path).is_ok());
        use std::os::unix::fs::{symlink, PermissionsExt};
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(service_identity(&store, &path).is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        symlink(&path, root.join("auth-link.json")).unwrap();
        assert!(service_identity(&store, &root.join("auth-link.json")).is_err());
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn persisted_recovery_automatically_rechecks_and_enables_exactly_once() {
        let (p, mut m) = fixture();
        advance(&p, &mut m, 100).unwrap();
        assert_eq!(m.begins, 1);
        assert_eq!(m.enables, 0);
        advance(&p, &mut m, 120).unwrap();
        assert_eq!(m.checks, 0); // restart retains backoff
        advance(&p, &mut m, 160).unwrap();
        assert_eq!(m.checks, 1);
        assert_eq!(m.enables, 0);
        m.ready = true;
        advance(&p, &mut m, 220).unwrap();
        assert_eq!(m.enables, 1);
        advance(&p, &mut m, 280).unwrap();
        advance(&p, &mut m, 340).unwrap();
        assert_eq!(m.enables, 1);
        assert_eq!(m.begins, 1);
        std::fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn login_after_install_and_unconfirmed_external_risk_never_enable() {
        let (p, mut m) = fixture();
        m.account = None;
        assert_eq!(
            advance(&p, &mut m, 100).unwrap()["status"],
            "login_required"
        );
        {
            let s = Store::open(&p).unwrap();
            let mut i: Intent = s.read("onboarding.json").unwrap();
            i.exclusive_account = false;
            s.write("onboarding.json", &i).unwrap();
        }
        m.account = Some("a".repeat(64));
        assert_eq!(
            advance(&p, &mut m, 160).unwrap()["status"],
            "exclusive_account_confirmation_required"
        );
        assert_eq!(m.begins + m.checks + m.enables, 0);
        std::fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn activity_network_account_change_corruption_and_pause_fail_closed() {
        let (p, mut m) = fixture();
        advance(&p, &mut m, 100).unwrap();
        m.offline = true;
        assert_eq!(advance(&p, &mut m, 160).unwrap()["status"], "retry_wait");
        let s = Store::open(&p).unwrap();
        let i: Intent = s.read("onboarding.json").unwrap();
        assert_eq!(i.failures, 1);
        drop(s);
        m.offline = false;
        advance(&p, &mut m, 220).unwrap();
        assert_eq!(m.enables, 0); // new activity keeps ready false
        m.account = Some("b".repeat(64));
        assert_eq!(
            advance(&p, &mut m, 280).unwrap()["status"],
            "account_changed_confirmation_required"
        );
        m.account = Some("a".repeat(64));
        m.class = "data_corrupt";
        assert_eq!(
            advance(&p, &mut m, 340).unwrap()["status"],
            "repair_required"
        );
        {
            let s = Store::open(&p).unwrap();
            pause(&s).unwrap();
        }
        m.ready = true;
        m.class = "recovery_waiting";
        assert_eq!(advance(&p, &mut m, 400).unwrap()["status"], "paused");
        assert_eq!(m.enables, 0);
        std::fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn upgrade_without_opt_in_preserves_all_existing_policies() {
        let (p, mut m) = fixture();
        std::fs::remove_file(p.join("onboarding.json")).unwrap();
        for class in ["authorized", "not_authorized", "recovery_waiting"] {
            m.class = class;
            assert_eq!(
                advance(&p, &mut m, 100).unwrap()["status"],
                "manual_policy_preserved"
            );
        }
        assert_eq!(m.begins + m.checks + m.enables, 0);
        std::fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn repeated_opt_in_preserves_bound_account_and_pause_cancels_cutover() {
        let (p, mut m) = fixture();
        advance(&p, &mut m, 100).unwrap();
        {
            let s = Store::open(&p).unwrap();
            request(&s, false).unwrap();
            request(&s, false).unwrap();
            pause(&s).unwrap();
            assert!(require_intent(&s, &"a".repeat(64)).is_err());
        }
        m.ready = true;
        advance(&p, &mut m, 200).unwrap();
        assert_eq!(m.begins, 1);
        assert_eq!(m.enables, 0);
        std::fs::remove_dir_all(p).unwrap();
    }
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
