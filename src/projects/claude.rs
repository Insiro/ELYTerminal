//! Claude Code sessions a tab can resume: their ids and transcripts.
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

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

/// The mode a restored tab takes from its transcript's search: resume unless surely absent.
pub fn mode(found: Option<bool>) -> Mode {
    match found {
        Some(false) => Mode::New,
        Some(true) | None => Mode::Resume,
    }
}

/// The modes of restored ids, found off the main thread from the start.
#[derive(Debug, Default)]
pub struct Modes {
    finding: Option<std::thread::JoinHandle<HashMap<String, Mode>>>,
    found: HashMap<String, Mode>,
}

impl Modes {
    /// Looks for each of `ids`' transcripts on a thread of its own.
    pub fn find(ids: Vec<String>) -> Self {
        if ids.is_empty() {
            return Self::default();
        }
        let finding = std::thread::spawn(move || {
            let dir = config_dir();
            ids.into_iter()
                .map(|id| {
                    let found = dir.as_deref().and_then(|dir| transcript(&id, dir));
                    let mode = mode(found);
                    log::info!("claude: {id} restores as {mode:?}");
                    (id, mode)
                })
                .collect()
        });
        Self {
            finding: Some(finding),
            found: HashMap::new(),
        }
    }

    /// The mode `id` restores in; Resume when it was not looked for.
    pub fn of(&mut self, id: &str) -> Mode {
        if let Some(finding) = self.finding.take() {
            match finding.join() {
                Ok(found) => self.found = found,
                Err(_) => log::error!("claude: the transcript search panicked"),
            }
        }
        self.found.get(id).copied().unwrap_or(Mode::Resume)
    }
}

/// An OSC 133 mark, as the watch on a Claude tab hears it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heard {
    Prompt,
    Input,
    Output,
    Done(Option<i32>),
}

/// Follows a Claude tab's marks: the integration's prompt start, then its command, until that command ends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Watch {
    #[default]
    Starting,
    /// A prompt started; Claude's command starts before its input does.
    Prompted,
    Running,
    /// The first prompt read input: Claude never started.
    Off,
}

impl Watch {
    /// Hears `mark`; true once Claude's command ended with a status.
    pub fn hear(&mut self, mark: Heard) -> bool {
        match (*self, mark) {
            (Self::Starting, Heard::Prompt) => *self = Self::Prompted,
            (Self::Prompted, Heard::Output) => *self = Self::Running,
            (Self::Prompted, Heard::Input) => *self = Self::Off,
            (Self::Running, Heard::Done(Some(_))) => {
                *self = Self::Off;
                return true;
            }
            _ => {}
        }
        false
    }
}
