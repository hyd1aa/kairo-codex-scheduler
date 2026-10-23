use anyhow::{bail, Result};
use chrono::{DateTime, Duration, LocalResult, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub const WINDOW: i64 = 18000;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema: u32,
    pub language: String,
    pub anchors: Vec<String>,
    pub idle_window_prewarm: bool,
    pub buffer_seconds: i64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            schema: 1,
            language: "zh-CN".into(),
            anchors: ["06:00", "11:00", "16:00", "21:00"]
                .map(String::from)
                .to_vec(),
            idle_window_prewarm: true,
            buffer_seconds: 30,
        }
    }
}
pub fn time(s: &str) -> Result<NaiveTime> {
    if s.len() != 5
        || s.as_bytes()[2] != b':'
        || !s
            .bytes()
            .enumerate()
            .all(|(i, b)| i == 2 || b.is_ascii_digit())
    {
        bail!("HH:MM required / 必须使用 HH:MM")
    }
    Ok(NaiveTime::parse_from_str(s, "%H:%M")?)
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        if self.schema != 1
            || !["zh-CN", "en-US"].contains(&self.language.as_str())
            || !(0..=300).contains(&self.buffer_seconds)
            || self.anchors.len() > 48
        {
            bail!("Invalid configuration / 配置无效")
        }
        let mut v = BTreeSet::new();
        for a in &self.anchors {
            time(a)?;
            if !v.insert(a) {
                bail!("Duplicate time / 时间重复")
            }
        }
        let mins: Vec<i64> = v
            .iter()
            .map(|s| {
                use chrono::Timelike;
                let t = time(s).unwrap();
                (t.hour() * 60 + t.minute()) as i64
            })
            .collect();
        if mins.len() > 1 {
            for i in 0..mins.len() {
                if (mins[(i + 1) % mins.len()] - mins[i]).rem_euclid(1440) < 10 {
                    bail!("Minimum spacing 10 minutes / 最小间隔十分钟")
                }
            }
        }
        Ok(())
    }
}
/// Future instants only. Ambiguous wall times choose the first occurrence; gaps are skipped.
pub fn plan(c: &Config, tz: Tz, now: DateTime<Utc>, hours: i64) -> Result<Vec<DateTime<Utc>>> {
    c.validate()?;
    let mut out = BTreeSet::new();
    let date = now.with_timezone(&tz).date_naive();
    for d in 0..=3 {
        for a in &c.anchors {
            let local = (date + Duration::days(d)).and_time(time(a)?);
            let candidate = match tz.from_local_datetime(&local) {
                LocalResult::Single(t) => Some(t),
                LocalResult::Ambiguous(a, b) => Some(a.min(b)),
                LocalResult::None => None,
            };
            if let Some(t) = candidate {
                let t = t.with_timezone(&Utc) + Duration::seconds(c.buffer_seconds);
                if t.with_timezone(&tz).date_naive() != local.date() {
                    continue;
                }
                if t > now && t <= now + Duration::hours(hours) {
                    out.insert(t);
                }
            }
        }
    }
    Ok(out.into_iter().collect())
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Quota {
    pub duration: i64,
    pub used: f64,
    pub reset_at: i64,
    pub remaining: i64,
}
impl Quota {
    pub fn valid(&self, now: i64, duration: i64) -> bool {
        self.duration == duration
            && duration > 0
            && now >= 0
            && self.reset_at > 0
            && self.used.is_finite()
            && (0.0..=100.0).contains(&self.used)
            && (0..=duration).contains(&self.remaining)
            && ((self.reset_at.saturating_sub(now)).max(0) - self.remaining).abs() <= 5
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub schema: u32,
    pub account: String,
    pub last_clock: i64,
    pub attempts: Vec<Attempt>,
    pub positive_until: i64,
    pub pending: Option<i64>,
    #[serde(default)]
    pub observations: Vec<Observation>,
    #[serde(default)]
    pub weekly: Option<Quota>,
    #[serde(default)]
    pub server_denied: bool,
    #[serde(default)]
    pub legacy_attempts: Vec<LegacyAttempt>,
}
impl State {
    pub fn validate(&self) -> Result<()> {
        if self.schema != 1
            || self.account.is_empty()
            || self.last_clock < 0
            || self.positive_until < 0
            || self.pending.is_some_and(|t| t < 0)
        {
            bail!("Invalid state / 状态无效")
        }
        if self.observations.len() > 64
            || self.observations.windows(2).any(|w| w[1].at < w[0].at)
            || self
                .observations
                .iter()
                .any(|o| o.at < 0 || !o.quota.valid(o.at, WINDOW))
        {
            bail!("Invalid window observations / 窗口观测记录无效")
        }
        let mut legacy_seen = BTreeSet::new();
        if self.legacy_attempts.iter().any(|a| {
            a.anchor.is_empty()
                || a.at < 0
                || a.at > self.last_clock
                || a.outcome.is_empty()
                || !legacy_seen.insert(&a.anchor)
        }) {
            bail!("legacy_attempts_invalid / 迁移调用历史无效")
        }
        let mut seen = BTreeSet::new();
        for a in &self.attempts {
            if a.at < 0
                || a.at > self.last_clock
                || a.completed_at
                    .is_some_and(|at| at < a.at || at > self.last_clock)
                || a.anchor.is_empty()
                || !seen.insert(&a.anchor)
                || !a.before.valid(a.at, WINDOW)
                || ![
                    "attempt_persisted_unknown",
                    "request_failed",
                    "five_hour_window_confirmed",
                    "request_ok_window_unconfirmed",
                ]
                .contains(&a.outcome.as_str())
            {
                bail!("Invalid attempt history / 尝试历史无效")
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attempt {
    pub anchor: String,
    pub at: i64,
    #[serde(default)]
    pub completed_at: Option<i64>,
    pub before: Quota,
    pub after: Option<Quota>,
    pub outcome: String,
}
#[derive(Debug, PartialEq)]
pub enum Decision {
    Block(&'static str),
    Defer(i64),
    Request(&'static str),
}
#[allow(clippy::too_many_arguments)]
pub fn decide(
    c: &Config,
    s: &State,
    anchor: &str,
    now: i64,
    cutoff: i64,
    five: &Quota,
    week: &Quota,
    denied: bool,
) -> Decision {
    if denied {
        return Decision::Block("server_denied");
    }
    if c.validate().is_err()
        || s.validate().is_err()
        || now < s.last_clock
        || s.observations.last().is_some_and(|o| now < o.at)
    {
        return Decision::Block("state_or_clock");
    }
    if now >= cutoff {
        return Decision::Block("daily_cutoff");
    }
    if !five.valid(now, WINDOW) || !week.valid(now, 604800) || week.used >= 100.0 {
        return Decision::Block("quota_invalid_or_weekly_exhausted");
    }
    if s.legacy_attempts.iter().any(|a| a.anchor == anchor)
        || s.attempts.iter().any(|a| a.anchor == anchor)
    {
        return Decision::Block("attempt_guard");
    }
    let recent_until = s
        .attempts
        .iter()
        .map(|a| {
            a.completed_at
                .unwrap_or(a.at)
                .saturating_add(WINDOW + c.buffer_seconds)
        })
        .chain(
            s.legacy_attempts
                .iter()
                .filter(|a| a.at > 0)
                .map(|a| a.at.saturating_add(WINDOW + c.buffer_seconds)),
        )
        .max()
        .unwrap_or(0);
    if let Some(previous) = s.observations.last() {
        if previous.quota.reset_at > now
            && five.reset_at < previous.quota.reset_at.saturating_sub(5)
        {
            return Decision::Block("inconsistent_reset_history");
        }
    }
    let historical = s
        .observations
        .iter()
        .filter(|o| o.quota.used > 0.0)
        .map(|o| o.quota.reset_at.saturating_add(c.buffer_seconds))
        .max()
        .unwrap_or(0);
    let active_until = if five.used > 0.0 {
        five.reset_at.saturating_add(c.buffer_seconds)
    } else {
        0
    };
    let until = recent_until
        .max(historical)
        .max(s.positive_until.saturating_add(c.buffer_seconds))
        .max(active_until);
    if until > now {
        return Decision::Defer(until);
    }
    if now >= five.reset_at.saturating_add(c.buffer_seconds) {
        return Decision::Request("expired");
    }
    if five.used > 0.0 {
        return Decision::Defer(five.reset_at.saturating_add(c.buffer_seconds));
    }
    if c.idle_window_prewarm && five.remaining >= WINDOW - 60 {
        return Decision::Request("rolled_zero_usage");
    }
    if now >= five.reset_at.saturating_add(c.buffer_seconds) {
        return Decision::Request("expired");
    }
    Decision::Defer(five.reset_at.saturating_add(c.buffer_seconds))
}
pub fn reserve(s: &mut State, anchor: &str, now: i64, five: Quota) {
    s.last_clock = now;
    s.pending = None;
    s.attempts.push(Attempt {
        anchor: anchor.into(),
        at: now,
        completed_at: None,
        before: five,
        after: None,
        outcome: "attempt_persisted_unknown".into(),
    });
}
pub fn complete(s: &mut State, ok: bool, after: Option<Quota>) {
    if let Some(a) = s.attempts.last_mut() {
        a.outcome = if !ok {
            "request_failed"
        } else if after
            .as_ref()
            .is_some_and(|q| q.used > 0.0 && q.reset_at > a.before.reset_at)
        {
            "five_hour_window_confirmed"
        } else {
            "request_ok_window_unconfirmed"
        }
        .into();
        a.after = after;
    }
    s.pending = None;
}
#[cfg(test)]
mod tests {
    use super::*;
    fn state() -> State {
        State {
            schema: 1,
            account: "synthetic".into(),
            ..Default::default()
        }
    }
    fn q(now: i64, duration: i64, used: f64) -> Quota {
        Quota {
            duration,
            used,
            reset_at: now + duration,
            remaining: duration,
        }
    }
    #[test]
    fn synthetic_window_confirmation_case() {
        let c = Config::default();
        let mut s = state();
        let n = 1700000000;
        let f = q(n, WINDOW, 0.);
        let w = q(n, 604800, 37.);
        let mut active = f.clone();
        active.used = 2.;
        assert!(matches!(
            decide(
                &c,
                &s,
                "2023-11-14/22:00",
                n - 8214,
                n + 36000,
                &active,
                &w,
                false
            ),
            Decision::Block(_) | Decision::Defer(_)
        ));
        assert_eq!(
            decide(&c, &s, "2023-11-14/22:00", n, n + 36000, &f, &w, false),
            Decision::Request("rolled_zero_usage")
        );
        reserve(&mut s, "2023-11-14/22:00", n, f.clone());
        complete(&mut s, true, Some(f.clone()));
        assert_eq!(s.attempts[0].outcome, "request_ok_window_unconfirmed");
        assert!(s.pending.is_none());
        assert_eq!(
            decide(
                &c,
                &s,
                "2023-11-14/22:00",
                n + 5,
                n + 36000,
                &q(n + 5, WINDOW, 0.),
                &q(n + 5, 604800, 37.),
                false
            ),
            Decision::Block("attempt_guard")
        );
    }
    #[test]
    fn schedule_validation() {
        let mut c = Config::default();
        c.anchors.push("11:00".into());
        assert!(c.validate().is_err());
        c.anchors = vec!["23:59".into(), "00:01".into()];
        assert!(c.validate().is_err());
        assert!(time("2:00").is_err());
    }
    #[test]
    fn dst() {
        let c = Config {
            anchors: vec!["02:30".into()],
            ..Default::default()
        };
        let n = "2026-03-08T00:00:00Z".parse().unwrap();
        assert!(plan(&c, chrono_tz::America::New_York, n, 24)
            .unwrap()
            .is_empty());
    }
    #[test]
    fn weekly_and_server_priority() {
        let n = 100;
        assert_eq!(
            decide(
                &Config::default(),
                &state(),
                "a",
                n,
                1000,
                &q(n, WINDOW, 0.),
                &q(n, 604800, 100.),
                true
            ),
            Decision::Block("server_denied")
        );
    }
    #[test]
    fn duration_strict() {
        assert!(!q(100, 17999, 0.).valid(100, WINDOW));
    }
    #[test]
    fn cross_anchor_guard() {
        let mut s = state();
        reserve(&mut s, "a", 100, q(100, WINDOW, 0.));
        assert_eq!(
            decide(
                &Config::default(),
                &s,
                "b",
                200,
                50000,
                &q(200, WINDOW, 0.),
                &q(200, 604800, 0.),
                false
            ),
            Decision::Defer(18130)
        );
    }
}

/// An anchor is identified by local date and wall time, independent of timezone.
/// Durable cursor forbids catch-up following restart, timezone change or clock reversal.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cursor {
    pub schema: u32,
    pub timezone: String,
    pub observed: i64,
    pub due: Option<i64>,
    pub anchor: Option<String>,
    pub cutoff: i64,
    pub finished: BTreeSet<String>,
    #[serde(default)]
    pub config_signature: String,
}
impl Cursor {
    pub fn new(now: i64, tz: Tz) -> Self {
        Self {
            schema: 1,
            timezone: tz.to_string(),
            observed: now,
            due: None,
            anchor: None,
            cutoff: 0,
            finished: BTreeSet::new(),
            config_signature: String::new(),
        }
    }
    pub fn arm(&mut self, c: &Config, tz: Tz, now: i64) -> Result<()> {
        let instant = DateTime::from_timestamp(now, 0)
            .ok_or_else(|| anyhow::anyhow!("Invalid clock / 时钟无效"))?;
        self.config_signature = config_signature(c);
        self.timezone = tz.to_string();
        self.observed = now;
        self.due = None;
        self.anchor = None;
        for due in plan(c, tz, instant, 48)? {
            let local = (due - Duration::seconds(c.buffer_seconds)).with_timezone(&tz);
            let key = local.format("%Y-%m-%d/%H:%M").to_string();
            if self.finished.contains(&key) {
                continue;
            }
            let end = local
                .date_naive()
                .succ_opt()
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap();
            let cutoff = tz
                .from_local_datetime(&end)
                .earliest()
                .ok_or_else(|| anyhow::anyhow!("Ambiguous cutoff / 截止时间无效"))?
                .timestamp();
            self.due = Some(due.timestamp());
            self.anchor = Some(key);
            self.cutoff = cutoff;
            break;
        }
        Ok(())
    }
    pub fn finish(&mut self) {
        if let Some(key) = self.anchor.take() {
            self.finished.insert(key);
        }
        self.due = None;
    }
}

#[cfg(test)]
mod safety_tests;

pub fn config_signature(c: &Config) -> String {
    let mut anchors = c.anchors.clone();
    anchors.sort();
    format!("{}:{}", anchors.join(","), c.buffer_seconds)
}
impl Cursor {
    pub fn resume(&mut self, c: &Config, tz: Tz, now: i64) -> Result<()> {
        if self.schema != 1 || now < self.observed {
            bail!("Invalid cursor or backward clock / 游标无效或时钟回拨")
        }
        if self.due.is_some_and(|due| due <= now) {
            self.finish();
            return self.replan(c, tz, now);
        }
        if self.timezone != tz.to_string()
            || self.config_signature != config_signature(c)
            || self.due.is_none_or(|due| due <= now || due >= self.cutoff)
        {
            self.replan(c, tz, now)?;
        } else {
            self.observed = now;
        }
        Ok(())
    }
}

impl Cursor {
    pub fn replan(&mut self, c: &Config, tz: Tz, now: i64) -> Result<()> {
        c.validate()?;
        let old_tz = self.timezone.parse::<Tz>().ok();
        let old_buffer = self
            .config_signature
            .rsplit_once(':')
            .and_then(|(_, s)| s.parse::<i64>().ok())
            .filter(|b| (0..=300).contains(b));
        let old_local = self
            .anchor
            .as_deref()
            .and_then(|s| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d/%H:%M").ok());
        let old_check = old_local
            .zip(old_tz)
            .and_then(|(local, tz)| tz.from_local_datetime(&local).earliest())
            .map(|d| {
                d.timestamp()
                    .saturating_add(old_buffer.unwrap_or(c.buffer_seconds))
            });
        if old_tz == Some(tz)
            && old_check.is_some_and(|at| at <= now)
            && old_local.is_some_and(|local| c.anchors.contains(&local.format("%H:%M").to_string()))
        {
            if let (Some(due), Some(check), Some(buffer)) = (self.due, old_check, old_buffer) {
                let updated = due.saturating_add(c.buffer_seconds - buffer);
                if due > check && updated > now && updated < self.cutoff {
                    self.due = Some(updated);
                    self.config_signature = config_signature(c);
                    self.observed = now;
                    return Ok(());
                }
            }
        }
        if old_tz.is_some_and(|old| old != tz) {
            let old = old_tz.unwrap();
            let instant = DateTime::from_timestamp(now, 0)
                .ok_or_else(|| anyhow::anyhow!("clock_invalid / 时间戳无效"))?;
            let date = instant.with_timezone(&old).date_naive();
            let anchors = self
                .config_signature
                .rsplit_once(':')
                .map(|(a, _)| a.split(',').map(str::to_string).collect::<Vec<_>>())
                .unwrap_or_else(|| c.anchors.clone());
            for days in 0..=2 {
                for anchor in &anchors {
                    if let Ok(time) = time(anchor) {
                        let local = (date - Duration::days(days)).and_time(time);
                        if old
                            .from_local_datetime(&local)
                            .earliest()
                            .is_some_and(|at| {
                                at.timestamp()
                                    .saturating_add(old_buffer.unwrap_or(c.buffer_seconds))
                                    <= now
                            })
                        {
                            self.finished
                                .insert(local.format("%Y-%m-%d/%H:%M").to_string());
                        }
                    }
                }
            }
        }
        if self.due.is_some_and(|due| due <= now) || old_check.is_some_and(|at| at <= now) {
            self.finish();
        } else {
            self.due = None;
            self.anchor = None;
        }
        self.arm(c, tz, now)
    }
    pub fn cover_due_anchors(&mut self, c: &Config, tz: Tz, now: i64) -> Result<bool> {
        if now <= self.observed || now - self.observed > 60 {
            return Ok(false);
        }
        let from = DateTime::from_timestamp(self.observed, 0)
            .ok_or_else(|| anyhow::anyhow!("clock_invalid / 时间戳无效"))?;
        let mut covered = false;
        for at in plan(c, tz, from, 48)? {
            if at.timestamp() > now {
                break;
            }
            let key = (at - Duration::seconds(c.buffer_seconds))
                .with_timezone(&tz)
                .format("%Y-%m-%d/%H:%M")
                .to_string();
            if self.anchor.as_deref() != Some(&key) && self.finished.insert(key) {
                covered = true
            }
        }
        Ok(covered)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub at: i64,
    pub quota: Quota,
    pub phase: String,
}
pub fn observe(s: &mut State, now: i64, quota: Quota) {
    let phase = if quota.used > 0.0 {
        "active"
    } else if quota.reset_at <= now {
        "expired"
    } else if s
        .observations
        .last()
        .is_some_and(|o| o.quota.reset_at < quota.reset_at)
    {
        "rolled_zero_usage"
    } else {
        "zero_usage_uncertain"
    };
    s.observations.push(Observation {
        at: now,
        quota,
        phase: phase.into(),
    });
    if s.observations.len() > 64 {
        s.observations.remove(0);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyAttempt {
    pub anchor: String,
    pub at: i64,
    pub outcome: String,
    pub request_ok: Option<bool>,
    pub before: Option<Quota>,
    pub after: Option<Quota>,
}

pub fn completed_at(s: &mut State, now: i64) {
    if let Some(a) = s.attempts.last_mut() {
        if now >= a.at {
            a.completed_at = Some(now);
            s.last_clock = s.last_clock.max(now);
        }
    }
}
