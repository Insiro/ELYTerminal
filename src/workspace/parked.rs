//! Tabs saved for later: a terminal closed with where it ran and what it showed, resumed in its home.

use std::path::{Path, PathBuf};

use gpui::{Context, Window};

use super::{Asking, Workspace, projects::notice};
use crate::{
    projects::{Home, TabId, claude::Mode, store::Parked},
    sessions::Session,
    term::shell::Start,
    usage::time,
};

mod view;

/// The most tabs kept for later; past it the oldest go.
pub(super) const KEPT: usize = 50;

/// Drops the oldest of `parked` past `KEPT`; how many went.
pub(super) fn cap(parked: &mut Vec<Parked>) -> usize {
    let mut dropped = 0;
    while parked.len() > KEPT {
        let oldest = (0..parked.len())
            .min_by_key(|at| parked[*at].saved)
            .expect("a saved tab past the cap");
        let gone = parked.remove(oldest);
        log::info!("app: saved tab {:?} goes, past {KEPT} kept", gone.name);
        dropped += 1;
    }
    dropped
}

/// `path` when `workspace.json` can keep it: absolute and UTF-8.
fn keepable(path: &Path) -> Option<PathBuf> {
    (path.is_absolute() && path.to_str().is_some()).then(|| path.to_path_buf())
}

/// What `session` of `home` keeps as it is saved for later at `saved`.
pub(super) fn record(home: Home, session: &Session, screen: String, saved: u64) -> Parked {
    let folder = keepable(&session.folder).or_else(|| session.start.as_deref().and_then(keepable));
    Parked {
        home,
        name: session.named().to_string(),
        renamed: !session.name.is_empty(),
        folder,
        shell: session.shell.clone(),
        args: session.args.clone(),
        claude: session.claude.clone(),
        saved,
        screen,
    }
}

/// Where a resume starts and how: its folder, else its home's when `is_dir` finds it gone, which then says so; none is home. Claude resumes by its id only in its own folder, since Claude ties a conversation to it.
#[derive(Debug, PartialEq)]
pub(super) struct Resume {
    pub folder: Option<PathBuf>,
    pub moved: bool,
    pub claude: Option<(String, Mode)>,
}

pub(super) fn resume(parked: &Parked, is_dir: impl Fn(&Path) -> bool) -> Resume {
    let home = match &parked.home {
        Home::Tree { path, .. } if is_dir(path) => Some(path.clone()),
        _ => None,
    };
    let (folder, moved) = match &parked.folder {
        Some(folder) if is_dir(folder) => (Some(folder.clone()), false),
        Some(_) => (home, true),
        None => (None, false),
    };
    let claude = (parked.claude.clone())
        .filter(|_| !moved)
        .map(|id| (id, Mode::Resume));
    Resume {
        folder,
        moved,
        claude,
    }
}

impl Workspace {
    /// Saves tab `id` for later, asking first while its shell runs a command other than Claude.
    pub fn ask_park(&mut self, id: TabId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.index(id).and_then(|at| self.tabs[at].term()) else {
            return;
        };
        if session.claude.is_none() && session.terminal.read(cx).busy() {
            log::debug!("app: tab {} runs a command, so saving asks", id.0);
            self.asking = Some(Asking::Park { id });
            cx.notify();
        } else {
            self.park(id, window, cx);
        }
    }

    /// Keeps tab `id`'s record among the saved, then closes it.
    pub(super) fn park(&mut self, id: TabId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(at) = self.index(id) else {
            return;
        };
        let home = self.tabs[at].home.clone();
        let Some(session) = self.tabs[at].term() else {
            return;
        };
        let screen = session.terminal.read(cx).screen_text();
        let parked = record(home, session, screen, time::now().max(0) as u64);
        log::info!(
            "app: tab {} saved for later as {:?}, {} bytes of screen",
            id.0,
            parked.name,
            parked.screen.len()
        );
        self.parked.push(parked);
        cap(&mut self.parked);
        self.close(id, window, cx);
        self.dirty(cx);
    }

    /// Opens `parked` again as a tab of its home, and drops it from the saved; a failure keeps it.
    pub(super) fn unpark(&mut self, parked: &Parked, window: &mut Window, cx: &mut Context<Self>) {
        let Some(at) = self.parked.iter().position(|kept| kept == parked) else {
            log::debug!("app: the saved tab to resume is gone");
            return;
        };
        let plan = resume(parked, Path::is_dir);
        let start = plan.folder.clone().map_or(Start::Home, Start::In);
        let place = (parked.home.clone(), None);
        let program = (parked.shell.as_str(), parked.args.clone());
        log::info!("app: resuming {:?} in {:?}", parked.name, plan.folder);
        match self.open(program, start, place, plan.claude, window, cx) {
            Ok(()) => {
                self.parked.remove(at);
                let id = self.current_id();
                let shown = id.and_then(|id| self.index(id));
                if let Some(session) = shown.and_then(|at| self.tabs[at].term_mut())
                    && parked.renamed
                {
                    session.name = parked.name.clone();
                }
                self.dirty(cx);
                cx.notify();
                if plan.moved {
                    let gone = parked.folder.as_deref().unwrap_or(Path::new(""));
                    let gone = gone.display();
                    match &parked.claude {
                        Some(id) => {
                            let error = anyhow::anyhow!("{gone} is gone; Claude conversation {id}");
                            notice("et.parked.claude_folder_gone", error, window, cx)
                        }
                        None => {
                            let error = anyhow::anyhow!("{gone} is gone");
                            notice("et.parked.folder_gone", error, window, cx)
                        }
                    }
                }
            }
            Err(error) => notice("et.parked.resume_failed", error, window, cx),
        }
    }

    /// Drops `parked` from the saved.
    pub(super) fn forget_parked(&mut self, parked: &Parked, cx: &mut Context<Self>) {
        let before = self.parked.len();
        self.parked.retain(|kept| kept != parked);
        if self.parked.len() != before {
            log::info!("app: saved tab {:?} deleted", parked.name);
            self.dirty(cx);
            cx.notify();
        }
    }

    /// Drops the saved tabs of every home `gone` says left.
    pub(super) fn drop_parked(&mut self, gone: impl Fn(&Home) -> bool) {
        let before = self.parked.len();
        self.parked.retain(|parked| !gone(&parked.home));
        let dropped = before - self.parked.len();
        if dropped > 0 {
            log::info!("app: {dropped} saved tabs left with their home");
        }
    }
}

#[cfg(test)]
mod tests;
