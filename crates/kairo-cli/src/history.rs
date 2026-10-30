//! Read-only presentation of existing request and confirmation evidence. No new ledger.
use kairo_core::State;
use kairo_platform::{confirmation, Store};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Confirmed,
    Pending,
    Unconfirmed,
    Uncertain,
    Skipped,
    Blocked,
    Failed,
    Unknown,
    Never,
    Unavailable,
}
#[derive(Clone, Debug)]
pub struct Record {
    pub at: Option<i64>,
    pub confirmed_at: Option<i64>,
    pub kind: Kind,
    pub reason: String,
}
pub struct View {
    pub request: Option<Record>,
    pub latest: Record,
    pub records: Vec<Record>,
}
impl Record {
    fn new(at: Option<i64>, kind: Kind, reason: &str) -> Self {
        Self {
            at,
            confirmed_at: None,
            kind,
            reason: reason.into(),
        }
    }
    pub fn label(&self, en: bool) -> &'static str {
        match (self.kind, en) {
            (Kind::Confirmed, false) => "✓ 预热请求成功，五小时倒计时已确认（时间关联，归因不确定）",
            (Kind::Confirmed, true) => {
                "✓ Request succeeded; five-hour countdown confirmed (time-correlated; attribution uncertain)"
            }
            (Kind::Pending, false) => "○ 请求成功，等待倒计时确认",
            (Kind::Pending, true) => "○ Request succeeded; awaiting countdown confirmation",
            (Kind::Unconfirmed, false) => "○ 请求成功，倒计时未能确认",
            (Kind::Unconfirmed, true) => "○ Request succeeded; countdown not verified",
            (Kind::Uncertain, false) => "○ 请求成功，窗口计时已确认，关联不确定",
            (Kind::Uncertain, true) => {
                "○ Request succeeded; window countdown confirmed, attribution uncertain"
            }
            (Kind::Skipped, false) => "○ 额度窗口仍有效，本次无需预热",
            (Kind::Skipped, true) => "○ Window still active; no warmup needed",
            (Kind::Blocked, false) => "! 安全条件不满足，本次未预热",
            (Kind::Blocked, true) => "! Safety conditions unmet; no warmup performed",
            (Kind::Failed, false) => "✗ 预热请求失败",
            (Kind::Failed, true) => "✗ Warmup request failed",
            (Kind::Unknown, false) => "! 请求结果未知，不能确认成功",
            (Kind::Unknown, true) => "! Request result unknown; success cannot be verified",
            (Kind::Never, false) => "— 尚未进行过预热",
            (Kind::Never, true) => "— No warmup performed yet",
            (Kind::Unavailable, false) => "— 暂无可验证记录",
            (Kind::Unavailable, true) => "— No verifiable history available",
        }
    }
    pub fn explanation(&self, en: bool) -> &'static str {
        let (zh,english)=match self.reason.as_str() {
            "request_failed"=>("模型请求未成功；已有记录未保存更具体的失败原因。","The model request failed; no more specific cause was saved."),
            "attempt_guard"=>("为防止重复请求，已跳过本次执行。","Duplicate-request protection prevented execution."),
            "server_denied"=>("服务端拒绝请求，等待额度或授权恢复。","The server denied access; waiting for quota or authorization."),
            "quota_invalid_or_weekly_exhausted"=>("额度数据无效或每周额度耗尽。","Quota data is invalid or weekly allowance is exhausted."),
            "state_or_clock"|"inconsistent_reset_history"=>("状态、时钟或窗口信息无法安全验证。","State, clock or window information could not be safely verified."),
            "missed_not_replayed"|"cutoff"|"daily_cutoff"=>("本次计划已过期或到达截止时间，未补发请求。","The scheduled check expired or reached its cutoff; no catch-up request was sent."),
            "deferred"=>("仍在等待已有保护期限或安全窗口，未发送请求。","Waiting for an existing protection deadline or safe window; no request sent."),
            "eligible_observation_only"=>("当前处于只读观察，未发送模型请求。","Read-only observation; no model request sent."),
            "timeout"|"check_limit_reached"=>("请求成功，但既有只读确认未取得倒计时证据。","The request succeeded, but read-only checks did not verify its countdown."),
            "confirmation_unreadable"=>("请求记录存在，但确认记录无法验证；不能宣称预热成功。","A request exists, but confirmation records cannot be verified; warmup success is unproven."),
            "attribution_uncertain"|"countdown_confirmed_time_correlated"=>("倒计时证据不代表排他因果证明，其他客户端也可能影响窗口。","Countdown evidence is not exclusive proof of causation; other clients may affect the window."),
            _=>("仅展示已保存的可信证据；缺失的旧记录不会补造。","Only saved verifiable evidence is shown; missing historical records are never invented."),
        };
        if en {
            english
        } else {
            zh
        }
    }
    pub fn color(&self) -> u8 {
        match self.kind {
            Kind::Confirmed => 32,
            Kind::Failed => 31,
            Kind::Never | Kind::Unavailable => 90,
            _ => 33,
        }
    }
}
pub fn load(store: &Store, expected: Option<&str>) -> View {
    load_at(store, expected, chrono::Utc::now().timestamp())
}
fn load_at(store: &Store, expected: Option<&str>, now: i64) -> View {
    let state = store
        .state()
        .ok()
        .filter(|s| expected.is_none_or(|a| s.account == a));
    let Some(state) = state else {
        return empty(Kind::Unavailable);
    };
    let ledger = confirmation::read(store, &state);
    let mut records = Vec::new();
    for a in &state.attempts {
        let mut r = Record::new(
            Some(a.completed_at.unwrap_or(a.at)),
            match a.outcome.as_str() {
                "request_failed" => Kind::Failed,
                "attempt_persisted_unknown" => Kind::Unknown,
                _ => Kind::Pending,
            },
            &a.outcome,
        );
        if matches!(
            a.outcome.as_str(),
            "request_ok_window_unconfirmed" | "five_hour_window_confirmed"
        ) {
            match &ledger {
                Ok(l) => {
                    if let Some(t) = l
                        .tasks
                        .iter()
                        .find(|t| t.anchor == a.anchor && t.request_at == a.at)
                    {
                        r.kind = match t.status.as_str() {
                            "countdown_confirmed_time_correlated" => Kind::Confirmed,
                            "attribution_uncertain" => Kind::Uncertain,
                            "timeout" | "check_limit_reached" => Kind::Unconfirmed,
                            _ => Kind::Pending,
                        };
                        r.reason = t.status.clone();
                        if r.kind == Kind::Pending && now >= t.deadline {
                            r.kind = Kind::Unconfirmed;
                            r.reason = "confirmation_deadline_passed".into();
                        }
                        if t.countdown_confirmed {
                            r.confirmed_at = t.samples.last().map(|s| s.checked_at);
                        }
                    }
                }
                Err(_) => {
                    r.kind = Kind::Unconfirmed;
                    r.reason = "confirmation_unreadable".into();
                }
            }
        }
        if r.kind == Kind::Pending && a.completed_at.is_none() {
            r.kind = Kind::Unconfirmed;
            r.reason = "legacy_confirmation_missing".into();
        } else if r.kind == Kind::Pending
            && a.completed_at
                .is_some_and(|at| now >= at.saturating_add(kairo_core::WINDOW))
        {
            r.kind = Kind::Unconfirmed;
            r.reason = "confirmation_deadline_passed".into();
        }
        records.push(r);
    }
    records.sort_by_key(|r| r.at);
    let request = records.last().cloned();
    if let Ok(v) = store.read::<Value>("natural-acceptance.json") {
        if let Some(c) = v
            .get("latest_natural_check")
            .or_else(|| v.get("first_natural_check"))
        {
            add_check(&mut records, c, &state);
        }
    }
    if let Ok(v) = store.read::<Value>("status.json") {
        add_check(&mut records, &v, &state);
    }
    records.sort_by_key(|r| r.at);
    records.dedup_by(|a, b| a.at == b.at && a.kind == b.kind && a.reason == b.reason);
    let latest = records.last().cloned().unwrap_or_else(|| {
        let declared_fresh = store
            .read::<Value>("account-origin.json")
            .ok()
            .is_some_and(|v| {
                v["origin"] == "administrator_declared_fresh" && v["account"] == state.account
            });
        let unknown = store.root.join("recovery.json").exists()
            || !state.legacy_attempts.is_empty()
            || (state.last_clock > 0 && !declared_fresh);
        Record::new(
            None,
            if unknown {
                Kind::Unavailable
            } else {
                Kind::Never
            },
            "",
        )
    });
    View {
        request,
        latest,
        records,
    }
}
fn empty(kind: Kind) -> View {
    View {
        request: None,
        latest: Record::new(None, kind, ""),
        records: Vec::new(),
    }
}
fn add_check(records: &mut Vec<Record>, v: &Value, state: &State) {
    let Some(at) = v["at"]
        .as_i64()
        .filter(|at| *at >= 0 && *at <= state.last_clock.max(chrono::Utc::now().timestamp()))
    else {
        return;
    };
    let Some(event) = v["event"].as_str() else {
        return;
    };
    let kind = match event {
        "deferred"
            if state
                .observations
                .iter()
                .any(|o| o.at == at && o.quota.used > 0. && o.quota.reset_at > at) =>
        {
            Kind::Skipped
        }
        "deferred"
        | "attempt_guard"
        | "server_denied"
        | "state_or_clock"
        | "inconsistent_reset_history"
        | "quota_invalid_or_weekly_exhausted"
        | "missed_not_replayed"
        | "cutoff"
        | "daily_cutoff"
        | "eligible_observation_only"
        | "blocked" => Kind::Blocked,
        _ => return,
    };
    records.push(Record::new(Some(at), kind, event));
}
pub fn natural(at: i64, now: i64, tz: chrono_tz::Tz, en: bool) -> String {
    use chrono::TimeZone;
    let Some(date) = tz.timestamp_opt(at, 0).single() else {
        return "—".into();
    };
    let Some(today) = tz.timestamp_opt(now, 0).single() else {
        return "—".into();
    };
    let days = today
        .date_naive()
        .signed_duration_since(date.date_naive())
        .num_days();
    let prefix = match (days, en) {
        (0, false) => "今天".into(),
        (0, true) => "Today".into(),
        (1, false) => "昨天".into(),
        (1, true) => "Yesterday".into(),
        (-1, false) => "明天".into(),
        (-1, true) => "Tomorrow".into(),
        (_, false) => date.format("%-m月%-d日").to_string(),
        (_, true) => date.format("%b %-d").to_string(),
    };
    if date.year() != today.year() {
        format!(
            "{} {}",
            date.format("%Y"),
            date.format(if en {
                "%b %-d %H:%M"
            } else {
                "%-m月%-d日 %H:%M"
            })
        )
    } else {
        format!("{prefix} {}", date.format("%H:%M"))
    }
}
use chrono::Datelike;

#[cfg(test)]
mod tests {
    use super::*;
    use confirmation::{Ledger, Sample, Task};
    use kairo_core::{Attempt, Quota};
    struct Fixture {
        root: std::path::PathBuf,
        state: State,
        done: i64,
    }
    impl Fixture {
        fn new() -> Self {
            static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "kairo-view-{}-{}",
                std::process::id(),
                N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            let done = 1793398092; // Synthetic: 2026-10-31 06:08:12 Asia/Shanghai.
            let state = State {
                schema: 1,
                account: "synthetic-view-account".into(),
                last_clock: done + 152,
                attempts: vec![Attempt {
                    anchor: "synthetic-06:00".into(),
                    at: done - 2,
                    completed_at: Some(done),
                    before: Quota {
                        duration: 18000,
                        used: 0.,
                        reset_at: done - 2 + 18000,
                        remaining: 18000,
                    },
                    after: None,
                    outcome: "request_ok_window_unconfirmed".into(),
                }],
                ..Default::default()
            };
            let f = Self { root, state, done };
            f.save();
            f
        }
        fn save(&self) {
            Store::open(&self.root)
                .unwrap()
                .write("state.json", &self.state)
                .unwrap();
        }
        fn confirmed(&self) {
            let samples = [60, 152]
                .into_iter()
                .map(|delta| Sample {
                    checked_at: self.done + delta,
                    reset_at: self.done + 18000,
                    remaining: 18000 - delta,
                    used_percent: 0.,
                })
                .collect();
            Store::open(&self.root)
                .unwrap()
                .write(
                    "confirmations.json",
                    &Ledger {
                        schema: 1,
                        account: self.state.account.clone(),
                        tasks: vec![Task {
                            anchor: self.state.attempts[0].anchor.clone(),
                            request_at: self.done - 2,
                            completed_at: self.done,
                            status: "countdown_confirmed_time_correlated".into(),
                            request_success: true,
                            countdown_confirmed: true,
                            time_correlated: true,
                            attribution_uncertain: true,
                            next_check: None,
                            deadline: self.done + 18000,
                            checks: 2,
                            samples,
                            events: vec![],
                        }],
                    },
                )
                .unwrap();
        }
        fn view(&self) -> View {
            load_at(
                &Store::open(&self.root).unwrap(),
                Some(&self.state.account),
                self.done + 152,
            )
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.root).unwrap();
        }
    }
    #[test]
    fn expired_or_incomplete_legacy_success_is_not_permanently_pending() {
        let mut f = Fixture::new();
        assert_eq!(
            load_at(
                &Store::open(&f.root).unwrap(),
                Some(&f.state.account),
                f.done + 18001
            )
            .request
            .unwrap()
            .kind,
            Kind::Unconfirmed
        );
        f.state.attempts[0].completed_at = None;
        f.save();
        assert_eq!(f.view().request.unwrap().kind, Kind::Unconfirmed);
    }
    #[test]
    fn timeout_and_unrelated_countdown_are_not_claimed_as_correlated_success() {
        let f = Fixture::new();
        f.confirmed();
        {
            let store = Store::open(&f.root).unwrap();
            let mut l: Ledger = store.read("confirmations.json").unwrap();
            let t = &mut l.tasks[0];
            t.status = "attribution_uncertain".into();
            t.time_correlated = false;
            for sample in &mut t.samples {
                sample.reset_at -= 600;
                sample.remaining -= 600;
            }
            store.write("confirmations.json", &l).unwrap();
        }
        assert_eq!(f.view().request.unwrap().kind, Kind::Uncertain);
        {
            let store = Store::open(&f.root).unwrap();
            let mut l: Ledger = store.read("confirmations.json").unwrap();
            let t = &mut l.tasks[0];
            t.status = "timeout".into();
            t.countdown_confirmed = false;
            t.time_correlated = false;
            t.samples.clear();
            store.write("confirmations.json", &l).unwrap();
        }
        assert_eq!(f.view().request.unwrap().kind, Kind::Unconfirmed);
    }
    #[test]
    fn unknown_attempt_and_missing_pre_recovery_history_stay_unproven() {
        let mut f = Fixture::new();
        f.state.attempts[0].outcome = "attempt_persisted_unknown".into();
        f.state.attempts[0].completed_at = None;
        f.save();
        assert_eq!(f.view().request.unwrap().kind, Kind::Unknown);
        f.state.attempts.clear();
        f.save();
        Store::open(&f.root)
            .unwrap()
            .write(
                "recovery.json",
                &serde_json::json!({"history_before_start":"unknown"}),
            )
            .unwrap();
        assert_eq!(f.view().latest.kind, Kind::Unavailable);
    }
    #[test]
    fn confirmed_evidence_survives_reopen_and_keeps_attribution_caveat() {
        let f = Fixture::new();
        f.confirmed();
        for _ in 0..2 {
            let r = f.view().request.unwrap();
            assert_eq!(r.kind, Kind::Confirmed);
            assert_eq!(r.at, Some(f.done));
            assert_eq!(r.confirmed_at, Some(f.done + 152));
            assert!(r.label(false).contains("时间关联"));
            assert!(r.explanation(true).contains("not exclusive"));
        }
    }
    #[test]
    fn request_success_and_full_quota_never_invent_countdown_confirmation() {
        let f = Fixture::new();
        assert_eq!(f.view().request.unwrap().kind, Kind::Pending);
        let store = Store::open(&f.root).unwrap();
        store
            .write(
                "quota-cache.json",
                &serde_json::json!({"five_hour":{"used":0},"weekly":{"used":0}}),
            )
            .unwrap();
        drop(store);
        assert_eq!(f.view().request.unwrap().kind, Kind::Pending);
    }
    #[test]
    fn later_failure_cannot_inherit_an_older_successful_confirmation() {
        let mut f = Fixture::new();
        f.confirmed();
        let mut a = f.state.attempts[0].clone();
        a.anchor = "later-failure".into();
        a.outcome = "request_failed".into();
        a.at = f.done + 200;
        a.completed_at = Some(f.done + 202);
        a.before.reset_at = a.at + 18000;
        f.state.last_clock = f.done + 202;
        f.state.attempts.push(a);
        f.state.attempts.reverse();
        f.save();
        let view = f.view();
        assert_eq!(view.request.unwrap().kind, Kind::Failed);
        assert_eq!(view.records[0].kind, Kind::Confirmed);
    }
    #[test]
    fn skipped_window_requires_saved_positive_quota_at_the_same_check() {
        let mut f = Fixture::new();
        f.confirmed();
        let at = f.done + 200;
        f.state.last_clock = at;
        kairo_core::observe(
            &mut f.state,
            at,
            Quota {
                duration: 18000,
                used: 2.,
                reset_at: at + 18000,
                remaining: 18000,
            },
        );
        f.save();
        Store::open(&f.root)
            .unwrap()
            .write(
                "natural-acceptance.json",
                &serde_json::json!({"latest_natural_check":{"at":at,"event":"deferred"}}),
            )
            .unwrap();
        assert_eq!(f.view().latest.kind, Kind::Skipped);
        assert_eq!(f.view().request.unwrap().kind, Kind::Confirmed);
        f.state.observations.clear();
        f.save();
        assert_eq!(f.view().latest.kind, Kind::Blocked);
    }
    #[test]
    fn never_unknown_missing_and_foreign_records_are_distinct() {
        let mut f = Fixture::new();
        f.state.attempts.clear();
        f.state.last_clock = 0;
        f.save();
        assert_eq!(f.view().latest.kind, Kind::Never);
        assert_eq!(
            load(&Store::open(&f.root).unwrap(), Some("another-account"))
                .latest
                .kind,
            Kind::Unavailable
        );
        std::fs::remove_file(f.root.join("state.json")).unwrap();
        assert_eq!(f.view().latest.kind, Kind::Unavailable);
    }
    #[test]
    fn broken_confirmation_is_not_reported_as_a_model_failure_or_success() {
        let f = Fixture::new();
        Store::open(&f.root)
            .unwrap()
            .write("confirmations.json", &serde_json::json!({"schema":99}))
            .unwrap();
        let r = f.view().request.unwrap();
        assert_eq!(r.kind, Kind::Unconfirmed);
        assert_eq!(r.reason, "confirmation_unreadable");
    }
    #[test]
    fn human_dates_use_local_calendar_in_both_languages() {
        use chrono::TimeZone;
        let tz = chrono_tz::Asia::Shanghai;
        let now = tz
            .with_ymd_and_hms(2026, 10, 31, 11, 0, 0)
            .unwrap()
            .timestamp();
        assert_eq!(natural(now, now, tz, false), "今天 11:00");
        assert_eq!(natural(now - 86400, now, tz, true), "Yesterday 11:00");
        assert_eq!(natural(now - 2 * 86400, now, tz, false), "10月29日 11:00");
        assert_eq!(natural(now + 86400, now, tz, true), "Tomorrow 11:00");
    }
}
