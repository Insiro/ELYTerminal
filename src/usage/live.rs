//! Readings fetched from the agents' own services, as Orca fetches them, and when each fetch is due.

use std::time::{Duration, Instant};

use super::Limits;

/// How often a watched source is fetched.
pub const EVERY: Duration = Duration::from_secs(5 * 60);
/// A rate limit without `Retry-After` waits this long.
pub const LIMITED: Duration = Duration::from_secs(15 * 60);
/// The longest wait `Retry-After` may ask.
const LONGEST: Duration = Duration::from_secs(24 * 60 * 60);

/// Where a reading came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// Claude's OAuth usage API.
    Api,
    /// The input Claude Code hands its statusLine.
    StatusLine,
    /// `codex app-server`.
    AppServer,
    /// ChatGPT's usage page, Codex's fallback.
    Web,
    /// Codex's newest session file.
    SessionFile,
}

impl Origin {
    pub fn key(self) -> &'static str {
        match self {
            Self::Api => "et.usage.from_api",
            Self::StatusLine => "et.usage.from_statusline",
            Self::AppServer => "et.usage.from_appserver",
            Self::Web => "et.usage.from_web",
            Self::SessionFile => "et.usage.from_session",
        }
    }
}

/// One fetched reading.
#[derive(Clone, Debug, PartialEq)]
pub struct Fetched {
    pub limits: Limits,
    /// When it was fetched, Unix seconds.
    pub at: i64,
    pub origin: Origin,
    pub plan: Option<String>,
    /// Codex's credits balance, where it has any.
    pub credits: Option<String>,
}

/// Why a fetch gave nothing.
#[derive(Clone, Debug, PartialEq)]
pub enum Trouble {
    /// Claude's token expired or was refused; Claude refreshes it when it runs.
    Expired,
    /// No signed-in account to read.
    NoToken,
    /// Rate limited until this Unix second.
    Limited(i64),
    /// No `codex` found.
    NoCodex,
    Failed(String),
}

/// What one fetch ended with.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    Read(Fetched),
    Trouble(Trouble),
    /// A 429, with the wait it asked.
    Limited(Option<u64>),
}

/// When the next fetch of one source is due.
#[derive(Debug, Default)]
pub struct Schedule {
    next: Option<Instant>,
    /// A rate limit's end, which a new watch does not pass.
    hold: Option<Instant>,
    watched: bool,
    running: bool,
}

impl Schedule {
    /// Whether to fetch now: at a watch's start, then every [`EVERY`], never while held or running.
    pub fn due(&mut self, watched: bool, now: Instant) -> bool {
        let started = watched && !self.watched;
        self.watched = watched;
        if !watched || self.running || self.hold.is_some_and(|until| now < until) {
            return false;
        }
        if started || self.next.is_none_or(|next| now >= next) {
            self.running = true;
            return true;
        }
        false
    }

    /// Notes a fetch's end; a rate limit holds every fetch for its wait, at least [`EVERY`].
    pub fn done(&mut self, outcome: &Outcome, now: Instant) {
        self.running = false;
        self.next = Some(now + EVERY);
        if let Outcome::Limited(after) = outcome {
            let wait = after
                .map_or(LIMITED, Duration::from_secs)
                .clamp(EVERY, LONGEST);
            self.hold = Some(now + wait);
            self.next = Some(now + wait);
        }
    }

    /// The wait a rate limit asked, as `Trouble::Limited` shows it.
    pub fn limited(after: Option<u64>, now: i64) -> Trouble {
        let wait = after
            .map_or(LIMITED, Duration::from_secs)
            .clamp(EVERY, LONGEST);
        Trouble::Limited(now + wait.as_secs() as i64)
    }
}

/// A fetched source: its last reading, why the last fetch gave none, and its schedule.
#[derive(Debug, Default)]
pub struct Live {
    pub value: Option<Fetched>,
    pub trouble: Option<Trouble>,
    pub schedule: Schedule,
}

impl Live {
    /// Takes a fetch's outcome, logging each change; a trouble keeps the last reading.
    pub fn set(&mut self, name: &str, outcome: Outcome, now: i64) -> bool {
        let trouble = match outcome {
            Outcome::Read(fetched) => {
                if self.value.as_ref().map(|v| &v.limits) != Some(&fetched.limits) {
                    log::info!(
                        "usage: {name} reads {:?} from {:?}",
                        fetched.limits,
                        fetched.origin
                    );
                }
                self.value = Some(fetched);
                None
            }
            Outcome::Trouble(trouble) => Some(trouble),
            Outcome::Limited(after) => Some(Schedule::limited(after, now)),
        };
        if trouble != self.trouble {
            log::info!("usage: {name}'s fetch trouble is {trouble:?}");
        }
        self.trouble = trouble;
        true
    }

    /// Forgets the reading as its setting turns off.
    pub fn clear(&mut self) -> bool {
        let had = self.value.is_some() || self.trouble.is_some();
        self.value = None;
        self.trouble = None;
        had
    }
}
