//! Local timed calls. Quota, account history and window resets never authorize execution.
use crate::{present, Store};
use anyhow::{bail, Context, Result};
use chrono::Utc;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
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

pub fn runtime(store: &Store) -> Result<Runtime> {
    let r: Runtime = store
        .retained("runtime.json")?
        .context("runtime_missing / 运行配置缺失")?;
    r.validate()?;
    Ok(r)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Call {
    pub slot: String,
    pub due: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub result: String,
    pub reason: String,
    pub exit_code: Option<i32>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduleState {
    pub schema: u32,
    pub paused: bool,
    /// Monotonic local watermark also protects slots evicted from the display history.
    pub watermark: i64,
    pub calls: Vec<Call>,
}
impl ScheduleState {
    pub fn load(store: &Store) -> Result<Self> {
        if let Some(s) = store.retained::<Self>("schedule.json")? {
            if s.schema != 1
                || s.watermark < 0
                || s.calls.len() > 1024
                || s.calls.iter().any(|c| c.due < 0 || c.due > s.watermark)
            {
                bail!("schedule_invalid / 本机调用记录无效");
            }
            return Ok(s);
        }
        // Legacy files are evidence, not execution prerequisites. Never rewrite them.
        let old = |name: &str| {
            store
                .retained::<Value>(name)
                .ok()
                .flatten()
                .unwrap_or(Value::Null)
        };
        let intent = old("onboarding.json");
        let deployment = old("deployment.json");
        let paused = intent["requested"] == false
            || (deployment["enabled"] == false && deployment["approved_at"].is_number());
        let mut calls = Vec::new();
        if let Some(attempts) = old("state.json")["attempts"].as_array() {
            for a in attempts.iter().rev().take(100).rev() {
                if let Some(at) = a["at"]
                    .as_i64()
                    .filter(|at| *at >= 0 && *at <= Utc::now().timestamp())
                {
                    calls.push(Call {
                        slot: a["anchor"].as_str().unwrap_or("legacy").into(),
                        due: at,
                        started_at: Some(at),
                        completed_at: a["completed_at"].as_i64(),
                        result: match a["outcome"].as_str() {
                            Some(
                                "request_ok_window_unconfirmed" | "five_hour_window_confirmed",
                            ) => "legacy_success",
                            Some("request_failed") => "failed",
                            _ => "unknown",
                        }
                        .into(),
                        reason: "legacy_history".into(),
                        exit_code: None,
                    });
                }
            }
        }
        calls.sort_by_key(|c| c.due);
        Ok(Self {
            schema: 1,
            paused,
            watermark: Utc::now()
                .timestamp()
                .max(calls.iter().map(|c| c.due).max().unwrap_or(0)),
            calls,
        })
    }
    pub fn save(&self, store: &Store) -> Result<()> {
        store.write("schedule.json", self)
    }
}
pub fn pause(store: &Store, paused: bool) -> Result<Value> {
    let mut s = ScheduleState::load(store)?;
    s.paused = paused;
    s.save(store)?;
    Ok(json!({"paused":paused,"enabled":!paused,"requested":!paused}))
}
pub fn verify(store: &Store) -> Result<Value> {
    let r = runtime(store)?;
    Ok(json!({"oauth_present":crate::codex::logged_in(store,&r)?,"quota_optional":true}))
}
pub fn daemon_lease(root: &Path) -> Result<fs::File> {
    drop(Store::open(root)?);
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(root.join("daemon.lock"))?;
    crate::single_owner(&lock)?;
    crate::preserve_owner(&lock, root)?;
    lock.try_lock_exclusive()
        .context("daemon_already_running / 守护进程已运行或正在升级")?;
    Ok(lock)
}

static STOP: AtomicBool = AtomicBool::new(false);
extern "C" fn signal(_: libc::c_int) {
    STOP.store(true, Ordering::Relaxed);
}
pub fn stopping() -> bool {
    STOP.load(Ordering::Relaxed)
}
fn signals() {
    unsafe {
        libc::signal(libc::SIGTERM, signal as *const () as usize);
        libc::signal(libc::SIGINT, signal as *const () as usize);
        libc::signal(libc::SIGHUP, libc::SIG_IGN);
    }
}

/// Invoked by the live loop and by offline fixtures using the same scheduler and fake CLI.
/// Reserve before any help/auth/exec process. Failure never retries this slot.
pub fn tick(root: &Path, after: i64, now: i64, tz: chrono_tz::Tz, observe: bool) -> Result<()> {
    let store = Store::open(root)?;
    let config = store.config()?;
    let mut state = ScheduleState::load(&store)?;
    if !present(&root.join("schedule.json"))? {
        state.save(&store)?;
    }
    let floor = after.max(state.watermark);
    if now <= floor {
        return Ok(());
    }
    let start = chrono::DateTime::from_timestamp(floor, 0).context("clock_invalid")?;
    for due in kairo_core::plan(&config, tz, start, 72)?
        .into_iter()
        .map(|d| d.timestamp())
        .take_while(|due| *due <= now)
    {
        let local = chrono::DateTime::from_timestamp(due - config.buffer_seconds, 0)
            .context("clock_invalid")?
            .with_timezone(&tz);
        let slot = local.format("%Y-%m-%d/%H:%M").to_string();
        state.watermark = due;
        if state.calls.iter().any(|c| c.slot == slot) {
            state.save(&store)?;
            continue;
        }
        let reason = if state.paused {
            "paused"
        } else if observe {
            "observation_only"
        } else if now - due > 60 {
            "missed_not_replayed"
        } else {
            "reserved"
        };
        if state.calls.len() >= 1024 {
            state.calls.remove(0);
        }
        let index = state.calls.len();
        state.calls.push(Call {
            slot,
            due,
            started_at: None,
            completed_at: None,
            result: if reason == "reserved" {
                "unknown"
            } else {
                "skipped"
            }
            .into(),
            reason: reason.into(),
            exit_code: None,
        });
        state.save(&store)?;
        if reason == "reserved" {
            let r = runtime(&store)?;
            // Holding the store lock through spawn makes pause/reservation/start atomic.
            match crate::codex::start(&store, &r) {
                Ok(running) => {
                    state.calls[index].started_at = Some(now);
                    state.calls[index].reason = "started".into();
                    state.save(&store)?;
                    drop(store);
                    let result = running.finish();
                    let store = Store::open(root)?;
                    let mut current = ScheduleState::load(&store)?;
                    if let Some(call) = current.calls.iter_mut().find(|c| c.due == due) {
                        call.completed_at = Some(Utc::now().timestamp());
                        call.result = if result.success { "success" } else { "failed" }.into();
                        call.reason = result.reason;
                        call.exit_code = result.exit_code;
                    }
                    if current.calls.len() > 1024 {
                        current.calls.drain(..current.calls.len() - 1024);
                    }
                    current.save(&store)?;
                    // Only one eligible call per tick; overdue slots are never replayed in a burst.
                    return Ok(());
                }
                Err(error) => {
                    state.calls[index].result = "skipped".into();
                    state.calls[index].reason = crate::codex::safe_reason(&error.to_string());
                }
            }
        }
        if state.calls.len() > 1024 {
            state.calls.drain(..state.calls.len() - 1024);
        }
        state.save(&store)?;
    }
    Ok(())
}
pub fn daemon(root: &Path, once: bool, observe: bool) -> Result<()> {
    let _lease = daemon_lease(root)?;
    signals();
    let mut after = Utc::now().timestamp();
    {
        let store = Store::open(root)?;
        crate::management::migrate(&store)?;
        let mut state = ScheduleState::load(&store)?;
        state.watermark = state.watermark.max(after);
        state.save(&store)?;
    }
    loop {
        let now = Utc::now().timestamp();
        if let Err(error) = tick(root, after, now, crate::timezone()?, observe) {
            eprintln!(
                "{}",
                json!({"event":"scheduler_error","error":crate::codex::safe_reason(&error.to_string())})
            );
        }
        after = after.max(now);
        if once || stopping() {
            break;
        }
        thread::sleep(Duration::from_secs(1));
    }
    Ok(())
}
pub fn replay(root: &Path, path: &Path, poll_ms: u64, hold: bool) -> Result<()> {
    if std::env::var_os("KAIRO_OFFLINE").as_deref() != Some(std::ffi::OsStr::new("1")) {
        bail!("offline_fixture_required");
    }
    let _lease = daemon_lease(root)?;
    signals();
    let v: Value = serde_json::from_slice(&fs::read(path)?)?;
    let start = v["start"].as_i64().context("fixture_start_required")?;
    let ticks = v["ticks"].as_array().context("fixture_ticks_required")?;
    let mut after = start;
    {
        let store = Store::open(root)?;
        crate::management::migrate(&store)?;
        if !present(&root.join("schedule.json"))? {
            unreachable!();
        }
        let mut state = ScheduleState::load(&store)?;
        // Fixture-only initial clock; never accepted by the production daemon.
        if v["initialize"] == true && state.calls.is_empty() {
            state.watermark = start;
            state.save(&store)?;
        }
    }
    for value in ticks {
        let now = value["now"].as_i64().context("fixture_time_required")?;
        tick(
            root,
            after,
            now,
            value["timezone"].as_str().unwrap_or("Etc/UTC").parse()?,
            false,
        )?;
        after = after.max(now);
        if stopping() {
            break;
        }
        thread::sleep(Duration::from_millis(poll_ms.min(60000)));
    }
    while hold && !stopping() {
        thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}
