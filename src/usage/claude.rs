//! Claude's limits from the statusLine input the wrapper keeps.

use anyhow::Context as _;
use serde_json::Value;

use super::{Limits, Window};

/// The limits in one statusLine input; `None` when it reports none.
pub fn parse(text: &str) -> anyhow::Result<Option<Limits>> {
    let input: Value = serde_json::from_str(text).context("usage: claude's data is not JSON")?;
    let Some(rates) = input.get("rate_limits").filter(|rates| !rates.is_null()) else {
        return Ok(None);
    };
    let window = |key: &str, minutes: Option<u32>| {
        let window = rates.get(key)?;
        Some(Window {
            percent: window.get("used_percentage")?.as_f64()?,
            resets_at: seconds(window.get("resets_at")?)?,
            minutes,
        })
    };
    Ok(Some(Limits {
        five_hour: window("five_hour", Some(300)),
        seven_day: window("seven_day", Some(10_080)),
        spend: window("spend_limit", None),
    }))
}

/// Unix seconds, whole or not.
fn seconds(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_f64().filter(|f| f.is_finite()).map(|f| f as i64))
}
