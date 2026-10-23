//! Independent reader of the Python 1.3 JSON wire format; no Python code is reused.
use crate::management::Transfer;
use anyhow::{bail, Context, Result};
use chrono::{NaiveDateTime, TimeZone};
use chrono_tz::Tz;
use kairo_core::{LegacyAttempt, Quota, State};
use serde_json::Value;
fn stamp(v: &Value) -> Result<i64> {
    if let Some(n) = v.as_i64() {
        if n >= 0 {
            return Ok(n);
        }
    }
    if let Some(n) = v.as_f64() {
        if n.is_finite() && n >= 0.0 && n < (i64::MAX as f64) {
            return Ok(n.ceil() as i64);
        }
    }
    if let Some(s) = v.as_str() {
        let n = chrono::DateTime::parse_from_rfc3339(s)?.timestamp();
        if n >= 0 {
            return Ok(n);
        }
    }
    bail!("legacy_timestamp_invalid / 旧状态时间戳无效")
}
fn anchor(key: &str, tz: Tz) -> Result<String> {
    let d = NaiveDateTime::parse_from_str(key, "%Y-%m-%dT%H:%M")
        .or_else(|_| NaiveDateTime::parse_from_str(key, "%Y-%m-%d/%H:%M"))?;
    if tz.from_local_datetime(&d).earliest().is_none() {
        bail!("legacy_anchor_invalid / 旧状态锚点不是有效本地时间")
    };
    Ok(d.format("%Y-%m-%d/%H:%M").to_string())
}
fn whole(v: &Value) -> Result<i64> {
    let n = v
        .as_f64()
        .context("legacy_quota_invalid / 旧状态额度记录无效")?;
    if !n.is_finite() || n < 0.0 || n.fract() != 0.0 || n >= i64::MAX as f64 {
        bail!("legacy_quota_invalid / 旧状态额度记录无效")
    }
    Ok(n as i64)
}
fn quota(v: &Value, at: i64) -> Result<Option<Quota>> {
    if v.is_null() {
        return Ok(None);
    }
    let duration = if v["limit_window_seconds"].is_null() {
        whole(&v["seconds"])?
    } else {
        whole(&v["limit_window_seconds"])?
    };
    if !v["seconds"].is_null() && whole(&v["seconds"])? != duration {
        bail!("legacy_quota_invalid / 旧状态额度记录无效")
    }
    let reset = stamp(&v["reset_at"])?;
    let q = Quota {
        duration,
        used: v["used_percent"]
            .as_f64()
            .context("legacy_quota_invalid / 旧状态额度记录无效")?,
        reset_at: reset,
        remaining: if v["reset_after_seconds"].is_null() {
            reset.saturating_sub(at).clamp(0, 18000)
        } else {
            whole(&v["reset_after_seconds"])?
        },
    };
    if !q.valid(reset.saturating_sub(q.remaining), 18000) {
        bail!("legacy_quota_invalid / 旧状态额度记录无效")
    };
    Ok(Some(q))
}
fn history(v: &Value, state: &mut State) -> Result<()> {
    match v {
        Value::Null => {}
        Value::Array(a) => {
            for v in a {
                history(v, state)?
            }
        }
        Value::Object(map) => {
            if map.contains_key("reset_at") {
                let at = stamp(&v["at"])?;
                let q = quota(v, at)?.context("legacy_quota_invalid / 旧状态额度记录无效")?;
                if q.used > 0.0 {
                    state.positive_until = state.positive_until.max(q.reset_at)
                };
                kairo_core::observe(state, at, q);
            } else {
                for v in map.values() {
                    history(v, state)?
                }
            }
        }
        _ => bail!("legacy_history_invalid / 旧窗口历史结构无效"),
    };
    Ok(())
}
pub fn convert(v: &Value, legacy_account: &str, new_account: &str, tz: Tz) -> Result<Transfer> {
    if v["v"] != 1
        || v["account"] != legacy_account
        || legacy_account.len() < 16
        || !legacy_account.bytes().all(|b| b.is_ascii_hexdigit())
        || new_account.len() != 64
        || !new_account.bytes().all(|b| b.is_ascii_hexdigit())
    {
        bail!("legacy_schema_invalid / 旧状态版本或账号指纹不匹配")
    }
    let object = v
        .as_object()
        .context("legacy_schema_invalid / 旧状态版本或账号指纹不匹配")?;
    for key in object.keys() {
        if ![
            "v",
            "account",
            "attempts",
            "processed",
            "pending",
            "last",
            "window_history",
            "window_last",
            "prewarm_last",
            "request_evidence",
        ]
        .contains(&key.as_str())
        {
            bail!("legacy_unknown_field / 旧状态存在未知字段，禁止猜测迁移")
        }
    }
    let mut state = State {
        schema: 1,
        account: new_account.into(),
        ..Default::default()
    };
    let attempts = v["attempts"]
        .as_object()
        .context("legacy_attempts_invalid / 旧调用历史结构无效")?;
    for (key, record) in attempts {
        let at = stamp(&record["at"])?;
        let before = quota(&record["before"], at)?;
        let after = quota(&record["after"], at)?;
        for q in [&before, &after].into_iter().flatten() {
            if q.used > 0.0 {
                state.positive_until = state.positive_until.max(q.reset_at)
            }
        }
        let outcome = record["outcome"]
            .as_str()
            .filter(|s| !s.is_empty())
            .context("legacy_attempts_invalid / 旧调用历史结构无效")?;
        if outcome.len() > 128 {
            bail!("legacy_attempts_invalid / 旧调用历史结构无效")
        };
        let request_ok = record
            .get("cli_success")
            .map(|v| {
                v.as_bool()
                    .context("legacy_attempts_invalid / 旧调用历史结构无效")
            })
            .transpose()?;
        state.legacy_attempts.push(LegacyAttempt {
            anchor: anchor(key, tz)?,
            at,
            outcome: outcome.into(),
            request_ok,
            before,
            after,
        });
        state.last_clock = state.last_clock.max(at);
    }
    let processed = v["processed"]
        .as_object()
        .context("legacy_processed_invalid / 旧锚点处理记录无效")?;
    for key in processed.keys() {
        let key = anchor(key, tz)?;
        if !state.legacy_attempts.iter().any(|a| a.anchor == key) {
            state.legacy_attempts.push(LegacyAttempt {
                anchor: key,
                at: 0,
                outcome: "processed_without_request".into(),
                request_ok: None,
                before: None,
                after: None,
            });
        }
    }
    history(&v["window_history"], &mut state)?;
    history(&v["window_last"], &mut state)?;
    if !v["last"].is_null() {
        let last = &v["last"];
        state.last_clock = state.last_clock.max(stamp(&last["at"])?);
        if last["result"]
            .as_str()
            .is_some_and(|r| r.contains("request"))
        {
            let key = anchor(
                last["anchor"]
                    .as_str()
                    .context("legacy_attempts_invalid / 旧调用历史结构无效")?,
                tz,
            )?;
            if !state
                .legacy_attempts
                .iter()
                .any(|a| a.anchor == key && a.at > 0)
            {
                bail!("legacy_attempts_invalid / 旧调用历史结构无效")
            }
        }
    }
    if !v["request_evidence"].is_null() {
        let evidence = &v["request_evidence"];
        let key = anchor(
            evidence["anchor"]
                .as_str()
                .context("legacy_attempts_invalid / 旧调用历史结构无效")?,
            tz,
        )?;
        let a = state
            .legacy_attempts
            .iter()
            .find(|a| a.anchor == key && a.at > 0)
            .context("legacy_attempts_invalid / 旧调用历史结构无效")?;
        quota(&evidence["before"], a.at)?;
        quota(&evidence["after"], a.at)?;
    }

    state.observations.sort_by_key(|o| o.at);
    state
        .observations
        .dedup_by(|a, b| a.at == b.at && a.quota.reset_at == b.quota.reset_at);
    if state.observations.len() > 64 {
        state.observations.drain(..state.observations.len() - 64);
    }
    state.pending = None;
    state.validate()?;
    Ok(Transfer {
        schema: 1,
        source: "normalized-kairo-transfer".into(),
        state,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn preserves_missing_snapshot_and_processed_guards() {
        let v = json!({"v":1,"account":"aaaaaaaaaaaaaaaa","attempts":{"2023-11-14T11:00":{"at":1700000000,"outcome":"request_ok_window_unconfirmed","cli_success":true}},"processed":{"2023-11-14T06:00":true},"pending":{"due":1700000006}});
        let t = convert(
            &v,
            "aaaaaaaaaaaaaaaa",
            &"b".repeat(64),
            chrono_tz::Asia::Shanghai,
        )
        .unwrap();
        assert_eq!(t.state.legacy_attempts.len(), 2);
        assert_eq!(t.state.legacy_attempts[0].at, 1700000000);
        assert!(t.state.pending.is_none());
        assert!(t.state.legacy_attempts[0].before.is_none());
    }
    #[test]
    fn rejects_unknown_or_malformed_state() {
        let mut v = json!({"v":1,"account":"aaaaaaaaaaaaaaaa","attempts":{},"processed":{}});
        v["unknown_safety_field"] = true.into();
        assert!(convert(&v, "aaaaaaaaaaaaaaaa", &"b".repeat(64), chrono_tz::UTC).is_err());
        v.as_object_mut().unwrap().remove("unknown_safety_field");
        v["attempts"] = json!({"bad":{"at":1,"outcome":"unknown"}});
        assert!(convert(&v, "aaaaaaaaaaaaaaaa", &"b".repeat(64), chrono_tz::UTC).is_err());
    }
}

#[cfg(test)]
mod legacy_wire_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn accepts_13_snapshot_units_and_float_seconds_without_losing_unknown_result() {
        let snapshot = json!({"seconds":18000,"used_percent":0.0,"reset_at":118000.0,"reset_after_seconds":18000.0});
        let history = json!({"at":100000.2,"limit_window_seconds":18000,"used_percent":0.0,"reset_at":118000.0,"reset_after_seconds":18000.0});
        let v = json!({"v":1,"account":"aaaaaaaaaaaaaaaa","attempts":{"2023-11-14T11:00":{"at":100000,"outcome":"request_ok_window_unconfirmed","cli_success":true,"before":snapshot,"after":snapshot}},"processed":{"2023-11-14T06:00":"skipped"},"window_history":{"2023-11-14T11:00":[history]},"window_last":history,"request_evidence":{"anchor":"2023-11-14T11:00","before":snapshot,"after":snapshot},"last":{"anchor":"2023-11-14T11:00","at":100001,"result":"request_ok_window_unconfirmed"},"pending":null});
        let t = convert(
            &v,
            "aaaaaaaaaaaaaaaa",
            &"b".repeat(64),
            chrono_tz::Asia::Shanghai,
        )
        .unwrap();
        assert_eq!(t.state.legacy_attempts.len(), 2);
        assert_eq!(
            t.state.legacy_attempts[0].before.as_ref().unwrap().duration,
            18000
        );
        assert_eq!(
            t.state.legacy_attempts[0].outcome,
            "request_ok_window_unconfirmed"
        );
        assert_eq!(t.state.last_clock, 100001);
        assert!(!t.state.observations.is_empty());
        let mut broken = v.clone();
        broken["request_evidence"]["anchor"] = "2023-11-14T16:00".into();
        assert!(convert(
            &broken,
            "aaaaaaaaaaaaaaaa",
            &"b".repeat(64),
            chrono_tz::Asia::Shanghai
        )
        .is_err());
        let mut broken = v;
        broken["attempts"]["2023-11-14T11:00"]["before"]["seconds"] = 17999.into();
        assert!(convert(
            &broken,
            "aaaaaaaaaaaaaaaa",
            &"b".repeat(64),
            chrono_tz::Asia::Shanghai
        )
        .is_err());
    }
}
