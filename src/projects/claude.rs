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

/// Whether `projects/*/<id>.jsonl` exists; None when the folders are missing or cannot be read.
pub fn transcript(id: &str, config_dir: &Path) -> Option<bool> {
    let projects = match std::fs::read_dir(config_dir.join("projects")) {
        Ok(projects) => projects,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            log::info!("claude: {} holds no projects", config_dir.display());
            return None;
        }
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

/// The key fish binds to Claude's line at its first prompt, a private-use character no keyboard types.
pub const KEY: &str = "\u{e000}";

/// How long a wake waits for the transcript search.
const SEARCH: std::time::Duration = std::time::Duration::from_millis(300);

/// The status a shell gives a job it suspended, 128 and SIGTSTP; Claude runs on.
#[cfg(unix)]
const SUSPENDED: Option<i32> = Some(128 + libc::SIGTSTP);
#[cfg(not(unix))]
const SUSPENDED: Option<i32> = None;

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

    /// The mode `id` restores in; Resume while the search runs or when it was not looked for.
    pub fn of(&mut self, id: &str) -> Mode {
        // The search reads a folder or two: waiting a little for it beats resuming a session never written.
        let since = std::time::Instant::now();
        while self
            .finding
            .as_ref()
            .is_some_and(|finding| !finding.is_finished())
            && since.elapsed() < SEARCH
        {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        if self
            .finding
            .as_ref()
            .is_some_and(|finding| finding.is_finished())
            && let Some(finding) = self.finding.take()
        {
            match finding.join() {
                Ok(found) => self.found = found,
                Err(_) => log::error!("claude: the transcript search panicked"),
            }
        }
        match self.found.get(id) {
            Some(mode) => *mode,
            None => {
                log::info!("claude: {id}'s transcript is not known yet, so it resumes");
                Mode::Resume
            }
        }
    }
}

/// An OSC 133 mark, as the watch on a Claude tab hears it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heard {
    Prompt,
    Input,
    Output,
    Done(Option<i32>),
    /// The integration's word that Claude does not start: D with `aid=elyterminal` and no status.
    Unstarted,
}

/// How Claude's watch ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    /// Its command ended with a status.
    Exited,
    /// The integration said, by a bare D before Claude's command, that it does not start.
    Skipped,
}

/// Follows a Claude tab's marks until Claude's command, the one a prompt start leads straight into, ends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Watch {
    #[default]
    Starting,
    /// A prompt started; a command now, before any input, is Claude's.
    Prompted,
    /// A prompt read input; Claude's command is still to come.
    Typed,
    Running,
    Off,
    /// cmd marks no command, so Claude's end goes unheard.
    Blind,
}

impl Watch {
    /// Hears `mark`; how the watch ended, once it does.
    pub fn hear(&mut self, mark: Heard) -> Option<End> {
        let end = match (*self, mark) {
            (Self::Starting | Self::Prompted | Self::Typed, Heard::Prompt) => {
                *self = Self::Prompted;
                None
            }
            (Self::Prompted, Heard::Input) => {
                *self = Self::Typed;
                None
            }
            (Self::Prompted, Heard::Output) => {
                *self = Self::Running;
                None
            }
            (Self::Starting | Self::Prompted | Self::Typed, Heard::Unstarted) => Some(End::Skipped),
            (Self::Running, Heard::Done(status)) if status.is_some() && status != SUSPENDED => {
                Some(End::Exited)
            }
            _ => None,
        };
        if end.is_some() {
            *self = Self::Off;
        }
        end
    }

    /// Whether Claude's command has yet to start.
    pub fn waiting(self) -> bool {
        matches!(self, Self::Starting | Self::Prompted | Self::Typed)
    }

    /// Whether a prompt has started since the shell did.
    pub fn prompted(self) -> bool {
        !matches!(self, Self::Starting)
    }

    /// Whether marks still matter to it.
    pub fn listening(self) -> bool {
        self.waiting() || self == Self::Running
    }
}

/// cmd's `/k` command: the prelude, then Claude; a prelude cmd would read apart, with `"` or `%`, is refused.
#[cfg(any(windows, test))]
pub fn cmd_line(id: &str, mode: Mode, prelude: &str) -> anyhow::Result<String> {
    anyhow::ensure!(
        !prelude.contains(['"', '%']),
        "claude_prelude holds \" or %, which cmd reads apart"
    );
    let word = match mode {
        Mode::New => "--session-id",
        Mode::Resume => "--resume",
    };
    Ok(match prelude {
        "" => format!("claude {word} {id}"),
        prelude => format!("{prelude} & claude {word} {id}"),
    })
}
