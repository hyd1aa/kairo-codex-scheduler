//! Read-only, durable confirmation of a successful model request's five-hour countdown.
//! This module has no Codex executor and never changes the request ledger.
use crate::{identity, quota, runtime::verified_usage, Store};
use anyhow::{bail, Result};
use chrono::Utc;
use kairo_core::{Attempt, Quota, State, WINDOW};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;

const FILE: &str = "confirmations.json";
const FIRST_DELAY: i64 = 60;
const RETRY_DELAY: i64 = 90;
const MAX_CHECKS: u8 = 8;
const RESET_TOLERANCE: i64 = 120;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sample {
    pub checked_at: i64,
    pub reset_at: i64,
    pub remaining: i64,
    pub used_percent: f64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub at: i64,
    pub kind: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub anchor: String,
    pub request_at: i64,
    pub completed_at: i64,
    pub status: String,
    pub request_success: bool,
    pub countdown_confirmed: bool,
    pub time_correlated: bool,
    pub attribution_uncertain: bool,
    pub next_check: Option<i64>,
    pub deadline: i64,
    pub checks: u8,
    pub samples: Vec<Sample>,
    pub events: Vec<Event>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ledger {
    pub schema: u32,
    pub account: String,
    pub tasks: Vec<Task>,
}
impl Ledger {
    fn validate(&self, state: &State) -> Result<()> {
        if self.schema != 1 || self.account != state.account {
            bail!("confirmation_account_invalid / 确认任务账号或版本无效")
        }
        let mut seen = std::collections::BTreeSet::new();
        for task in &self.tasks {
            let attempt = state.attempts.iter().find(|a| a.anchor == task.anchor);
            if !seen.insert(&task.anchor)
                || attempt.is_none_or(|a| {
                    a.at != task.request_at
                        || a.completed_at != Some(task.completed_at)
                        || !request_succeeded(a)
                })
                || task.completed_at < task.request_at
                || task.deadline != task.completed_at.saturating_add(WINDOW)
                || task.checks > MAX_CHECKS
                || !task.request_success
                || (task.status == "pending") != task.next_check.is_some()
                || (task.status == "countdown_confirmed_time_correlated"
                    && (!task.countdown_confirmed
                        || !task.time_correlated
                        || !task.attribution_uncertain))
                || (task.status == "attribution_uncertain"
                    && (!task.countdown_confirmed
                        || task.time_correlated
                        || !task.attribution_uncertain))
                || (["pending", "timeout", "check_limit_reached"].contains(&task.status.as_str())
                    && (task.countdown_confirmed || task.time_correlated))
                || task
                    .next_check
                    .is_some_and(|at| at < task.completed_at || at > task.deadline)
                || (task.countdown_confirmed && task.samples.len() < 2)
                || task.samples.len() > usize::from(task.checks)
                || task.events.len() > usize::from(MAX_CHECKS) + 4
                || task
                    .samples
                    .windows(2)
                    .any(|w| w[1].checked_at < w[0].checked_at)
                || task.samples.iter().any(|s| {
                    s.checked_at < task.completed_at
                        || s.checked_at > task.deadline
                        || !(Quota {
                            duration: WINDOW,
                            used: s.used_percent,
                            reset_at: s.reset_at,
                            remaining: s.remaining,
                        })
                        .valid(s.checked_at, WINDOW)
                })
                || ![
                    "pending",
                    "countdown_confirmed_time_correlated",
                    "attribution_uncertain",
                    "timeout",
                    "check_limit_reached",
                ]
                .contains(&task.status.as_str())
            {
                bail!("confirmation_task_invalid / 确认任务记录无效")
            }
        }
        Ok(())
    }
}
fn request_succeeded(a: &Attempt) -> bool {
    matches!(
        a.outcome.as_str(),
        "request_ok_window_unconfirmed" | "five_hour_window_confirmed"
    )
}
fn new_task(a: &Attempt) -> Task {
    let completed = a
        .completed_at
        .expect("successful request has completion time");
    Task {
        anchor: a.anchor.clone(),
        request_at: a.at,
        completed_at: completed,
        status: "pending".into(),
        request_success: true,
        countdown_confirmed: false,
        time_correlated: false,
        attribution_uncertain: true,
        next_check: Some(completed.saturating_add(FIRST_DELAY)),
        deadline: completed.saturating_add(WINDOW),
        checks: 0,
        samples: Vec::new(),
        events: vec![Event {
            at: completed,
            kind: "request_success_window_unconfirmed".into(),
        }],
    }
}
pub fn read(store: &Store, state: &State) -> Result<Ledger> {
    let ledger = if store.root.join(FILE).exists() {
        store.read(FILE)?
    } else {
        Ledger {
            schema: 1,
            account: state.account.clone(),
            tasks: Vec::new(),
        }
    };
    let ledger: Ledger = ledger;
    ledger.validate(state)?;
    Ok(ledger)
}
pub fn sync(store: &Store, state: &State) -> Result<Ledger> {
    let mut ledger = read(store, state)?;
    let mut changed = false;
    for a in &state.attempts {
        if request_succeeded(a)
            && a.completed_at.is_some()
            && !ledger.tasks.iter().any(|t| t.anchor == a.anchor)
        {
            ledger.tasks.push(new_task(a));
            changed = true;
        }
    }
    if changed {
        store.write(FILE, &ledger)?;
    }
    Ok(ledger)
}
fn advance(task: &mut Task, now: i64, result: Result<(Quota, bool)>) {
    task.checks += 1;
    if now >= task.deadline {
        task.status = "timeout".into();
        task.next_check = None;
        task.events.push(Event {
            at: now,
            kind: "timeout".into(),
        });
        return;
    }
    match result {
        Ok((five, denied)) if !denied && five.valid(now, WINDOW) => {
            task.samples.push(Sample {
                checked_at: now,
                reset_at: five.reset_at,
                remaining: five.remaining,
                used_percent: five.used,
            });
            task.events.push(Event {
                at: now,
                kind: "quota_read_ok".into(),
            });
            if let Some(current) = task.samples.last() {
                if let Some(previous) =
                    task.samples[..task.samples.len() - 1]
                        .iter()
                        .rev()
                        .find(|s| {
                            s.reset_at == current.reset_at
                                && current.checked_at - s.checked_at >= 30
                        })
                {
                    let elapsed = current.checked_at - previous.checked_at;
                    let decrease = previous.remaining - current.remaining;
                    if elapsed > 0
                        && (elapsed - decrease).abs() <= 10
                        && current.reset_at == previous.reset_at
                    {
                        task.countdown_confirmed = true;
                        task.time_correlated =
                            (current.reset_at - task.deadline).abs() <= RESET_TOLERANCE;
                        // Matching timing is evidence, not exclusive proof of causation.
                        task.attribution_uncertain = true;
                        task.status = if task.time_correlated {
                            "countdown_confirmed_time_correlated"
                        } else {
                            "attribution_uncertain"
                        }
                        .into();
                        task.next_check = None;
                        task.events.push(Event {
                            at: now,
                            kind: task.status.clone(),
                        });
                        return;
                    }
                }
            }
        }
        Ok((_five, true)) => task.events.push(Event {
            at: now,
            kind: "server_denied".into(),
        }),
        Ok(_) => task.events.push(Event {
            at: now,
            kind: "quota_invalid".into(),
        }),
        Err(_) => task.events.push(Event {
            at: now,
            kind: "quota_or_identity_error".into(),
        }),
    }
    if now >= task.deadline {
        task.status = "timeout".into();
        task.next_check = None;
        task.events.push(Event {
            at: now,
            kind: "timeout".into(),
        });
    } else if task.checks >= MAX_CHECKS {
        task.status = "check_limit_reached".into();
        task.next_check = None;
        task.events.push(Event {
            at: now,
            kind: "check_limit_reached".into(),
        });
    } else {
        task.next_check = Some(now.saturating_add(RETRY_DELAY).min(task.deadline));
    }
}
pub fn run_due(store: &Store, state: &State, codex_home: &Path, now: i64) -> Result<()> {
    let mut ledger = sync(store, state)?;
    let account = ledger.account.clone();
    let Some(task) = ledger
        .tasks
        .iter_mut()
        .find(|t| t.status == "pending" && t.next_check.is_some_and(|due| due <= now))
    else {
        return Ok(());
    };
    // A task older than its five-hour window cannot be attributed safely.
    if now >= task.deadline {
        task.status = "timeout".into();
        task.next_check = None;
        task.events.push(Event {
            at: now,
            kind: "timeout".into(),
        });
    } else {
        let checked = (|| -> Result<(Quota, bool)> {
            let id = identity(&codex_home.join("auth.json"))?;
            if id.fingerprint != account {
                bail!("confirmation_account_mismatch / 确认任务账号不匹配")
            }
            let response = quota(&id)?;
            let (five, _weekly, denied) = verified_usage(&id, &response, Utc::now().timestamp())?;
            Ok((five, denied))
        })();
        advance(task, Utc::now().timestamp(), checked);
    }
    let event = task.events.last().cloned();
    store.write(FILE, &ledger)?;
    if let Some(e) = event {
        eprintln!(
            "{}",
            json!({"event":"window_confirmation","kind":e.kind,"at":e.at})
        );
        if [
            "timeout",
            "check_limit_reached",
            "server_denied",
            "quota_or_identity_error",
        ]
        .contains(&e.kind.as_str())
        {
            eprintln!(
                "{}",
                json!({"event":"confirmation_safety_warning","reason":e.kind,"at":e.at})
            );
        }
    }
    Ok(())
}
pub fn summary(store: &Store, state: &State, tz: chrono_tz::Tz) -> Result<Value> {
    let ledger = read(store, state)?;
    let request = state.attempts.last();
    let latest = request.and_then(|a| {
        ledger
            .tasks
            .iter()
            .find(|t| t.anchor == a.anchor && t.request_at == a.at)
    });
    let local = |at: i64| {
        chrono::DateTime::from_timestamp(at, 0).map(|d| d.with_timezone(&tz).to_rfc3339())
    };
    Ok(
        json!({"recent_request_at":state.attempts.last().and_then(|a|local(a.at)),
        "recent_request_outcome":state.attempts.last().map(|a|a.outcome.as_str()),
        "confirmation_status":latest.map(|t|t.status.as_str()),
        "request_success":request.and_then(|a|if a.outcome=="attempt_persisted_unknown" {None}else{Some(request_succeeded(a))}),
        "countdown_confirmed":latest.map(|t|t.countdown_confirmed),
        "time_correlated":latest.map(|t|t.time_correlated),
        "attribution_uncertain":latest.map(|t|t.attribution_uncertain),
        "last_confirmation_check":latest.and_then(|t|t.events.iter().rev().find(|e|["quota_read_ok","quota_invalid","quota_or_identity_error","server_denied"].contains(&e.kind.as_str())).map(|e|e.at)).and_then(local),
        "pending_confirmation":ledger.tasks.iter().any(|t|t.status=="pending"),
        "next_confirmation_check":latest.and_then(|t|t.next_check).and_then(local),
        "confirmation_checks":latest.map(|t|t.checks),
        "confirmation_max_checks":MAX_CHECKS,
        "confirmation_last_event":latest.and_then(|t|t.events.last()).map(|e|e.kind.as_str()),
        "confirmation_last_error":latest.and_then(|t|t.events.last()).and_then(|e| if ["server_denied","quota_invalid","quota_or_identity_error","timeout","check_limit_reached"].contains(&e.kind.as_str()) {Some(e.kind.as_str())} else {None}),
        "confirmation_history":ledger.tasks.len()}),
    )
}
pub fn details(store: &Store, state: &State, tz: chrono_tz::Tz) -> Result<Value> {
    let ledger = read(store, state)?;
    let Some(task) = ledger.tasks.last() else {
        return Ok(Value::Null);
    };
    let local =
        |at| chrono::DateTime::from_timestamp(at, 0).map(|d| d.with_timezone(&tz).to_rfc3339());
    Ok(
        json!({"anchor":task.anchor,"request_at":local(task.request_at),"completed_at":local(task.completed_at),
        "status":task.status,"deadline":local(task.deadline),"check_count":task.checks,
        "samples":task.samples.iter().map(|s|json!({"checked_at":local(s.checked_at),"reset_at":local(s.reset_at),"remaining_seconds":s.remaining,"used_percent":s.used_percent})).collect::<Vec<_>>(),
        "events":task.events.iter().map(|e|json!({"at":local(e.at),"kind":e.kind})).collect::<Vec<_>>() }),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    fn attempt() -> Attempt {
        Attempt {
            anchor: "2026-10-20/06:00".into(),
            at: 1000,
            completed_at: Some(1006),
            before: Quota {
                duration: WINDOW,
                used: 0.,
                reset_at: 19000,
                remaining: 18000,
            },
            after: None,
            outcome: "request_ok_window_unconfirmed".into(),
        }
    }
    fn q(now: i64, reset: i64, used: f64) -> Quota {
        Quota {
            duration: WINDOW,
            used,
            reset_at: reset,
            remaining: reset - now,
        }
    }
    #[test]
    fn zero_percent_stable_countdown_correlates() {
        let mut t = new_task(&attempt());
        advance(&mut t, 6348, Ok((q(6348, 19006, 0.), false)));
        advance(&mut t, 6997, Ok((q(6997, 19006, 0.), false)));
        assert_eq!(t.status, "countdown_confirmed_time_correlated");
        assert!(t.countdown_confirmed && t.time_correlated && t.attribution_uncertain);
        assert_eq!(t.checks, 2);
    }
    #[test]
    fn real_20261020_delayed_zero_percent_samples() {
        let mut a = attempt();
        a.at = 1792447230;
        a.completed_at = Some(1792447236);
        let mut t = new_task(&a);
        advance(
            &mut t,
            1792453578,
            Ok((
                Quota {
                    duration: WINDOW,
                    used: 0.,
                    reset_at: 1792465236,
                    remaining: 11658,
                },
                false,
            )),
        );
        advance(
            &mut t,
            1792454227,
            Ok((
                Quota {
                    duration: WINDOW,
                    used: 0.,
                    reset_at: 1792465236,
                    remaining: 11009,
                },
                false,
            )),
        );
        assert_eq!(t.status, "countdown_confirmed_time_correlated");
        assert!(t.countdown_confirmed && t.time_correlated && t.attribution_uncertain);
        assert_eq!(t.samples[0].remaining - t.samples[1].remaining, 649);
    }
    #[test]
    fn unrelated_countdown_is_uncertain() {
        let mut t = new_task(&attempt());
        advance(&mut t, 1200, Ok((q(1200, 18800, 0.), false)));
        advance(&mut t, 1290, Ok((q(1290, 18800, 0.), false)));
        assert_eq!(t.status, "attribution_uncertain");
        assert!(t.countdown_confirmed && !t.time_correlated);
    }
    #[test]
    fn errors_timeout_and_limit_never_create_attempts() {
        let mut t = new_task(&attempt());
        for i in 0..MAX_CHECKS {
            advance(
                &mut t,
                1100 + i64::from(i) * 90,
                Err(anyhow::anyhow!("synthetic")),
            );
        }
        assert_eq!(t.status, "check_limit_reached");
        assert!(t.next_check.is_none());
        let mut t = new_task(&attempt());
        advance(&mut t, 19006, Ok((q(19006, 19006, 0.), false)));
        assert_eq!(t.status, "timeout");
    }
    #[test]
    fn rolling_reset_or_bad_elapsed_never_confirms() {
        let mut t = new_task(&attempt());
        advance(&mut t, 1200, Ok((q(1200, 19006, 0.), false)));
        advance(&mut t, 1290, Ok((q(1290, 19100, 0.), false)));
        assert!(!t.countdown_confirmed);
        advance(
            &mut t,
            1380,
            Ok((
                Quota {
                    duration: WINDOW,
                    used: 0.,
                    reset_at: 19100,
                    remaining: 17500,
                },
                false,
            )),
        );
        assert!(!t.countdown_confirmed);
        assert_eq!(t.status, "pending");
    }
    #[test]
    fn delayed_api_data_and_server_denial_cannot_confirm() {
        let mut t = new_task(&attempt());
        // The response describes an earlier instant, not the completed HTTP check.
        advance(&mut t, 1200, Ok((q(1100, 19006, 0.), false)));
        assert!(t.samples.is_empty());
        advance(&mut t, 1290, Ok((q(1290, 19006, 0.), true)));
        assert!(t.samples.is_empty());
        assert_eq!(t.events.last().unwrap().kind, "server_denied");
        advance(&mut t, 1380, Ok((q(1380, 19006, 0.), false)));
        assert!(!t.countdown_confirmed);
        assert_eq!(t.status, "pending");
    }
    #[test]
    fn restart_restores_task_and_timeout_does_not_change_request() {
        let root = std::env::temp_dir().join(format!("kairo-confirmation-{}", std::process::id()));
        let store = Store::open(&root).unwrap();
        let state = State {
            schema: 1,
            account: "synthetic".into(),
            last_clock: 1006,
            attempts: vec![attempt()],
            ..Default::default()
        };
        state.validate().unwrap();
        store.write("state.json", &state).unwrap();
        assert_eq!(sync(&store, &state).unwrap().tasks.len(), 1);
        assert_eq!(sync(&store, &state).unwrap().tasks.len(), 1);
        drop(store);
        let store = Store::open(&root).unwrap();
        assert_eq!(read(&store, &state).unwrap().tasks[0].status, "pending");
        run_due(&store, &state, Path::new("/nonexistent-auth"), 19006).unwrap();
        let ledger = read(&store, &state).unwrap();
        assert_eq!(ledger.tasks[0].status, "timeout");
        assert!(
            summary(&store, &state, chrono_tz::UTC).unwrap()["last_confirmation_check"].is_null()
        );
        let event_count = ledger.tasks[0].events.len();
        run_due(&store, &state, Path::new("/nonexistent-auth"), 19007).unwrap();
        assert_eq!(
            read(&store, &state).unwrap().tasks[0].events.len(),
            event_count
        );
        assert_eq!(store.state().unwrap().attempts.len(), 1);
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn corrupt_confirmation_or_write_failure_preserves_attempt() {
        let root =
            std::env::temp_dir().join(format!("kairo-confirmation-fault-{}", std::process::id()));
        let store = Store::open(&root).unwrap();
        let state = State {
            schema: 1,
            account: "synthetic".into(),
            last_clock: 1006,
            attempts: vec![attempt()],
            ..Default::default()
        };
        store.write("state.json", &state).unwrap();
        fs::create_dir(root.join(FILE)).unwrap();
        assert!(sync(&store, &state).is_err());
        assert_eq!(store.state().unwrap().attempts.len(), 1);
        fs::remove_dir(root.join(FILE)).unwrap();
        store
            .write(FILE, &serde_json::json!({"schema":99}))
            .unwrap();
        assert!(read(&store, &state).is_err());
        assert_eq!(store.state().unwrap().attempts.len(), 1);
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }
}
