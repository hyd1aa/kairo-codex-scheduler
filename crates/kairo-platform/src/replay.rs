//! Process-level offline daemon harness. It has no HTTP or real executable adapter.
use crate::{
    codex::{AccountGate, AccountJournal},
    engine::{self, Adapter},
    Store,
};
use anyhow::{bail, Context, Result};
use chrono_tz::Tz;
use kairo_core::{Cursor, Quota, State};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, path::Path, thread, time::Duration};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tick {
    pub now: i64,
    #[serde(default)]
    pub completed_at: Option<i64>,
    pub timezone: String,
    pub usage: Value,
    #[serde(default = "yes")]
    pub request_ok: bool,
    #[serde(default)]
    pub crash_after_reserve: bool,
    #[serde(default)]
    pub after: Option<Value>,
}
fn yes() -> bool {
    true
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Replay {
    pub state: State,
    pub cursor: Option<Cursor>,
    pub ticks: Vec<Tick>,
}
struct Fake<'a> {
    store: &'a Store,
    tick: &'a Tick,
    account: String,
    gate: AccountGate,
    calls: u32,
    after: bool,
    anchor: String,
    buffer: i64,
}
impl Adapter for Fake<'_> {
    fn clock(&self) -> Option<i64> {
        Some(if self.after {
            self.tick.completed_at.unwrap_or(self.tick.now)
        } else {
            self.tick.now
        })
    }
    fn account_guard_until(&self) -> Option<i64> {
        Some(self.gate.not_before(self.buffer))
    }

    fn account(&mut self) -> Result<String> {
        Ok(self.account.clone())
    }
    fn usage(&mut self) -> Result<(Quota, Quota, bool)> {
        crate::parse_usage(
            if self.after {
                self.tick.after.as_ref().unwrap_or(&self.tick.usage)
            } else {
                &self.tick.usage
            },
            self.tick.now,
        )
    }
    fn request(&mut self) -> Result<bool> {
        let s = self.store.state()?;
        let a = s
            .attempts
            .last()
            .context("reservation_missing / 缺少调用前记录")?;
        if a.anchor != self.anchor || a.at != self.tick.now {
            bail!("reservation_invalid / 调用前记录无效")
        };
        self.gate.reserve_budget(
            &self.anchor,
            self.tick.now,
            self.buffer,
            self.tick.completed_at.unwrap_or(self.tick.now),
        )?;
        if self.tick.crash_after_reserve {
            std::process::exit(86)
        };
        self.calls += 1;
        self.after = true;
        Ok(self.tick.request_ok)
    }
}
pub fn run(root: &Path, path: &Path, poll_ms: u64, hold: bool) -> Result<()> {
    fs::create_dir_all(root)?;
    let root = fs::canonicalize(root)?;
    if !root.starts_with("/tmp") && !root.starts_with("/opt/Kairo/sandbox") {
        bail!("fixture_path_required / 模拟测试必须使用隔离目录")
    }
    let _lease = crate::runtime::daemon_lease(&root)?;
    crate::runtime::install_signals();
    let fixture: Replay = serde_json::from_slice(&fs::read(path)?)?;
    if fixture.ticks.is_empty() {
        bail!("fixture_empty / 模拟时间线为空")
    };
    fixture.state.validate()?;
    let first = &fixture.ticks[0];
    let tz: Tz = first.timezone.parse()?;
    let guard_root = root
        .parent()
        .context("fixture_path_required / 模拟测试必须使用隔离目录")?
        .join(".fixture-account-guards");
    let guardpath = guard_root.join(&fixture.state.account);
    {
        let store = Store::open(&root)?;
        let fresh = !root.join(".fixture-initialized").exists();
        if fresh {
            if root.join("state.json").exists() {
                bail!("fixture_state_exists / 模拟状态已存在")
            };
            if !guardpath.exists() {
                let guard = Store::open(&guardpath)?;
                guard.write(
                    "account.json",
                    &AccountJournal {
                        schema: 1,
                        account: fixture.state.account.clone(),
                        last_attempt: 0,
                        anchors: Default::default(),
                    },
                )?;
            }
            store.write("state.json", &fixture.state)?;
            let cur = if let Some(c) = fixture.cursor {
                c
            } else {
                let mut c = Cursor::new(first.now, tz);
                c.arm(&store.config()?, tz, first.now)?;
                c
            };
            store.write("cursor.json", &cur)?;
            store.write(".fixture-initialized", &true)?;
        } else {
            let s = store.state()?;
            if s.account != fixture.state.account {
                bail!("account_mismatch / 账号不匹配")
            };
            let mut cur: Cursor = store.read("cursor.json")?;
            cur.resume(&store.config()?, tz, first.now)?;
            store.write("cursor.json", &cur)?;
        }
    }
    let mut calls = 0;
    for tick in fixture.ticks {
        if crate::runtime::stopping() {
            break;
        }
        let store = Store::open(&root)?;
        let c = store.config()?;
        let tz: Tz = tick.timezone.parse()?;
        let mut s = store.state()?;
        let mut cur: Cursor = store.read("cursor.json")?;
        let mut fake = Fake {
            store: &store,
            tick: &tick,
            account: s.account.clone(),
            gate: AccountGate::open_existing(&guard_root, &s.account)?,
            calls: 0,
            after: false,
            anchor: cur.anchor.clone().unwrap_or_default(),
            buffer: c.buffer_seconds,
        };
        let event = engine::tick(&store, &c, &mut cur, &mut s, tz, tick.now, &mut fake)?;
        calls += fake.calls;
        let value = json!({"event":event,"at":tick.now,"timezone":tz.to_string(),"next_check":cur.due,"simulated_calls":calls,"model_requests":0});
        store.write("status.json", &value)?;
        println!("{}", value);
        drop(fake);
        drop(store);
        thread::sleep(Duration::from_millis(poll_ms.min(60000)));
    }
    while hold && !crate::runtime::stopping() {
        thread::sleep(Duration::from_millis(50))
    }
    Ok(())
}
