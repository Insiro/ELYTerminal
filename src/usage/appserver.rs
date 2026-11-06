//! Codex's limits from `codex app-server` over JSON-RPC on stdio, as Orca asks it; ChatGPT's page when it fails.

use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context as _, bail};
use serde_json::{Value, json};

use super::{
    Limits, Window,
    live::{Fetched, Origin, Outcome, Trouble},
    run::{Reaped, drain},
    time, web,
};

/// Each answer's wait.
const WAIT: Duration = Duration::from_secs(10);
/// The arguments Orca starts the server with: read only, never asking.
const ARGS: [&str; 5] = ["-s", "read-only", "-a", "never", "app-server"];

/// What the login shell found, asked once a run: `codex` and the folder of its `node`, each by its real path.
static FOUND: Mutex<Option<Option<Found>>> = Mutex::new(None);

/// `codex` and its `node`'s folder.
type Found = (PathBuf, Option<PathBuf>);

/// Fetches Codex's limits: the app server first, then ChatGPT's page; runs off the main thread.
pub fn fetch() -> Outcome {
    log::info!("usage: codex's live fetch starts");
    let now = time::now();
    let failed = match program() {
        None => {
            log::info!("usage: no codex found on PATH, in its usual folders or by the login shell");
            None
        }
        Some(codex) => match ask(&codex, &ARGS, WAIT).and_then(|result| parse(&result, now)) {
            Ok(fetched) => {
                log::info!("usage: codex's app server at {} answers", codex.display());
                return Outcome::Read(fetched);
            }
            Err(error) => {
                log::warn!("usage: codex's app server failed: {error:#}; ChatGPT's page next");
                Some(format!("{error:#}"))
            }
        },
    };
    let web = web::fetch(now).inspect_err(|error| log::warn!("{error:#}"));
    match (web, failed) {
        (Ok(Some(fetched)), _) => Outcome::Read(fetched),
        (_, None) => Outcome::Trouble(Trouble::NoCodex),
        (_, Some(why)) => Outcome::Trouble(Trouble::Failed(why)),
    }
}

/// Starts `program` with `args`, says initialize, initialized, then asks `account/rateLimits/read`; the child is always killed.
pub fn ask(program: &Path, args: &[&str], wait: Duration) -> anyhow::Result<Value> {
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(std::env::temp_dir())
        .env("PATH", search_path(program))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(
        &mut command,
        crate::git::CREATE_NO_WINDOW,
    );
    let child = command
        .spawn()
        .with_context(|| format!("usage: {} did not start", program.display()))?;
    let mut child = Reaped {
        child,
        group: cfg!(unix),
    };
    let mut stdin = child.child.stdin.take().context("usage: no stdin")?;
    let stdout = child.child.stdout.take().context("usage: no stdout")?;
    let stderr = drain(child.child.stderr.take());
    let (lines, read) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if lines.send(line).is_err() {
                break;
            }
        }
    });
    let mut say = |message: Value| -> anyhow::Result<()> {
        writeln!(stdin, "{message}").context("usage: codex's app server closed its input")?;
        stdin
            .flush()
            .context("usage: codex's app server closed its input")
    };
    say(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"clientInfo": {"name": "elyterminal", "version": crate::updates::VERSION}}}))?;
    let answered = answer(&read, 1, wait);
    let answered = answered.and_then(|_| {
        say(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}))?;
        say(json!({"jsonrpc": "2.0", "id": 2, "method": "account/rateLimits/read", "params": {}}))?;
        answer(&read, 2, wait)
    });
    drop(child);
    answered.map_err(|error| {
        let said = stderr.join();
        let said = String::from_utf8_lossy(&said);
        let tail = said.trim().lines().last().unwrap_or_default().to_string();
        match tail.is_empty() {
            true => error,
            false => error.context(tail),
        }
    })
}

/// The result of the answer to `id`, passing notifications and other lines by.
fn answer(read: &mpsc::Receiver<String>, id: i64, wait: Duration) -> anyhow::Result<Value> {
    let deadline = Instant::now() + wait;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let line = match read.recv_timeout(left) {
            Ok(line) => line,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                bail!(
                    "usage: codex's app server gave no answer to {id} in {}s",
                    wait.as_secs()
                )
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                bail!("usage: codex's app server exited before answering {id}")
            }
        };
        let Ok(message) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        if message.get("id").and_then(Value::as_i64) != Some(id) {
            continue;
        }
        if let Some(error) = message.get("error") {
            let why = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("no message");
            bail!("usage: codex's app server refused {id}: {why}");
        }
        return Ok(message.get("result").cloned().unwrap_or(Value::Null));
    }
}

/// The reading in `account/rateLimits/read`'s result.
pub fn parse(result: &Value, now: i64) -> anyhow::Result<Fetched> {
    let rates = result
        .get("rateLimits")
        .filter(|rates| rates.is_object())
        .context("usage: codex's app server named no rateLimits")?;
    let window = |key: &str| {
        let window = rates.get(key)?;
        let percent = window.get("usedPercent")?.as_f64()?;
        let minutes = window.get("windowDurationMins").and_then(Value::as_u64);
        let resets = window.get("resetsAt").and_then(Value::as_i64)?;
        Some((percent, minutes.map(|m| m as u32), resets))
    };
    let credits = rates.get("credits").and_then(|credits| {
        if credits.get("unlimited").and_then(Value::as_bool) == Some(true) {
            return Some("unlimited".to_string());
        }
        (credits.get("hasCredits").and_then(Value::as_bool) == Some(true))
            .then(|| match credits.get("balance")? {
                Value::String(text) => Some(text.clone()),
                Value::Number(n) => Some(n.to_string()),
                _ => None,
            })
            .flatten()
    });
    Ok(Fetched {
        limits: limits(window("primary"), window("secondary")),
        at: now,
        origin: Origin::AppServer,
        plan: rates
            .get("planType")
            .and_then(Value::as_str)
            .map(str::to_string),
        credits,
    })
}

/// A window: its percent, its length in minutes and its reset in Unix seconds.
pub type Raw = (f64, Option<u32>, i64);

/// Places two windows by their lengths, as Orca does: 300 minutes the 5-hour, 10080 the weekly, else primary then secondary.
pub fn limits(primary: Option<Raw>, secondary: Option<Raw>) -> Limits {
    let kind = |minutes: Option<u32>| match minutes {
        Some(m) if m.abs_diff(300) <= 1 => Some(300),
        Some(m) if m.abs_diff(10_080) <= 1 => Some(10_080),
        _ => None,
    };
    let (mut five, mut seven) = (None, None);
    for raw in [primary, secondary].into_iter().flatten() {
        match kind(raw.1) {
            Some(300) if five.is_none() => five = Some(raw),
            Some(10_080) if seven.is_none() => seven = Some(raw),
            _ => {}
        }
    }
    if five.is_none() && primary.is_some_and(|raw| kind(raw.1).is_none()) {
        five = primary;
    }
    if seven.is_none() && secondary.is_some_and(|raw| kind(raw.1).is_none()) {
        seven = secondary;
    }
    let window = |raw: Option<Raw>, default: u32| {
        raw.map(|(percent, minutes, resets_at)| Window {
            percent: percent.clamp(0.0, 100.0),
            resets_at,
            minutes: Some(minutes.unwrap_or(default)),
        })
    };
    Limits {
        five_hour: window(five, 300),
        seven_day: window(seven, 10_080),
        spend: None,
    }
}

/// `PATH` with `program`'s own folder first, so its `#!/usr/bin/env node` finds the node beside it.
fn search_path(program: &Path) -> std::ffi::OsString {
    let mut folders: Vec<PathBuf> = program
        .parent()
        .map(Path::to_path_buf)
        .into_iter()
        .collect();
    let found = FOUND.lock().unwrap_or_else(|poison| poison.into_inner());
    folders.extend(found.iter().flatten().filter_map(|(_, node)| node.clone()));
    folders.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    folders.extend(usual());
    std::env::join_paths(folders).unwrap_or_default()
}

/// Folders a GUI app's `PATH` lacks that hold `codex` and `node`.
fn usual() -> Vec<PathBuf> {
    let mut folders = vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ];
    if let Some(home) = dirs::home_dir() {
        for folder in [
            ".local/bin",
            ".local/share/fnm/aliases/default/bin",
            "Library/Application Support/fnm/aliases/default/bin",
            ".volta/bin",
            ".bun/bin",
            ".npm-global/bin",
        ] {
            folders.push(home.join(folder));
        }
    }
    folders
}

/// `codex`: on `PATH`, in the usual folders, else where the user's login shell finds it.
pub fn program() -> Option<PathBuf> {
    let name = if cfg!(windows) { "codex.cmd" } else { "codex" };
    let on_path = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .chain(usual())
        .filter(|folder| folder.is_absolute())
        .map(|folder| folder.join(name))
        .find(|file| file.is_file());
    if on_path.is_some() {
        return on_path;
    }
    let mut found = FOUND.lock().unwrap_or_else(|poison| poison.into_inner());
    if found
        .as_ref()
        .is_none_or(|kept| kept.as_ref().is_some_and(|(codex, _)| !codex.is_file()))
    {
        *found = Some(shell_found());
    }
    found.clone().flatten().map(|(codex, _)| codex)
}

#[cfg(unix)]
fn shell_found() -> Option<Found> {
    let shell = std::env::var_os("SHELL").filter(|shell| !shell.is_empty())?;
    let mut command = Command::new(&shell);
    command.args([
        "-ilc",
        "printf '\\036%s\\036%s\\036' \"$(command -v codex)\" \"$(command -v node)\"",
    ]);
    let output = super::run::run(command, b"", Duration::from_secs(5))
        .inspect_err(|error| log::warn!("{error:#}"))
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let mut parts = text.split('\u{1e}').skip(1);
    // A version manager's shell folder goes with that shell; the real paths stay.
    let real = |path: Option<&str>| {
        path.filter(|path| path.starts_with('/'))
            .and_then(|path| std::fs::canonicalize(path).ok())
    };
    let codex = real(parts.next()).filter(|path| path.is_file());
    let node = real(parts.next()).and_then(|node| node.parent().map(Path::to_path_buf));
    match &codex {
        Some(path) => log::info!("usage: the login shell finds codex at {}", path.display()),
        None => log::info!("usage: the login shell finds no codex"),
    }
    codex.map(|codex| (codex, node))
}

#[cfg(not(unix))]
fn shell_found() -> Option<Found> {
    None
}
