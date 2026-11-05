//! Codex's limits and tokens from its newest session file; the conversation is never parsed.

use std::{
    fs::{self, File},
    io::{self, BufRead, BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::SystemTime,
};

use anyhow::{Context as _, bail};
use serde::Deserialize;

use super::{Limits, Window, time::parse_utc};

/// A compressed file past this is not read.
pub const PACKED: u64 = 64 << 20;
/// Decompressed bytes past this end the read.
pub const UNPACKED: u64 = 256 << 20;
/// A line past this is skipped unparsed.
pub const LINE: usize = 4 << 20;
/// A plain file's tail that is read.
pub const TAIL: u64 = 1 << 20;
/// Day folders looked in, newest first.
const DAYS: usize = 3;

/// Codex's folder: `CODEX_HOME`, else `~/.codex`.
pub fn home() -> Option<PathBuf> {
    match std::env::var_os("CODEX_HOME") {
        Some(dir) if !dir.is_empty() => Some(dir.into()),
        _ => dirs::home_dir().map(|home| home.join(".codex")),
    }
}

/// What the newest session file says.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reading {
    pub limits: Limits,
    pub plan: Option<String>,
    /// When the limits were reported, Unix seconds.
    pub limits_at: Option<i64>,
    pub tokens: Option<Tokens>,
}

/// The session's tokens and its last turn's.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Tokens {
    pub input: u64,
    pub cached: u64,
    pub output: u64,
    pub total: u64,
    pub last: u64,
    pub window: Option<u64>,
}

/// The rollout file changed last in the newest day folders of `sessions`; none when there is none.
pub fn newest(sessions: &Path) -> io::Result<Option<PathBuf>> {
    let mut days = Vec::new();
    'years: for year in folders(sessions)? {
        for month in folders(&year)? {
            for day in folders(&month)? {
                days.push(day);
                if days.len() == DAYS {
                    break 'years;
                }
            }
        }
    }
    let mut best: Option<(SystemTime, PathBuf)> = None;
    for day in days {
        for entry in fs::read_dir(&day)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let rollout = name.starts_with("rollout-")
                && (name.ends_with(".jsonl") || name.ends_with(".jsonl.zst"));
            if !rollout {
                continue;
            }
            let modified = entry.metadata()?.modified()?;
            if best.as_ref().is_none_or(|(time, _)| modified > *time) {
                best = Some((modified, entry.path()));
            }
        }
    }
    Ok(best.map(|(_, path)| path))
}

/// `folder`'s subfolders, newest name first; none when it is missing.
fn folders(folder: &Path) -> io::Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut found = Vec::new();
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            found.push(entry.path());
        }
    }
    found.sort_unstable_by(|a, b| b.cmp(a));
    Ok(found)
}

/// Reads `path`, `.zst` or plain, within the bounds; a truncated frame ends it quietly.
pub fn read(path: &Path) -> anyhow::Result<Reading> {
    let owner = || format!("usage: codex {}", path.display());
    let mut file = File::open(path).with_context(owner)?;
    let len = file.metadata().with_context(owner)?.len();
    let mut reading = Reading::default();
    let mut found = Found::default();
    if path.extension().is_some_and(|ext| ext == "zst") {
        if len > PACKED {
            bail!(
                "usage: codex {} is {len} bytes, past {PACKED}",
                path.display()
            );
        }
        let decoder = ruzstd::decoding::StreamingDecoder::new(BufReader::new(file))
            .map_err(|error| anyhow::anyhow!("usage: codex {}: {error}", path.display()))?;
        let mut lines = BufReader::new(decoder.take(UNPACKED + 1));
        let total = lines_of(&mut lines, &mut found, &mut reading);
        if total > UNPACKED {
            bail!(
                "usage: codex {} unpacks past {UNPACKED} bytes",
                path.display()
            );
        }
    } else {
        let from = len.saturating_sub(TAIL);
        file.seek(SeekFrom::Start(from)).with_context(owner)?;
        let mut lines = BufReader::new(file);
        if from > 0 {
            skip_line(&mut lines);
        }
        lines_of(&mut lines, &mut found, &mut reading);
    }
    if found.bad > 0 {
        log::debug!(
            "usage: codex {} had {} bad lines",
            path.display(),
            found.bad
        );
    }
    reading.limits_at = found.limits_at;
    Ok(reading)
}

#[derive(Default)]
struct Found {
    bad: usize,
    limits_at: Option<i64>,
    /// The kept limits came from Codex's own id.
    own: bool,
}

/// Reads every line, keeping the last limits and tokens; a read error ends it. Gives the bytes read.
fn lines_of(lines: &mut impl BufRead, found: &mut Found, reading: &mut Reading) -> u64 {
    let mut total = 0u64;
    let mut line = Vec::new();
    loop {
        line.clear();
        match next_line(lines, &mut line) {
            Ok((0, _)) => return total,
            Ok((read, whole)) => {
                total += read as u64;
                if whole {
                    take(&line, found, reading);
                }
            }
            Err(error) => {
                log::debug!("usage: codex read ends at {total} bytes: {error}");
                return total;
            }
        }
    }
}

/// Reads to the next newline, keeping at most `LINE` bytes; gives the bytes read and whether all were kept.
fn next_line(lines: &mut impl BufRead, line: &mut Vec<u8>) -> io::Result<(usize, bool)> {
    let mut read = 0;
    loop {
        let buffer = lines.fill_buf()?;
        if buffer.is_empty() {
            return Ok((read, read <= LINE));
        }
        let (part, done) = match memchr::memchr(b'\n', buffer) {
            Some(at) => (&buffer[..=at], true),
            None => (buffer, false),
        };
        let used = part.len();
        if read + used <= LINE + 1 {
            line.extend_from_slice(part);
        }
        read += used;
        lines.consume(used);
        if done {
            return Ok((read, read <= LINE + 1));
        }
    }
}

fn skip_line(lines: &mut impl BufRead) {
    let mut line = Vec::new();
    if let Err(error) = next_line(lines, &mut line) {
        log::debug!("usage: codex tail's first line: {error}");
    }
}

#[derive(Deserialize)]
struct Line {
    timestamp: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    payload: Option<Payload>,
}

#[derive(Deserialize)]
struct Payload {
    #[serde(rename = "type")]
    kind: Option<String>,
    info: Option<Info>,
    rate_limits: Option<Rates>,
}

#[derive(Deserialize)]
struct Info {
    total_token_usage: Option<Usage>,
    last_token_usage: Option<Usage>,
    model_context_window: Option<u64>,
}

#[derive(Deserialize)]
struct Usage {
    input_tokens: Option<u64>,
    cached_input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    total_tokens: Option<u64>,
}

#[derive(Deserialize)]
struct Rates {
    limit_id: Option<String>,
    primary: Option<Rate>,
    secondary: Option<Rate>,
    plan_type: Option<String>,
}

#[derive(Deserialize)]
struct Rate {
    used_percent: Option<f64>,
    window_minutes: Option<u32>,
    resets_at: Option<i64>,
    resets_in_seconds: Option<i64>,
}

/// Parses a `token_count` line, never another, into the reading.
fn take(line: &[u8], found: &mut Found, reading: &mut Reading) {
    if memchr::memmem::find(line, b"\"token_count\"").is_none() {
        return;
    }
    let Ok(line) = serde_json::from_slice::<Line>(line) else {
        found.bad += 1;
        return;
    };
    let Some(payload) = line
        .payload
        .filter(|_| line.kind.as_deref() == Some("event_msg"))
    else {
        return;
    };
    if payload.kind.as_deref() != Some("token_count") {
        return;
    }
    let at = line.timestamp.as_deref().and_then(parse_utc);
    if let Some(info) = payload.info {
        let total = info.total_token_usage;
        let field = |pick: fn(&Usage) -> Option<u64>| total.as_ref().and_then(pick).unwrap_or(0);
        reading.tokens = Some(Tokens {
            input: field(|u| u.input_tokens),
            cached: field(|u| u.cached_input_tokens),
            output: field(|u| u.output_tokens),
            total: field(|u| u.total_tokens),
            last: info
                .last_token_usage
                .and_then(|u| u.total_tokens)
                .unwrap_or(0),
            window: info.model_context_window,
        });
    }
    let Some(rates) = payload.rate_limits else {
        return;
    };
    let own = rates.limit_id.as_deref().is_none_or(|id| id == "codex");
    if found.own && !own {
        return;
    }
    let window = |rate: Option<Rate>| {
        let rate = rate?;
        let resets_at = rate
            .resets_at
            .or_else(|| Some(at? + rate.resets_in_seconds?))?;
        Some(Window {
            percent: rate.used_percent?,
            resets_at,
            minutes: rate.window_minutes,
        })
    };
    found.own = own;
    found.limits_at = at;
    reading.plan = rates.plan_type;
    reading.limits = Limits {
        five_hour: window(rates.primary),
        seven_day: window(rates.secondary),
        spend: None,
    };
}
