//! Claude's limits from its OAuth usage API, as Orca and Claude Code's `/usage` read them; never refreshes a token.

use anyhow::Context as _;
use serde_json::Value;

use super::{
    Limits, Window, keychain,
    live::{Fetched, Origin, Outcome, Trouble},
    run, time,
};

const URL: &str = "https://api.anthropic.com/api/oauth/usage";
/// The client Orca names itself as.
const AGENT: &str = "claude-code/2.1.0";

/// Fetches the signed-in account's limits; runs off the main thread.
pub fn fetch() -> Outcome {
    log::info!("usage: claude's API fetch starts");
    let found = match keychain::read() {
        Ok(found) => found,
        Err(error) => return failed(format!("{error:#}")),
    };
    let Some(credentials) = found else {
        log::info!("usage: claude has no OAuth token");
        return Outcome::Trouble(Trouble::NoToken);
    };
    let now = time::now();
    if credentials.expired(now * 1000) {
        log::info!("usage: claude's token expired; Claude refreshes it when it runs");
        return Outcome::Trouble(Trouble::Expired);
    }
    let bearer = format!("Bearer {}", credentials.token.0);
    let headers = [
        ("Authorization", bearer.as_str()),
        ("anthropic-beta", "oauth-2025-04-20"),
        ("User-Agent", AGENT),
    ];
    let answer = match run::get(URL, &headers) {
        Ok(answer) => answer,
        Err(error) => return failed(format!("{error:#}")),
    };
    log::info!("usage: claude's API answers {}", answer.status);
    outcome(answer, credentials.plan, now)
}

fn failed(why: String) -> Outcome {
    log::warn!("{why}");
    Outcome::Trouble(Trouble::Failed(why))
}

/// What an answer means.
pub fn outcome(answer: run::Answer, plan: Option<String>, now: i64) -> Outcome {
    match answer.status {
        200 => match parse(&answer.body) {
            Ok(limits) => Outcome::Read(Fetched {
                limits,
                at: now,
                origin: Origin::Api,
                plan,
                credits: None,
            }),
            Err(error) => failed(format!("{error:#}")),
        },
        401 | 403 => Outcome::Trouble(Trouble::Expired),
        429 => {
            log::warn!(
                "usage: claude's API is rate limited, retry after {:?}",
                answer.retry_after
            );
            Outcome::Limited(answer.retry_after)
        }
        status => failed(format!("usage: claude's API answered HTTP {status}")),
    }
}

/// The 5-hour and weekly windows in an answer's body.
pub fn parse(body: &[u8]) -> anyhow::Result<Limits> {
    let body: Value =
        serde_json::from_slice(body).context("usage: claude's API answer is not JSON")?;
    anyhow::ensure!(body.is_object(), "usage: claude's API answer is no object");
    Ok(Limits {
        five_hour: window(body.get("five_hour"), 300),
        seven_day: window(body.get("seven_day"), 10_080),
        spend: None,
    })
}

fn window(value: Option<&Value>, minutes: u32) -> Option<Window> {
    let value = value?;
    let percent = value
        .get("utilization")
        .and_then(Value::as_f64)
        .or_else(|| value.get("used_percentage").and_then(Value::as_f64))
        .filter(|p| p.is_finite())?;
    Some(Window {
        percent: percent.clamp(0.0, 100.0),
        resets_at: resets(value.get("resets_at")?)?,
        minutes: Some(minutes),
    })
}

/// Unix seconds from seconds, milliseconds or an RFC 3339 time with any offset and fraction.
pub fn resets(value: &Value) -> Option<i64> {
    let secs = |n: f64| (n.is_finite()).then(|| if n > 1e10 { n / 1000.0 } else { n } as i64);
    if let Some(n) = value.as_f64() {
        return secs(n);
    }
    let text = value.as_str()?.trim();
    if let Ok(n) = text.parse::<f64>() {
        return secs(n);
    }
    rfc3339(text)
}

/// `2026-11-06T12:00:00.123456+00:00` or `…Z`, as Unix seconds.
fn rfc3339(text: &str) -> Option<i64> {
    let base = text.get(..19)?;
    let rest = &text[19..];
    let rest = rest.strip_prefix('.').map_or(rest, |frac| {
        frac.trim_start_matches(|c: char| c.is_ascii_digit())
    });
    let offset = match rest {
        "Z" | "z" => 0,
        _ => {
            let sign = match rest.as_bytes().first()? {
                b'+' => 1,
                b'-' => -1,
                _ => return None,
            };
            let (hours, minutes) = rest[1..].split_once(':')?;
            let (hours, minutes): (i64, i64) = (hours.parse().ok()?, minutes.parse().ok()?);
            sign * (hours * 3_600 + minutes * 60)
        }
    };
    Some(time::parse_utc(&format!("{base}Z"))? - offset)
}
