use crate::Store;
use anyhow::{bail, Result};
use chrono_tz::Tz;
use kairo_core::{complete, decide, reserve, Config, Cursor, Decision, Quota, State};
pub trait Adapter {
    fn clock(&self) -> Option<i64> {
        None
    }
    fn account_guard_until(&self) -> Option<i64> {
        None
    }
    fn enabled(&self) -> bool {
        true
    }
    fn account(&mut self) -> Result<String>;
    fn usage(&mut self) -> Result<(Quota, Quota, bool)>;
    fn request(&mut self) -> Result<bool>;
}
/// All adapters, including offline fixtures, pass the same durable reservation gate.
/// Caller holds the state lock and a canonical account lock throughout this function.
#[allow(clippy::too_many_arguments)]
pub fn tick<A: Adapter>(
    store: &Store,
    c: &Config,
    cursor: &mut Cursor,
    state: &mut State,
    tz: Tz,
    now: i64,
    adapter: &mut A,
) -> Result<String> {
    c.validate()?;
    state.validate()?;
    if cursor.schema != 1 || state.schema != 1 {
        bail!("Schema invalid / 状态版本无效")
    }
    if now < state.last_clock || now < cursor.observed {
        bail!("Clock moved backward; blocked / 时钟回拨，已阻止")
    }
    let account = adapter.account()?;
    if account != state.account {
        bail!("Account changed; blocked / 账号变化，已阻止")
    }
    if cursor.config_signature.is_empty() {
        cursor.config_signature = kairo_core::config_signature(c);
    }
    if !cursor.config_signature.is_empty()
        && cursor.config_signature != kairo_core::config_signature(c)
    {
        let anchor = cursor.anchor.clone();
        cursor.replan(c, tz, now)?;
        if state.pending.is_some() {
            state.pending = if cursor.anchor == anchor {
                cursor.due
            } else {
                None
            };
            store.write("state.json", state)?;
        }
        store.write("cursor.json", cursor)?;
        return Ok("config_replanned".into());
    }
    if cursor.timezone != tz.to_string() {
        cursor.replan(c, tz, now)?;
        state.pending = None;
        store.write("state.json", state)?;
        store.write("cursor.json", cursor)?;
        return Ok("timezone_replanned".into());
    }
    if cursor.due.is_none() {
        cursor.arm(c, tz, now)?;
        store.write("cursor.json", cursor)?;
        return Ok("armed".into());
    }
    let due = cursor.due.unwrap();
    if now.saturating_sub(due) > 60 || now >= cursor.cutoff {
        state.pending = None;
        store.write("state.json", state)?;
        cursor.finish();
        cursor.arm(c, tz, now)?;
        store.write("cursor.json", cursor)?;
        return Ok("missed_not_replayed".into());
    }
    let covered = state.pending.is_some() && cursor.cover_due_anchors(c, tz, now)?;
    if now < due && !covered {
        cursor.observed = now;
        store.write("cursor.json", cursor)?;
        return Ok("waiting".into());
    }
    let (f, w, denied) = adapter.usage()?;
    let anchor = cursor
        .anchor
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Anchor missing / 锚点缺失"))?;
    let mut decision = decide(c, state, &anchor, now, cursor.cutoff, &f, &w, denied);
    if let Some(until) = adapter.account_guard_until().filter(|t| *t > now) {
        decision = match decision {
            Decision::Request(_) => Decision::Defer(until),
            Decision::Defer(at) => Decision::Defer(at.max(until)),
            block => block,
        };
    }
    if f.valid(now, 18000) {
        state.weekly = Some(w.clone());
        state.server_denied = denied;
        kairo_core::observe(state, now, f.clone());
        store.write("state.json", state)?;
    }

    let outcome = match decision {
        Decision::Request(reason) => {
            if !adapter.enabled() {
                state.pending = None;
                store.write("state.json", state)?;
                cursor.finish();
                cursor.arm(c, tz, now)?;
                store.write("cursor.json", cursor)?;
                return Ok("eligible_observation_only".into());
            }
            reserve(state, &anchor, now, f);
            store.write("state.json", state)?;
            let ok = adapter.request().unwrap_or(false);
            let after = adapter.usage().ok().map(|(f, _, _)| f);
            complete(state, ok, after);
            if let Some(end) = adapter.clock() {
                kairo_core::completed_at(state, end);
            }

            store.write("state.json", state)?;
            format!("{reason}:{}", state.attempts.last().unwrap().outcome)
        }
        Decision::Defer(at) => {
            if f.used > 0.0 {
                state.positive_until = state.positive_until.max(f.reset_at);
                store.write("state.json", state)?;
            }
            if at < cursor.cutoff && at > now {
                state.pending = Some(at);
                store.write("state.json", state)?;
                cursor.due = Some(at);
                cursor.observed = now;
                store.write("cursor.json", cursor)?;
                return Ok("deferred".into());
            }
            "cutoff".into()
        }
        Decision::Block(reason) => reason.into(),
    };
    state.pending = None;
    store.write("state.json", state)?;
    cursor.finish();
    cursor.arm(c, tz, now)?;
    store.write("cursor.json", cursor)?;
    Ok(outcome)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    struct Mock {
        now: i64,
        calls: u32,
        fail: bool,
    }
    impl Adapter for Mock {
        fn account(&mut self) -> Result<String> {
            Ok("synthetic".into())
        }
        fn usage(&mut self) -> Result<(Quota, Quota, bool)> {
            let q = |d| Quota {
                duration: d,
                used: 0.,
                reset_at: self.now + d,
                remaining: d,
            };
            Ok((q(18000), q(604800), false))
        }
        fn request(&mut self) -> Result<bool> {
            self.calls += 1;
            Ok(!self.fail)
        }
    }
    #[test]
    fn durable_end_to_end() {
        let root = std::env::temp_dir().join(format!("kairo-engine-{}", std::process::id()));
        let store = Store::open(&root).unwrap();
        let c = Config::default();
        let n = 1792387044;
        let mut s = State {
            schema: 1,
            account: "synthetic".into(),
            ..Default::default()
        };
        let mut cur = Cursor::new(n, chrono_tz::UTC);
        cur.anchor = Some("2026-10-19/11:00".into());
        cur.due = Some(n);
        cur.cutoff = n + 10000;
        let mut mock = Mock {
            now: n,
            calls: 0,
            fail: false,
        };
        let out = tick(&store, &c, &mut cur, &mut s, chrono_tz::UTC, n, &mut mock).unwrap();
        assert!(out.ends_with("request_ok_window_unconfirmed"));
        assert_eq!(store.state().unwrap().attempts.len(), 1);
        tick(
            &store,
            &c,
            &mut cur,
            &mut s,
            chrono_tz::UTC,
            n + 1,
            &mut mock,
        )
        .unwrap();
        assert_eq!(mock.calls, 1);
        assert!(s.pending.is_none());
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn state_write_failure_prevents_call() {
        let root = std::env::temp_dir().join(format!("kairo-failure-{}", std::process::id()));
        let store = Store::open(&root).unwrap();
        fs::create_dir(root.join("state.json")).unwrap();
        let c = Config::default();
        let n = 100000;
        let mut s = State {
            schema: 1,
            account: "synthetic".into(),
            ..Default::default()
        };
        let mut cur = Cursor::new(n, chrono_tz::UTC);
        cur.anchor = Some("a".into());
        cur.due = Some(n);
        cur.cutoff = n + 10000;
        let mut mock = Mock {
            now: n,
            calls: 0,
            fail: false,
        };
        assert!(tick(&store, &c, &mut cur, &mut s, chrono_tz::UTC, n, &mut mock).is_err());
        assert_eq!(mock.calls, 0);
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }
}
#[cfg(test)]
mod regression {
    use super::*;
    struct Timeline {
        now: i64,
        calls: u32,
        active: bool,
    }
    impl Adapter for Timeline {
        fn clock(&self) -> Option<i64> {
            Some(self.now)
        }
        fn account(&mut self) -> Result<String> {
            Ok("synthetic".into())
        }
        fn usage(&mut self) -> Result<(Quota, Quota, bool)> {
            let five = if self.active {
                Quota {
                    duration: 18000,
                    used: 2.,
                    reset_at: 1792387014,
                    remaining: 1792387014 - self.now,
                }
            } else {
                Quota {
                    duration: 18000,
                    used: 0.,
                    reset_at: self.now + 18000,
                    remaining: 18000,
                }
            };
            Ok((
                five,
                Quota {
                    duration: 604800,
                    used: 49.,
                    reset_at: self.now + 604800,
                    remaining: 604800,
                },
                false,
            ))
        }
        fn request(&mut self) -> Result<bool> {
            self.calls += 1;
            self.now += 5;
            Ok(true)
        }
    }
    #[test]
    fn accepted_130_timeline() {
        let path = std::env::temp_dir().join(format!("kairo-timeline-{}", std::process::id()));
        let store = Store::open(&path).unwrap();
        let c = Config::default();
        let tz = chrono_tz::Asia::Shanghai;
        let start = 1792378830;
        let mut cursor = Cursor::new(start, tz);
        cursor.anchor = Some("2026-10-19/11:00".into());
        cursor.due = Some(start);
        cursor.cutoff = 1792425600;
        let mut state = State {
            schema: 1,
            account: "synthetic".into(),
            ..Default::default()
        };
        let mut adapter = Timeline {
            now: start,
            calls: 0,
            active: true,
        };
        assert_eq!(
            tick(&store, &c, &mut cursor, &mut state, tz, start, &mut adapter).unwrap(),
            "deferred"
        );
        assert_eq!(cursor.due, Some(1792387044));
        assert_eq!(adapter.calls, 0);
        adapter.now = 1792387044;
        adapter.active = false;
        assert_eq!(
            tick(
                &store,
                &c,
                &mut cursor,
                &mut state,
                tz,
                1792387044,
                &mut adapter
            )
            .unwrap(),
            "rolled_zero_usage:request_ok_window_unconfirmed"
        );
        assert_eq!(adapter.calls, 1);
        assert_eq!(state.attempts[0].at, 1792387044);
        assert!(state.pending.is_none());
        tick(
            &store,
            &c,
            &mut cursor,
            &mut state,
            tz,
            1792387049,
            &mut adapter,
        )
        .unwrap();
        assert_eq!(adapter.calls, 1);
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
    }
}
#[cfg(test)]
mod reload_tests {
    use super::*;
    struct NoRequest;
    impl Adapter for NoRequest {
        fn account(&mut self) -> Result<String> {
            Ok("synthetic".into())
        }
        fn usage(&mut self) -> Result<(Quota, Quota, bool)> {
            bail!("must not query")
        }
        fn request(&mut self) -> Result<bool> {
            panic!("must not request")
        }
    }
    #[test]
    fn reload_timezone_and_clock_fail_closed() {
        let root = std::env::temp_dir().join(format!("kairo-reload-{}", std::process::id()));
        let store = Store::open(&root).unwrap();
        let now = 1792387044;
        let mut s = State {
            schema: 1,
            account: "synthetic".into(),
            ..Default::default()
        };
        let mut c = Config::default();
        let mut cursor = Cursor::new(now, chrono_tz::UTC);
        cursor.arm(&c, chrono_tz::UTC, now).unwrap();
        let mut adapter = NoRequest;
        c.anchors.push("02:00".into());
        assert_eq!(
            tick(
                &store,
                &c,
                &mut cursor,
                &mut s,
                chrono_tz::UTC,
                now + 1,
                &mut adapter
            )
            .unwrap(),
            "config_replanned"
        );
        assert_eq!(
            tick(
                &store,
                &c,
                &mut cursor,
                &mut s,
                chrono_tz::Asia::Tokyo,
                now + 2,
                &mut adapter
            )
            .unwrap(),
            "timezone_replanned"
        );
        assert!(tick(
            &store,
            &c,
            &mut cursor,
            &mut s,
            chrono_tz::Asia::Tokyo,
            now - 1,
            &mut adapter
        )
        .is_err());
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
}
#[cfg(test)]
mod dense_tests {
    use super::*;
    struct Active {
        now: i64,
        reads: u32,
    }
    impl Adapter for Active {
        fn account(&mut self) -> Result<String> {
            Ok("synthetic".into())
        }
        fn usage(&mut self) -> Result<(Quota, Quota, bool)> {
            self.reads += 1;
            let reset = self.now + 3600;
            Ok((
                Quota {
                    duration: 18000,
                    used: 2.,
                    reset_at: reset,
                    remaining: 3600,
                },
                Quota {
                    duration: 604800,
                    used: 20.,
                    reset_at: self.now + 604800,
                    remaining: 604800,
                },
                false,
            ))
        }
        fn request(&mut self) -> Result<bool> {
            panic!("active window must not invoke")
        }
    }
    #[test]
    fn dense_anchor_checks_do_not_discard_pending_reset() {
        let root = std::env::temp_dir().join(format!("kairo-dense-{}", std::process::id()));
        let store = Store::open(&root).unwrap();
        let c = Config {
            anchors: vec!["11:00".into(), "12:00".into(), "14:00".into()],
            ..Default::default()
        };
        let due = "2026-10-19T12:00:30Z"
            .parse::<chrono::DateTime<chrono::Utc>>()
            .unwrap()
            .timestamp();
        let mut cursor = Cursor::new(due - 1, chrono_tz::UTC);
        cursor.anchor = Some("2026-10-19/11:00".into());
        cursor.due = Some(due + 3600);
        cursor.cutoff = "2026-10-20T00:00:00Z"
            .parse::<chrono::DateTime<chrono::Utc>>()
            .unwrap()
            .timestamp();
        cursor.config_signature = kairo_core::config_signature(&c);
        let mut state = State {
            schema: 1,
            account: "synthetic".into(),
            pending: cursor.due,
            ..Default::default()
        };
        let mut adapter = Active { now: due, reads: 0 };
        assert_eq!(
            tick(
                &store,
                &c,
                &mut cursor,
                &mut state,
                chrono_tz::UTC,
                due,
                &mut adapter
            )
            .unwrap(),
            "deferred"
        );
        assert_eq!(adapter.reads, 1);
        assert!(cursor.finished.contains("2026-10-19/12:00"));
        assert_eq!(cursor.anchor.as_deref(), Some("2026-10-19/11:00"));
        assert_eq!(state.pending, Some(due + 3630));
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
}
