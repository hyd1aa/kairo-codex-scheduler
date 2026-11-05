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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cc_custom_times_and_thirty_second_delay() {
        let c = Config {
            anchors: ["06:00", "11:02", "16:04", "21:06"]
                .map(String::from)
                .to_vec(),
            ..Default::default()
        };
        let tz: Tz = "Asia/Shanghai".parse().unwrap();
        let now = DateTime::parse_from_rfc3339("2026-11-05T00:00:00+08:00")
            .unwrap()
            .with_timezone(&Utc);
        let times = plan(&c, tz, now, 24)
            .unwrap()
            .into_iter()
            .map(|d| d.with_timezone(&tz).format("%H:%M:%S").to_string())
            .collect::<Vec<_>>();
        assert_eq!(times, ["06:00:30", "11:02:30", "16:04:30", "21:06:30"]);
    }
    #[test]
    fn dst_ambiguous_slot_is_once_and_gap_is_not_invented() {
        let tz: Tz = "America/New_York".parse().unwrap();
        let mut c = Config {
            anchors: vec!["01:30".into()],
            buffer_seconds: 0,
            ..Default::default()
        };
        let fall = DateTime::parse_from_rfc3339("2026-11-01T00:00:00-04:00")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(plan(&c, tz, fall, 24).unwrap().len(), 1);
        c.anchors = vec!["02:30".into()];
        let spring = DateTime::parse_from_rfc3339("2026-03-08T00:00:00-05:00")
            .unwrap()
            .with_timezone(&Utc);
        assert!(plan(&c, tz, spring, 20).unwrap().is_empty());
    }
    #[test]
    fn invalid_and_duplicate_times_do_not_validate() {
        for anchors in [
            vec!["6:00".into()],
            vec!["06:00".into(), "06:00".into()],
            vec!["23:59".into(), "00:01".into()],
        ] {
            assert!(Config {
                anchors,
                ..Default::default()
            }
            .validate()
            .is_err());
        }
    }
}
