use super::*;
fn state() -> State {
    State {
        schema: 1,
        account: "synthetic".into(),
        ..Default::default()
    }
}
fn q(n: i64, d: i64, u: f64) -> Quota {
    Quota {
        duration: d,
        used: u,
        reset_at: n + d,
        remaining: d,
    }
}
fn decision(c: &Config, s: &State, f: &Quota, w: &Quota) -> Decision {
    decide(c, s, "new", 100000, 150000, f, w, false)
}
#[test]
fn weekly_exhaustion_blocks() {
    assert_eq!(
        decision(
            &Config::default(),
            &state(),
            &q(100000, WINDOW, 0.),
            &q(100000, 604800, 100.)
        ),
        Decision::Block("quota_invalid_or_weekly_exhausted")
    );
}
#[test]
fn clock_reversal_blocks() {
    let mut s = state();
    s.last_clock = 100001;
    assert_eq!(
        decision(
            &Config::default(),
            &s,
            &q(100000, WINDOW, 0.),
            &q(100000, 604800, 1.)
        ),
        Decision::Block("state_or_clock")
    );
}
#[test]
fn missing_state_blocks() {
    assert_eq!(
        decision(
            &Config::default(),
            &State::default(),
            &q(100000, WINDOW, 0.),
            &q(100000, 604800, 1.)
        ),
        Decision::Block("state_or_clock")
    );
}
#[test]
fn malformed_attempt_blocks() {
    let mut s = state();
    reserve(&mut s, "old", 1, q(1, WINDOW, 0.));
    s.attempts[0].outcome = "unknown-schema".into();
    assert!(s.validate().is_err());
}
#[test]
fn disabled_idle_defers() {
    let c = Config {
        idle_window_prewarm: false,
        ..Default::default()
    };
    assert_eq!(
        decision(&c, &state(), &q(100000, WINDOW, 0.), &q(100000, 604800, 1.)),
        Decision::Defer(118030)
    );
}
#[test]
fn expired_buffer_is_enforced() {
    let c = Config::default();
    let mut f = q(100000, WINDOW, 0.);
    f.reset_at = 99980;
    f.remaining = 0;
    assert_eq!(
        decision(&c, &state(), &f, &q(100000, 604800, 1.)),
        Decision::Defer(100010)
    );
    f.reset_at = 99970;
    assert_eq!(
        decision(&c, &state(), &f, &q(100000, 604800, 1.)),
        Decision::Request("expired")
    );
}
#[test]
fn rounded_zero_is_insufficient() {
    let mut f = q(100000, WINDOW, 0.);
    f.remaining = 10000;
    f.reset_at = 110000;
    assert_eq!(
        decision(&Config::default(), &state(), &f, &q(100000, 604800, 1.)),
        Decision::Defer(110030)
    );
}
#[test]
fn failed_attempts_also_block() {
    for outcome in [
        "request_failed",
        "request_ok_window_unconfirmed",
        "attempt_persisted_unknown",
    ] {
        let mut s = state();
        reserve(&mut s, "other", 99990, q(99990, WINDOW, 0.));
        s.attempts[0].outcome = outcome.into();
        assert_eq!(
            decision(
                &Config::default(),
                &s,
                &q(100000, WINDOW, 0.),
                &q(100000, 604800, 1.)
            ),
            Decision::Defer(118020)
        );
    }
}
#[test]
fn no_hardcoded_daily_limit() {
    let mut s = state();
    for i in 0..6 {
        let n = 1 + i * 18031;
        reserve(&mut s, &format!("old-{i}"), n, q(n, WINDOW, 0.));
    }
    assert_eq!(
        decide(
            &Config::default(),
            &s,
            "seventh",
            130000,
            150000,
            &q(130000, WINDOW, 0.),
            &q(130000, 604800, 1.),
            false
        ),
        Decision::Request("rolled_zero_usage")
    );
}
#[test]
fn cutoff_is_absolute() {
    assert_eq!(
        decide(
            &Config::default(),
            &state(),
            "a",
            100000,
            100000,
            &q(100000, WINDOW, 0.),
            &q(100000, 604800, 1.),
            false
        ),
        Decision::Block("daily_cutoff")
    );
}
#[test]
fn positive_history_defers() {
    let mut s = state();
    s.positive_until = 100100;
    assert_eq!(
        decision(
            &Config::default(),
            &s,
            &q(100000, WINDOW, 0.),
            &q(100000, 604800, 1.)
        ),
        Decision::Defer(100130)
    );
}
#[test]
fn malformed_timestamps_do_not_panic() {
    let mut f = q(1, WINDOW, 0.);
    f.reset_at = i64::MIN;
    assert!(!f.valid(i64::MAX, WINDOW));
    f.used = f64::NAN;
    assert!(!f.valid(1, WINDOW));
}
#[test]
fn autumn_fold_only_once() {
    let c = Config {
        anchors: vec!["01:30".into()],
        ..Default::default()
    };
    let now = "2026-11-01T04:00:00Z".parse().unwrap();
    let v = plan(&c, chrono_tz::America::New_York, now, 24).unwrap();
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].to_rfc3339(), "2026-11-01T05:30:30+00:00");
}
#[test]
fn changed_timezone_does_not_replay_anchor() {
    let c = Config::default();
    let n = "2026-10-19T00:00:00Z"
        .parse::<DateTime<Utc>>()
        .unwrap()
        .timestamp();
    let mut cursor = Cursor::new(n, chrono_tz::Asia::Shanghai);
    cursor.finished.insert("2026-10-19/06:00".into());
    cursor.arm(&c, chrono_tz::UTC, n).unwrap();
    assert_eq!(cursor.anchor.as_deref(), Some("2026-10-19/11:00"));
}
#[test]
fn no_past_schedule() {
    let c = Config::default();
    let n = "2026-10-19T11:00:31Z".parse().unwrap();
    let v = plan(&c, chrono_tz::UTC, n, 24).unwrap();
    assert_eq!(v[0].to_rfc3339(), "2026-10-19T16:00:30+00:00");
}
#[test]
fn anchor_limit_and_buffer() {
    let c = Config {
        anchors: (0..49)
            .map(|i| format!("{:02}:{:02}", i / 6, i % 6 * 10))
            .collect(),
        ..Default::default()
    };
    assert!(c.validate().is_err());
    for b in [-1, 301] {
        assert!(Config {
            buffer_seconds: b,
            ..Default::default()
        }
        .validate()
        .is_err());
    }
}
#[test]
fn ascii_time_only() {
    for t in [" 1:00", "01: 0", "01:0a", "24:00", "12:60"] {
        assert!(time(t).is_err(), "{t}");
    }
}
#[test]
fn restart_skips_overdue_pending() {
    let c = Config::default();
    let n = "2026-10-19T11:00:30Z"
        .parse::<DateTime<Utc>>()
        .unwrap()
        .timestamp();
    let mut cur = Cursor::new(n - 10, chrono_tz::UTC);
    cur.arm(&c, chrono_tz::UTC, n - 10).unwrap();
    assert_eq!(cur.due, Some(n));
    cur.resume(&c, chrono_tz::UTC, n + 1).unwrap();
    assert!(cur.due.unwrap() > n + 1);
    assert!(cur.finished.contains("2026-10-19/11:00"));
}
#[test]
fn restart_preserves_future_pending() {
    let c = Config::default();
    let n = "2026-10-19T11:00:00Z"
        .parse::<DateTime<Utc>>()
        .unwrap()
        .timestamp();
    let mut cur = Cursor::new(n, chrono_tz::UTC);
    cur.arm(&c, chrono_tz::UTC, n).unwrap();
    let due = cur.due;
    cur.resume(&c, chrono_tz::UTC, n + 1).unwrap();
    assert_eq!(cur.due, due);
}
#[test]
fn reconfigure_never_catches_up() {
    let mut c = Config::default();
    let n = "2026-10-19T13:00:00Z"
        .parse::<DateTime<Utc>>()
        .unwrap()
        .timestamp();
    let mut cur = Cursor::new(n, chrono_tz::UTC);
    cur.arm(&c, chrono_tz::UTC, n).unwrap();
    c.anchors.push("12:30".into());
    cur.resume(&c, chrono_tz::UTC, n + 1).unwrap();
    assert_eq!(cur.anchor.as_deref(), Some("2026-10-19/16:00"));
}
#[test]
fn inconsistent_reset_history_blocks() {
    let mut s = state();
    observe(&mut s, 99990, q(99990, WINDOW, 0.));
    let mut f = q(100000, WINDOW, 0.);
    f.reset_at -= 100;
    f.remaining -= 100;
    assert_eq!(
        decision(&Config::default(), &s, &f, &q(100000, 604800, 1.)),
        Decision::Block("inconsistent_reset_history")
    );
}
#[test]
fn observation_history_is_bounded() {
    let mut s = state();
    for n in 0..100 {
        observe(&mut s, n, q(n, WINDOW, 0.));
    }
    assert_eq!(s.observations.len(), 64);
    assert!(s.validate().is_ok());
}
#[test]
fn observation_clock_reversal_blocks() {
    let mut s = state();
    observe(&mut s, 100001, q(100001, WINDOW, 0.));
    assert_eq!(
        decision(
            &Config::default(),
            &s,
            &q(100000, WINDOW, 0.),
            &q(100000, 604800, 1.)
        ),
        Decision::Block("state_or_clock")
    );
}
#[test]
fn buffer_cannot_cross_anchor_date() {
    let c = Config {
        anchors: vec!["23:59".into()],
        buffer_seconds: 300,
        ..Default::default()
    };
    let n = "2026-10-19T20:00:00Z".parse().unwrap();
    assert!(plan(&c, chrono_tz::UTC, n, 24).unwrap().is_empty());
}
#[test]
fn legacy_reservations_guard_all_request_paths() {
    let mut s = state();
    s.last_clock = 99999;
    s.legacy_attempts.push(LegacyAttempt {
        anchor: "old".into(),
        at: 99999,
        outcome: "unknown".into(),
        request_ok: None,
        before: None,
        after: None,
    });
    let c = Config::default();
    assert_eq!(
        decision(&c, &s, &q(100000, WINDOW, 0.), &q(100000, 604800, 1.)),
        Decision::Defer(118029)
    );
    let expired = Quota {
        duration: WINDOW,
        used: 0.,
        reset_at: 99900,
        remaining: 0,
    };
    assert_eq!(
        decision(&c, &s, &expired, &q(100000, 604800, 1.)),
        Decision::Defer(118029)
    );
}
#[test]
fn next_anchor_waits_for_actual_reset_and_completed_request() {
    let mut s = state();
    reserve(&mut s, "previous", 100000, q(100000, WINDOW, 0.));
    completed_at(&mut s, 100010);
    let f = Quota {
        duration: WINDOW,
        used: 2.,
        reset_at: 118015,
        remaining: 17015,
    };
    assert_eq!(
        decide(
            &Config::default(),
            &s,
            "next",
            101000,
            150000,
            &f,
            &q(101000, 604800, 1.),
            false
        ),
        Decision::Defer(118045)
    );
    assert_eq!(
        decide(
            &Config::default(),
            &s,
            "next",
            118045,
            150000,
            &q(118045, WINDOW, 0.),
            &q(118045, 604800, 1.),
            false
        ),
        Decision::Request("rolled_zero_usage")
    );
}
#[test]
fn editing_times_keeps_valid_pending_reset() {
    let mut c = Config::default();
    let start = "2026-10-19T11:00:30Z"
        .parse::<DateTime<Utc>>()
        .unwrap()
        .timestamp();
    let mut cur = Cursor::new(start - 1, chrono_tz::UTC);
    cur.arm(&c, chrono_tz::UTC, start - 1).unwrap();
    cur.due = Some(start + 3600);
    c.anchors.push("14:00".into());
    cur.replan(&c, chrono_tz::UTC, start + 10).unwrap();
    assert_eq!(cur.anchor.as_deref(), Some("2026-10-19/11:00"));
    assert_eq!(cur.due, Some(start + 3600));
    c.anchors.retain(|a| a != "11:00");
    cur.replan(&c, chrono_tz::UTC, start + 11).unwrap();
    assert_eq!(cur.anchor.as_deref(), Some("2026-10-19/14:00"));
}
#[test]
fn timezone_change_skips_elapsed_old_zone_times() {
    let c = Config::default();
    let now = "2026-10-19T01:00:00Z"
        .parse::<DateTime<Utc>>()
        .unwrap()
        .timestamp();
    let mut cur = Cursor::new(now, chrono_tz::Asia::Shanghai);
    cur.arm(&c, chrono_tz::Asia::Shanghai, now).unwrap();
    cur.replan(&c, chrono_tz::UTC, now).unwrap();
    assert!(cur.finished.contains("2026-10-19/06:00"));
    assert_eq!(cur.anchor.as_deref(), Some("2026-10-19/11:00"));
}
