//! Claude Code sessions a tab can resume: their ids and transcripts.
use std::path::{Path, PathBuf};

/// How a tab starts Claude.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    New,
    Resume,
}

/// A random version 4 UUID, lowercase.
pub fn new_id() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("claude: the system gave no random bytes");
    bytes[6] = bytes[6] & 0x0f | 0x40;
    bytes[8] = bytes[8] & 0x3f | 0x80;
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

/// Whether `id` is exactly 8-4-4-4-12 lowercase hex.
pub fn valid(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(at, byte)| match at {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => matches!(byte, b'0'..=b'9' | b'a'..=b'f'),
        })
}

/// Claude's config folder: `CLAUDE_CONFIG_DIR`, else `~/.claude`.
pub fn config_dir() -> Option<PathBuf> {
    match std::env::var_os("CLAUDE_CONFIG_DIR") {
        Some(dir) if !dir.is_empty() => Some(dir.into()),
        _ => dirs::home_dir().map(|home| home.join(".claude")),
    }
}

/// Whether `projects/*/<id>.jsonl` exists; None when the folders cannot be read.
pub fn transcript(id: &str, config_dir: &Path) -> Option<bool> {
    let projects = match std::fs::read_dir(config_dir.join("projects")) {
        Ok(projects) => projects,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Some(false),
        Err(error) => {
            log::warn!("claude: cannot read {}: {error}", config_dir.display());
            return None;
        }
    };
    for entry in projects {
        let found = entry
            .and_then(|entry| entry.path().join(format!("{id}.jsonl")).try_exists())
            .map_err(|error| log::warn!("claude: cannot look for {id}: {error}"))
            .ok()?;
        if found {
            return Some(true);
        }
    }
    Some(false)
}

/// The value of `ELY_CLAUDE` for the integration scripts.
pub fn env(id: &str, mode: Mode) -> String {
    let word = match mode {
        Mode::New => "new",
        Mode::Resume => "resume",
    };
    format!("{word}:{id}")
}
