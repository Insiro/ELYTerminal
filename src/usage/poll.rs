//! The files usage reads, looked at by their stamps and read only when they change; runs off the main thread.

use std::{
    path::PathBuf,
    time::{Duration, Instant, UNIX_EPOCH},
};

use super::{
    Limits, claude, codex,
    install::{Install, Status},
};
use crate::docs::read::{Stamp, stamp};

/// How often Codex's day folders are listed again.
const LIST: Duration = Duration::from_secs(10);

/// A temp file the wrapper left this long ago goes.
const LEFT: Duration = Duration::from_secs(60);

/// Where each source lives.
#[derive(Clone, Debug)]
pub struct Paths {
    /// Claude's settings and the wrapper's folder in the app's config folder.
    pub install: Install,
    /// Codex's folder; none where no home is named.
    pub codex: Option<PathBuf>,
}

impl Paths {
    pub fn claude_data(&self) -> PathBuf {
        self.install.usage.join("claude-rate.json")
    }
}

/// Claude's limits and the data file's time, Unix seconds.
#[derive(Clone, Debug, PartialEq)]
pub struct Claude {
    pub limits: Limits,
    pub at: i64,
}

/// What a tick found that changed; `None` leaves a source as it was.
#[derive(Debug, Default)]
pub struct Changes {
    /// `Ok(None)`: no data file.
    pub claude: Option<Result<Option<Claude>, String>>,
    /// `Ok(None)`: no session file.
    pub codex: Option<Result<Option<codex::Reading>, String>>,
    pub install: Option<Status>,
}

/// The stamps last read, kept between ticks.
#[derive(Debug, Default)]
pub struct Poll {
    claude: Option<Option<Stamp>>,
    codex_file: Option<Option<PathBuf>>,
    codex: Option<Option<Stamp>>,
    listed: Option<Instant>,
    /// Claude's settings' and the app's state's stamps, as the status was last read.
    install: Option<[Option<Stamp>; 2]>,
}

impl Poll {
    /// Looks at each source, reading what changed since the last tick.
    pub fn tick(&mut self, paths: &Paths, codex_on: bool, now: Instant) -> Changes {
        Changes {
            install: self.install(&paths.install),
            claude: self.claude(paths),
            codex: if codex_on {
                self.codex(paths, now)
            } else {
                self.codex_file = None;
                self.codex = None;
                self.listed = None;
                None
            },
        }
    }

    /// The wrapper's status, read again when either file changed.
    fn install(&mut self, install: &Install) -> Option<Status> {
        let look = |path: Option<PathBuf>| path.and_then(|path| stamp(&path).ok().flatten());
        let stamps = [look(install.settings.clone()), look(Some(install.state()))];
        if self.install == Some(stamps) {
            return None;
        }
        self.install = Some(stamps);
        Some(install.status())
    }

    fn claude(&mut self, paths: &Paths) -> Option<Result<Option<Claude>, String>> {
        sweep(&paths.install.usage);
        let path = paths.claude_data();
        let found = match stamp(&path) {
            Ok(found) => found,
            Err(error) => return Some(Err(format!("{}: {error}", path.display()))),
        };
        if self.claude == Some(found) {
            return None;
        }
        self.claude = Some(found);
        let Some(found) = found else {
            return Some(Ok(None));
        };
        let at = found
            .mtime
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |since| since.as_secs() as i64);
        let read = std::fs::read_to_string(&path)
            .map_err(|error| format!("{}: {error}", path.display()))
            .and_then(|text| claude::parse(&text).map_err(|error| format!("{error:#}")))
            .and_then(|limits| {
                limits.ok_or_else(|| format!("{} reports no rate_limits", path.display()))
            });
        Some(read.map(|limits| Some(Claude { limits, at })))
    }

    fn codex(
        &mut self,
        paths: &Paths,
        now: Instant,
    ) -> Option<Result<Option<codex::Reading>, String>> {
        let Some(home) = &paths.codex else {
            return (self.codex_file != Some(None)).then(|| {
                self.codex_file = Some(None);
                Ok(None)
            });
        };
        if self.listed.is_none_or(|at| now.duration_since(at) >= LIST) {
            self.listed = Some(now);
            let sessions = home.join("sessions");
            let file = match codex::newest(&sessions) {
                Ok(file) => file,
                Err(error) => return Some(Err(format!("{}: {error}", sessions.display()))),
            };
            if self.codex_file.as_ref() != Some(&file) {
                self.codex_file = Some(file.clone());
                self.codex = None;
                if file.is_none() {
                    return Some(Ok(None));
                }
            }
        }
        let path = self.codex_file.clone().flatten()?;
        let found = match stamp(&path) {
            Ok(found) => found,
            Err(error) => return Some(Err(format!("{}: {error}", path.display()))),
        };
        if self.codex == Some(found) {
            return None;
        }
        self.codex = Some(found);
        if found.is_none() {
            self.listed = None;
            return None;
        }
        Some(
            codex::read(&path)
                .map(Some)
                .map_err(|error| format!("{error:#}")),
        )
    }
}

/// Removes the temp files a wrapper killed mid-write left behind.
fn sweep(folder: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let left = name.to_string_lossy().starts_with("claude-rate.json.")
            && entry
                .metadata()
                .and_then(|meta| meta.modified())
                .is_ok_and(|time| time.elapsed().is_ok_and(|age| age >= LEFT));
        if left {
            match std::fs::remove_file(entry.path()) {
                Ok(()) => log::info!("usage: {} was left behind; it goes", entry.path().display()),
                Err(error) => log::warn!("usage: {} stays: {error}", entry.path().display()),
            }
        }
    }
}
