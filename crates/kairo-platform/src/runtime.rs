use crate::{
    codex::{AccountGate, AccountJournal, CodexExecutor},
    confirmation,
    engine::{self, Adapter},
    identity, parse_usage, quota, timezone, Store,
};
use anyhow::{bail, Context, Result};
use chrono::Utc;
use fs2::FileExt;
use kairo_core::{Config, Cursor, Decision, Quota, State};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Runtime {
    pub schema: u32,
    pub codex: PathBuf,
    pub home: PathBuf,
    pub codex_home: PathBuf,
    pub interval_seconds: u64,
}
impl Default for Runtime {
    fn default() -> Self {
        // Cloud-init and other root provisioning contexts may omit HOME entirely.
        // Keep the temporary default valid until system-install sets the service home.
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| PathBuf::from("/"));
        Self {
            schema: 1,
            codex: find_codex(),
            codex_home: std::env::var_os("CODEX_HOME")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .unwrap_or_else(|| home.join(".codex")),
            home,
            interval_seconds: 1,
        }
    }
}
fn find_codex() -> PathBuf {
    std::env::var_os("PATH")
        .and_then(|p| {
            std::env::split_paths(&p)
                .map(|d| d.join("codex"))
                .find(|p| p.is_file())
        })
        .unwrap_or_else(|| PathBuf::from("/usr/local/bin/codex"))
}
impl Runtime {
    pub fn validate(&self) -> Result<()> {
        if self.schema != 1
            || !self.codex.is_absolute()
            || !self.home.is_absolute()
            || !self.codex_home.is_absolute()
            || !(1..=60).contains(&self.interval_seconds)
        {
            bail!("runtime_invalid / 运行配置无效")
        }
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Deployment {
    pub schema: u32,
    pub account: String,
    pub enabled: bool,
    pub approved_at: Option<i64>,
}
pub fn runtime(store: &Store) -> Result<Runtime> {
    let r: Runtime = store.read("runtime.json")?;
    r.validate()?;
    Ok(r)
}
pub fn verified_usage(id: &crate::Identity, v: &Value, now: i64) -> Result<(Quota, Quota, bool)> {
    let account = v["account_id"]
        .as_str()
        .context("server_identity_missing / 服务端未返回账号身份")?;
    if account != id.account_id {
        bail!("server_identity_mismatch / 服务端账号身份不匹配")
    }
    parse_usage(v, now)
}
pub fn verify(store: &Store) -> Result<Value> {
    let r = runtime(store)?;
    let id = identity(&r.codex_home.join("auth.json"))?;
    let v = quota(&id)?;
    let (f, w, d) = verified_usage(&id, &v, Utc::now().timestamp())?;
    Ok(
        json!({"account_fingerprint":id.fingerprint,"identity_server_verified":true,"five_hour":f,"weekly":w,"server_denied":d}),
    )
}
pub fn bind(store: &Store, acknowledge_fresh: bool) -> Result<Value> {
    if !acknowledge_fresh {
        bail!("fresh_account_ack_required / 首次绑定须确认无近期请求；已有服务应使用迁移导入")
    }
    if [
        "deployment.json",
        "state.json",
        "cursor.json",
        "recovery.json",
        "account-origin.json",
    ]
    .iter()
    .any(|n| crate::recovery::present(&store.root.join(n)).unwrap_or(true))
    {
        bail!("already_bound / 已绑定或状态需要人工复核")
    }
    ensure_legacy_inactive()?;
    let r = runtime(store)?;
    let id = identity(&r.codex_home.join("auth.json"))?;
    let v = quota(&id)?;
    let now = Utc::now().timestamp();
    verified_usage(&id, &v, now)?;
    let root = guard_root();
    let path = root.join(&id.fingerprint);
    if path.exists() {
        bail!("account_guard_exists / 账号安全门禁已存在；不得重建状态")
    }
    let gate = Store::open(&path)?;
    gate.write(
        "account.json",
        &AccountJournal {
            schema: 1,
            account: id.fingerprint.clone(),
            last_attempt: now,
            anchors: Default::default(),
        },
    )?;
    let state = State {
        schema: 1,
        account: id.fingerprint.clone(),
        last_clock: now,
        ..Default::default()
    };
    store.write("state.json", &state)?;
    store.write(
        "deployment.json",
        &Deployment {
            schema: 1,
            account: id.fingerprint.clone(),
            enabled: false,
            approved_at: None,
        },
    )?;
    let mut cursor = Cursor::new(now, timezone()?);
    cursor.arm(&store.config()?, timezone()?, now)?;
    store.write("cursor.json", &cursor)?;
    store.write(
        "account-origin.json",
        &json!({"account":id.fingerprint,"origin":"administrator_declared_fresh","at":now}),
    )?;
    Ok(
        json!({"bound":true,"account_fingerprint":id.fingerprint,"execution":"observation_only","quarantine_until":now+18300}),
    )
}
pub fn guard_root() -> PathBuf {
    PathBuf::from("/var/lib/kairo-account-guards")
}
pub fn ensure_legacy_inactive() -> Result<()> {
    let o = crate::process::output(
        std::process::Command::new("systemctl").args([
            "show",
            "codex-window-scheduler.service",
            "--property=ActiveState",
            "--value",
        ]),
        10,
        4096,
    )
    .context("legacy_status_unknown / 无法检查旧服务状态")?;
    let status = String::from_utf8_lossy(&o.stdout);
    if !o.status.success() || !["inactive", "failed"].contains(&status.trim()) {
        bail!("legacy_active_or_unknown / 旧服务仍运行或状态未知，禁止接管")
    }
    // A stopped unit does not rule out a manually started legacy process.
    for entry in fs::read_dir("/proc")?.flatten() {
        if entry
            .file_name()
            .to_string_lossy()
            .bytes()
            .all(|b| b.is_ascii_digit())
        {
            if let Ok(cmd) = fs::read(entry.path().join("cmdline")) {
                if cmd
                    .split(|b| *b == 0)
                    .any(|arg| arg == b"/opt/codex-window-scheduler/scheduler.py")
                {
                    bail!("legacy_process_active / 旧调度进程仍运行")
                }
            }
        }
    }
    Ok(())
}
pub fn activate(store: &Store, account: &str, ack: bool) -> Result<()> {
    if !ack {
        bail!("cutover_approval_required / 需要明确生产切换批准")
    };
    for name in ["deployment.json", "state.json", "cursor.json"] {
        if !store.root.join(name).try_exists()? {
            bail!("account_recovery_required / 缺少账号绑定或调度历史；旧账号请运行 recover begin --acknowledge-unknown-history，新账号请使用首次绑定向导");
        }
    }
    crate::management::validate(store)?;
    ensure_legacy_inactive()?;
    let verified = verify(store)?;
    let mut d: Deployment = store.read("deployment.json")?;
    let s = store.state()?;
    if d.schema != 1
        || d.account != account
        || s.account != account
        || verified["account_fingerprint"] != account
    {
        bail!("account_mismatch / 账号不匹配")
    };
    let gate = AccountGate::open_existing(&guard_root(), account)?;
    gate.check_local(&store.root)?;
    d.enabled = true;
    d.approved_at = Some(Utc::now().timestamp());
    store.write("deployment.json", &d)?;
    crate::onboarding::resume_manual(store)
}
pub fn pause(store: &Store) -> Result<()> {
    let mut d: Deployment = store.read("deployment.json")?;
    d.enabled = false;
    store.write("deployment.json", &d)
}
static STOP: AtomicBool = AtomicBool::new(false);
static RELOAD: AtomicBool = AtomicBool::new(false);
extern "C" fn reload(_: i32) {
    RELOAD.store(true, Ordering::Relaxed);
}
extern "C" fn stop(_: i32) {
    STOP.store(true, Ordering::Relaxed);
}
pub fn stopping() -> bool {
    STOP.load(Ordering::Relaxed)
}
pub fn install_signals() {
    STOP.store(false, Ordering::Relaxed);
    unsafe {
        libc::signal(libc::SIGTERM, stop as *const () as usize);
        libc::signal(libc::SIGINT, stop as *const () as usize);
        libc::signal(libc::SIGHUP, reload as *const () as usize);
    }
}
/// Observation has no executor, account reservation gate, or live deployment capability.
struct Observer {
    r: Runtime,
    account: String,
}
impl Adapter for Observer {
    fn enabled(&self) -> bool {
        false
    }
    fn account(&mut self) -> Result<String> {
        Ok(identity(&self.r.codex_home.join("auth.json"))?.fingerprint)
    }
    fn usage(&mut self) -> Result<(Quota, Quota, bool)> {
        let id = identity(&self.r.codex_home.join("auth.json"))?;
        if id.fingerprint != self.account {
            bail!("account_changed / 账号变化")
        }
        verified_usage(&id, &quota(&id)?, Utc::now().timestamp())
    }
    fn request(&mut self) -> Result<bool> {
        bail!("observer_cannot_execute / 观察适配器没有模型执行能力")
    }
}
struct Live<'a> {
    store: &'a Store,
    r: Runtime,
    c: Config,
    d: Deployment,
    now: i64,
    anchor: String,
    cutoff: i64,
    gate: Option<AccountGate>,
}
impl Adapter for Live<'_> {
    fn clock(&self) -> Option<i64> {
        Some(Utc::now().timestamp())
    }
    fn account_guard_until(&self) -> Option<i64> {
        self.gate
            .as_ref()
            .map(|g| g.not_before(self.c.buffer_seconds))
    }

    fn account(&mut self) -> Result<String> {
        Ok(identity(&self.r.codex_home.join("auth.json"))?.fingerprint)
    }
    fn usage(&mut self) -> Result<(Quota, Quota, bool)> {
        if stopping() {
            bail!("shutdown_requested / 已收到退出信号，禁止新请求")
        }
        let id = identity(&self.r.codex_home.join("auth.json"))?;
        if id.fingerprint != self.d.account {
            bail!("account_changed / 账号变化")
        };
        let v = quota(&id)?;
        verified_usage(&id, &v, Utc::now().timestamp())
    }
    fn enabled(&self) -> bool {
        self.d.enabled
    }
    fn request(&mut self) -> Result<bool> {
        if stopping() {
            bail!("shutdown_requested / 已收到退出信号，禁止新请求")
        }
        use std::os::unix::fs::MetadataExt;
        if fs::metadata(&self.r.home)?.uid() != unsafe { libc::geteuid() }
            || fs::metadata(&self.store.root)?.uid() != unsafe { libc::geteuid() }
        {
            bail!("runtime_user_mismatch / 模型执行用户与 OAuth 用户不一致")
        }
        ensure_legacy_inactive()?;
        let (f, w, denied) = self.usage()?;
        let mut s = self.store.state()?;
        s.attempts.pop();
        let now = Utc::now().timestamp();
        if !matches!(
            kairo_core::decide(&self.c, &s, &self.anchor, now, self.cutoff, &f, &w, denied),
            Decision::Request(_)
        ) {
            bail!("final_gate_denied / 调用前最终门禁拒绝")
        }
        let work_root = self.store.root.join("work");
        fs::create_dir_all(&work_root)?;
        let work = work_root.join(format!("attempt-{}", self.now));
        fs::create_dir(&work)?;
        let exec = CodexExecutor::new(self.r.codex.clone(), self.r.home.clone(), work)?
            .with_codex_home(self.r.codex_home.clone())
            .with_cutoff(self.cutoff);
        let result = exec.execute(
            self.gate
                .as_mut()
                .context("account_guard_missing / 账号门禁缺失")?,
            &self.anchor,
            self.now,
            self.c.buffer_seconds,
            self.store,
        );
        let _ = fs::remove_dir_all(work_root.join(format!("attempt-{}", self.now)));
        result
    }
}
pub fn iteration(store: &Store, startup: bool, observe_only: bool) -> Result<Value> {
    let c = store.config()?;
    let r = runtime(store)?;
    let tz = timezone()?;
    let now = Utc::now().timestamp();
    if observe_only
        || crate::onboarding::execution_paused(store)?
        || !store.root.join("deployment.json").exists()
    {
        return shadow_iteration(store, &c, r, tz, now, startup);
    }
    let d: Deployment = store
        .read("deployment.json")
        .context("account_binding_required / 请先验证并绑定账号")?;
    if d.schema != 1 || d.account.is_empty() || d.enabled && d.approved_at.is_none() {
        bail!("deployment_invalid / 部署授权记录无效")
    }
    let mut s = store.state()?;
    if s.account != d.account {
        bail!("account_mismatch / 账号不匹配")
    };
    let mut cursor: Cursor = store.read("cursor.json")?;
    if startup {
        let anchor = cursor.anchor.clone();
        cursor.resume(&c, tz, now)?;
        s.pending = cursor
            .due
            .filter(|_| s.pending.is_some() && cursor.anchor == anchor);
        store.write("state.json", &s)?;
        store.write("cursor.json", &cursor)?;
    }
    validate_cursor(&cursor, &c, tz)?;
    if !d.enabled {
        let mut observer = Observer {
            r,
            account: d.account,
        };
        let event = engine::tick(store, &c, &mut cursor, &mut s, tz, now, &mut observer)?;
        return Ok(
            json!({"event":event,"at":now,"timezone":tz.to_string(),"next_check":cursor.due,"execution":"observation_only"}),
        );
    }
    let gate = if d.enabled {
        ensure_legacy_inactive()?;
        let gate = AccountGate::open_existing(&guard_root(), &d.account)?;
        gate.check_local(&store.root)?;
        Some(gate)
    } else {
        None
    };
    if d.enabled && now >= s.last_clock {
        confirmation::run_due(store, &s, &r.codex_home, now)?;
    }
    let mut adapter = Live {
        store,
        r,
        c: c.clone(),
        d,
        now,
        anchor: cursor.anchor.clone().unwrap_or_default(),
        cutoff: cursor.cutoff,
        gate,
    };
    let outcome = engine::tick(store, &c, &mut cursor, &mut s, tz, now, &mut adapter)?;
    confirmation::sync(store, &s)?;
    if let Some(a) = s
        .attempts
        .iter()
        .find(|a| a.at >= adapter.d.approved_at.unwrap_or(i64::MAX))
    {
        let old = store.read::<Value>("natural-acceptance.json").ok();
        let mut result = json!({"status":"first_natural_request_recorded","anchor":a.anchor,"at":a.at,"completed_at":a.completed_at,"outcome":a.outcome,"request_ok":matches!(a.outcome.as_str(),"request_ok_window_unconfirmed"|"five_hour_window_confirmed"),"window_confirmed":a.outcome=="five_hour_window_confirmed","forced_model_requests":0});
        if let Some(check) = old.as_ref().and_then(|v| v.get("first_natural_check")) {
            result["first_natural_check"] = check.clone();
        }
        if old.as_ref() != Some(&result) {
            store.write("natural-acceptance.json", &result)?;
            eprintln!(
                "{}",
                json!({"event":"first_natural_request_acceptance","result":result})
            );
        }
    } else if natural_check_event(&outcome) {
        let old = store.read::<Value>("natural-acceptance.json").ok();
        let mut result = old.unwrap_or_else(
            || json!({"status":"awaiting_first_future_natural_request","forced_model_requests":0}),
        );
        let check = json!({"at":now,"event":outcome,"next_check":cursor.due});
        if result.get("first_natural_check").is_none() {
            result["first_natural_check"] = check.clone();
        }
        result["latest_natural_check"] = check;
        store.write("natural-acceptance.json", &result)?;
        if outcome == "server_denied" {
            eprintln!(
                "{}",
                json!({"event":"natural_check_safety_warning","reason":"server_denied","at":now})
            );
        }
    }

    Ok(
        json!({"event":outcome,"at":now,"timezone":tz.to_string(),"next_check":cursor.due,"execution":if adapter.enabled(){"live"}else{"observation_only"}}),
    )
}
fn natural_check_event(event: &str) -> bool {
    !matches!(
        event,
        "waiting" | "armed" | "config_replanned" | "timezone_replanned"
    )
}
pub fn daemon_lease(root: &Path) -> Result<fs::File> {
    fs::create_dir_all(root)?;
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(root.join("daemon.lock"))?;
    lock.try_lock_exclusive()
        .context("daemon_already_running / 守护进程已运行或正在升级")?;
    Ok(lock)
}
fn execution_lease(root: &Path, account: &str) -> Result<fs::File> {
    let gate = Store::open(&root.join(account))?;
    authority_lease(&gate)
}
pub(crate) fn authority_lease(gate: &Store) -> Result<fs::File> {
    let f = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(gate.root.join("execution.lock"))?;
    crate::preserve_owner(&f, &gate.root)?;
    f.try_lock_exclusive()
        .context("account_execution_owned / 此账号已有其他调度实例持有执行权")?;
    Ok(f)
}
pub fn daemon(root: &Path, once: bool, observe_only: bool) -> Result<()> {
    if root.join(".upgrade-lock").exists() {
        let prefix: PathBuf = {
            let store = Store::open(root)?;
            store.read(".upgrade-lock")?
        };
        crate::update::recover(&prefix, root)?;
    }
    let _lease = daemon_lease(root)?;
    install_signals();
    let mut startup = true;
    let mut previous = String::new();
    let mut authority = None;
    let mut authority_account = String::new();
    loop {
        let store = Store::open(root)?;
        let result = (|| -> Result<Value> {
            if !observe_only
                && !crate::onboarding::execution_paused(&store)?
                && store.root.join("deployment.json").exists()
            {
                let d: Deployment = store.read("deployment.json")?;
                if d.enabled {
                    if d.schema != 1
                        || d.account.len() != 64
                        || !d.account.bytes().all(|b| b.is_ascii_hexdigit())
                    {
                        bail!("deployment_invalid / 部署授权记录无效")
                    }
                    if authority.is_none() {
                        let f = execution_lease(&guard_root(), &d.account)?;
                        authority = Some(f);
                        authority_account = d.account.clone();
                    }
                    if authority_account != d.account {
                        bail!("account_changed / 账号变化")
                    }
                } else {
                    authority = None;
                    authority_account.clear();
                }
            }
            if crate::onboarding::execution_paused(&store)? {
                authority = None;
                authority_account.clear();
            }
            iteration(&store, startup, observe_only)
        })();
        let failed = result.is_err();
        let mut value = match result {
            Ok(v) => v,
            Err(e) => {
                let message = e.to_string();
                let code = if message.contains(" / ") {
                    message.split(" / ").next().unwrap_or("runtime_error")
                } else if e.chain().any(|x| x.is::<std::io::Error>()) {
                    "io_error"
                } else if e.chain().any(|x| x.is::<serde_json::Error>()) {
                    "json_invalid"
                } else {
                    "runtime_error"
                };
                json!({"event":"blocked","at":Utc::now().timestamp(),"error":code})
            }
        };
        let signature = format!("{}:{}", value["event"], value["error"]);
        value["daemon_pid"] = json!(std::process::id());
        store.write("status.json", &value)?;
        if signature != previous || once {
            println!("{}", value);
            std::io::stdout().flush()?;
            previous = signature;
        }
        let interval = runtime(&store).map(|r| r.interval_seconds).unwrap_or(1);
        drop(store);
        startup = failed;
        if once {
            if failed {
                bail!("daemon_blocked / 守护进程安全阻止执行")
            }
            return Ok(());
        }
        if stopping() {
            break;
        }
        for _ in 0..interval * 10 {
            if stopping() || RELOAD.swap(false, Ordering::Relaxed) {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }
    Ok(())
}
fn shadow_iteration(
    root: &Store,
    c: &Config,
    r: Runtime,
    tz: chrono_tz::Tz,
    now: i64,
    startup: bool,
) -> Result<Value> {
    let id = identity(&r.codex_home.join("auth.json"))?;
    let shadow = Store::open(&root.root.join("shadow").join(&id.fingerprint))?;
    if !shadow.root.join("initialized.json").exists() {
        if shadow.root.join("state.json").exists() {
            bail!("shadow_incomplete / 只读观察状态初始化未完成")
        };
        shadow.write(
            "state.json",
            &State {
                schema: 1,
                account: id.fingerprint.clone(),
                ..Default::default()
            },
        )?;
        let mut cursor = Cursor::new(now, tz);
        cursor.arm(c, tz, now)?;
        shadow.write("cursor.json", &cursor)?;
        shadow.write("initialized.json", &true)?;
    }
    let mut state = shadow.state()?;
    let mut cursor: Cursor = shadow.read("cursor.json")?;
    if startup {
        cursor.resume(c, tz, now)?;
    }
    validate_cursor(&cursor, c, tz)?;
    let mut adapter = Observer {
        r,
        account: id.fingerprint,
    };
    if startup {
        let (five, weekly, denied) = adapter.usage()?;
        state.weekly = Some(weekly);
        state.server_denied = denied;
        state.last_clock = state.last_clock.max(now);
        kairo_core::observe(&mut state, now, five);
        shadow.write("state.json", &state)?;
    }
    let event = engine::tick(&shadow, c, &mut cursor, &mut state, tz, now, &mut adapter)?;
    Ok(
        json!({"event":event,"at":now,"timezone":tz.to_string(),"next_check":cursor.due,"execution":"observation_only"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn payload() -> Value {
        let q = |d| json!({"limit_window_seconds":d,"used_percent":0,"reset_at":100000+d,"reset_after_seconds":d});
        json!({"account_id":"synthetic-account","rate_limit":{"allowed":true,"limit_reached":false,"primary_window":q(18000),"secondary_window":q(604800)}})
    }
    fn id() -> crate::Identity {
        crate::Identity {
            fingerprint: "synthetic".into(),
            token: "synthetic".into(),
            account_id: "synthetic-account".into(),
        }
    }
    #[test]
    fn authenticated_account_contract() {
        assert!(verified_usage(&id(), &payload(), 100000).is_ok());
        let mut p = payload();
        p["account_id"] = "other".into();
        assert!(verified_usage(&id(), &p, 100000).is_err());
        p["account_id"] = Value::Null;
        assert!(verified_usage(&id(), &p, 100000).is_err());
        p["account_id"] = "".into();
        assert!(verified_usage(&id(), &p, 100000).is_err());
    }
    #[test]
    fn verified_identity_does_not_bypass_denial() {
        let mut p = payload();
        p["rate_limit"]["allowed"] = false.into();
        assert!(verified_usage(&id(), &p, 100000).unwrap().2);
    }
    #[test]
    fn activation_without_approval_stays_closed() {
        let root = std::env::temp_dir().join(format!("kairo-activation-{}", std::process::id()));
        let s = Store::open(&root).unwrap();
        assert!(activate(&s, "synthetic", false).is_err());
        assert!(!root.join("deployment.json").exists());
        drop(s);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn natural_check_receipt_only_for_due_decisions() {
        for event in ["waiting", "armed", "config_replanned", "timezone_replanned"] {
            assert!(!natural_check_event(event));
        }
        for event in ["server_denied", "deferred", "cutoff", "missed_not_replayed"] {
            assert!(natural_check_event(event));
        }
    }
}

pub fn validate_cursor(cursor: &Cursor, c: &Config, tz: chrono_tz::Tz) -> Result<()> {
    use chrono::{NaiveDateTime, TimeZone};
    if cursor.schema != 1 || cursor.observed < 0 {
        bail!("cursor_invalid / 调度游标无效")
    }
    if let Some(due) = cursor.due {
        let key = cursor
            .anchor
            .as_deref()
            .context("anchor_missing / 调度锚点缺失")?;
        let local = NaiveDateTime::parse_from_str(key, "%Y-%m-%d/%H:%M")
            .context("anchor_invalid / 调度锚点格式无效")?;
        let anchor = tz
            .from_local_datetime(&local)
            .earliest()
            .context("anchor_invalid / 调度锚点格式无效")?
            .timestamp();
        let midnight = local
            .date()
            .succ_opt()
            .context("anchor_invalid / 调度锚点格式无效")?
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let cutoff = tz
            .from_local_datetime(&midnight)
            .earliest()
            .context("anchor_invalid / 调度锚点格式无效")?
            .timestamp();
        if cursor.timezone == tz.to_string()
            && cursor.config_signature == kairo_core::config_signature(c)
            && (due < anchor + c.buffer_seconds
                || due >= cutoff
                || cursor.cutoff != cutoff
                || !c.anchors.contains(&local.format("%H:%M").to_string()))
        {
            bail!("cursor_inconsistent / 调度游标与锚点或当日截止时间不一致")
        }
    }
    Ok(())
}
#[cfg(test)]
mod cursor_tests {
    use super::*;
    #[test]
    fn live_cursor_rejects_corrupt_deadline() {
        let now = 1792387044;
        let c = Config::default();
        let mut cursor = Cursor::new(now, chrono_tz::Asia::Shanghai);
        cursor.arm(&c, chrono_tz::Asia::Shanghai, now).unwrap();
        assert!(validate_cursor(&cursor, &c, chrono_tz::Asia::Shanghai).is_ok());
        cursor.cutoff += 86400;
        assert!(validate_cursor(&cursor, &c, chrono_tz::Asia::Shanghai).is_err());
    }
}

#[cfg(test)]
mod observer_capability_tests {
    use super::*;
    #[test]
    fn observer_cannot_execute_even_if_request_is_called_directly() {
        let mut o = Observer {
            r: Runtime::default(),
            account: "synthetic".into(),
        };
        assert!(!o.enabled());
        assert!(o
            .request()
            .unwrap_err()
            .to_string()
            .contains("observer_cannot_execute"));
    }
}

#[cfg(test)]
mod authority_tests {
    use super::*;
    #[test]
    fn only_one_daemon_can_own_an_account_across_data_directories() {
        let root = std::env::temp_dir().join(format!("kairo-owner-{}", std::process::id()));
        let account = "b".repeat(64);
        let first = execution_lease(&root, &account).unwrap();
        assert!(execution_lease(&root, &account).is_err());
        drop(first);
        drop(execution_lease(&root, &account).unwrap());
        fs::remove_dir_all(root).unwrap();
    }
}
