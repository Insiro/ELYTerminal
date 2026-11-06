//! Codex's limits from ChatGPT's usage page, with the token Codex keeps in `auth.json`, read only.

use anyhow::Context as _;
use serde_json::Value;

use super::{
    appserver::{Raw, limits},
    codex,
    live::{Fetched, Origin},
    run::{self, Secret},
};

const URL: &str = "https://chatgpt.com/backend-api/wham/usage";

/// Codex's ChatGPT token and account.
#[derive(Debug, PartialEq)]
pub struct Auth {
    pub token: Secret,
    pub account: Option<String>,
}

/// The tokens in `auth.json`; none without an access token.
pub fn auth(text: &str) -> anyhow::Result<Option<Auth>> {
    let file: Value = serde_json::from_str(text).context("usage: codex's auth.json is not JSON")?;
    let tokens = file.get("tokens");
    let field = |key: &str| {
        tokens
            .and_then(|tokens| tokens.get(key))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    Ok(field("access_token").map(|token| Auth {
        token: Secret(token),
        account: field("account_id"),
    }))
}

/// Fetches the page; `Ok(None)` when Codex is not signed in.
pub fn fetch(now: i64) -> anyhow::Result<Option<Fetched>> {
    let Some(home) = codex::home() else {
        return Ok(None);
    };
    let path = home.join("auth.json");
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("usage: {}", path.display())),
    };
    let Some(auth) = auth(&text)? else {
        return Ok(None);
    };
    log::info!("usage: codex's web fetch starts");
    let bearer = format!("Bearer {}", auth.token.0);
    let mut headers = vec![
        ("Authorization", bearer.as_str()),
        ("User-Agent", "codex-cli"),
        ("OpenAI-Beta", "codex-1"),
        ("originator", "Codex Desktop"),
    ];
    if let Some(account) = &auth.account {
        headers.push(("ChatGPT-Account-Id", account));
    }
    let answer = run::get(URL, &headers)?;
    log::info!("usage: ChatGPT's usage page answers {}", answer.status);
    anyhow::ensure!(
        answer.status == 200,
        "usage: ChatGPT's usage page answered HTTP {}",
        answer.status
    );
    parse(&answer.body, now).map(Some)
}

/// The windows in the page's JSON.
pub fn parse(body: &[u8], now: i64) -> anyhow::Result<Fetched> {
    let page: Value = serde_json::from_slice(body).context("usage: ChatGPT's usage is not JSON")?;
    let plan = page
        .get("plan_type")
        .and_then(Value::as_str)
        .context("usage: ChatGPT's usage names no plan")?;
    let rates = page.get("rate_limit");
    let window = |key: &str| -> Option<Raw> {
        let window = rates?.get(key)?;
        let percent = window.get("used_percent")?.as_f64()?;
        let minutes = window
            .get("limit_window_seconds")
            .and_then(Value::as_u64)
            .filter(|&secs| secs > 0)
            .map(|secs| secs.div_ceil(60) as u32);
        Some((percent, minutes, window.get("reset_at")?.as_i64()?))
    };
    Ok(Fetched {
        limits: limits(window("primary_window"), window("secondary_window")),
        at: now,
        origin: Origin::Web,
        plan: Some(plan.to_string()),
        credits: None,
    })
}
