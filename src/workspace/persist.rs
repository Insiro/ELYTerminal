//! `workspace.json` kept: a snapshot 0.5 s after a change and at close, restored at start, each home's tabs woken as it is selected.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

use ely_gpui_component::{
    buttons::{Button, ButtonVariant},
    feedback::EmptyState,
    i18n::text,
    primitives::IconName,
};
use gpui::{
    AnyElement, Context, IntoElement, ParentElement, Styled, Subscription, Task, Window, div,
};

pub(super) use shape::Dormant;
use shape::{parts, saved, state};

use super::{
    Workspace,
    projects::notice,
    tabs::{neighbor, strip},
};
use crate::{
    projects::{
        Home, TabId,
        claude::{Mode, Modes},
        store::{self, Loaded, SavedTab, State},
    },
    settings::SettingsStore,
    term::{Launch, config, shell::Start, spawn},
};

/// How long changes rest before they are saved, so a burst saves once.
const SETTLE: Duration = Duration::from_millis(500);

/// What the window opens with: the saved state, and a folder or program the start names.
pub struct Boot {
    pub path: PathBuf,
    pub loaded: Loaded,
    /// The file's time as loaded, to notice another writer.
    pub mtime: Option<SystemTime>,
    /// The folder the command line names, which opens a tab in its home.
    pub folder: Option<PathBuf>,
    /// A program to run in a tab of its folder's home, as `shot` asks.
    pub launch: Option<Launch>,
    /// How each saved Claude tab starts, found off the main thread.
    pub modes: Modes,
}

impl Boot {
    /// Reads `workspace.json` before any window; a file that cannot be read stops the start.
    pub fn load(folder: Option<PathBuf>) -> anyhow::Result<Self> {
        let path = store::default_path()?;
        let loaded = store::load(&path)?;
        let mtime = modified(&path);
        let modes = match &loaded {
            Loaded::Fresh(state) | Loaded::Ok(state) => Modes::find(claude_ids(state)),
            Loaded::Corrupt { .. } => Modes::default(),
        };
        Ok(Self {
            path,
            loaded,
            mtime,
            folder,
            launch: None,
            modes,
        })
    }
}

/// The last save written and the file's time after it.
#[derive(Debug, Default)]
struct Disk {
    written: u64,
    mtime: Option<SystemTime>,
    /// What the last save wrote, so an unchanged state writes nothing.
    last: Option<State>,
}

/// Writes saves in their order: one older than the last written is dropped.
#[derive(Clone, Debug)]
pub(super) struct Writer {
    path: PathBuf,
    disk: Arc<Mutex<Disk>>,
}

impl Writer {
    pub fn new(path: PathBuf, mtime: Option<SystemTime>) -> Self {
        let disk = Disk {
            written: 0,
            mtime,
            last: None,
        };
        Self {
            path,
            disk: Arc::new(Mutex::new(disk)),
        }
    }

    /// Writes `state` as save `number`; false when a later save is written already, or it holds the same.
    pub fn write(&self, number: u64, state: &State) -> anyhow::Result<bool> {
        let mut disk = self
            .disk
            .lock()
            .map_err(|_| anyhow::anyhow!("workspace: a save panicked before"))?;
        if number <= disk.written || disk.last.as_ref() == Some(state) {
            return Ok(false);
        }
        let now = modified(&self.path);
        if now != disk.mtime {
            log::warn!(
                "workspace: {} changed since this app last read or wrote it; another instance may run, and this save wins",
                self.path.display()
            );
        }
        store::save(&self.path, state)?;
        disk.written = number;
        disk.mtime = modified(&self.path);
        disk.last = Some(state.clone());
        Ok(true)
    }
}

/// Every Claude id `state` keeps.
fn claude_ids(state: &State) -> Vec<String> {
    let trees = state.projects.iter().flat_map(|project| &project.spaces);
    let tabs = trees.flat_map(|tree| &tree.tabs).chain(&state.others.tabs);
    tabs.filter_map(|SavedTab::Terminal { claude, .. }| claude.clone())
        .collect()
}

fn modified(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|meta| meta.modified()).ok()
}

/// The saving a restored workspace does.
pub(super) struct Persist {
    writer: Writer,
    number: u64,
    saving: Option<Task<()>>,
    /// The window began closing or the app quitting, and the last save is written: programs ending now change nothing kept.
    closed: bool,
    /// The tab the command line opened and its folder, homed again once every project's worktrees list.
    pending: Option<(TabId, PathBuf)>,
    _hooks: [Subscription; 2],
}

impl Workspace {
    /// Restores `boot`'s projects and tabs, wakes the selected home, opens the folder or program it names, and opens one tab when none is kept.
    pub fn restore(
        &mut self,
        boot: Boot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        self.modes = boot.modes;
        let (state, corrupt) = match boot.loaded {
            Loaded::Fresh(state) | Loaded::Ok(state) => (state, None),
            Loaded::Corrupt { backup } => (State::empty(), Some(backup)),
        };
        let (projects, dormant, selected) = parts(state, &mut self.next_project);
        log::info!(
            "workspace: {} projects and {} homes of saved tabs restored",
            projects.len(),
            dormant.len()
        );
        self.projects = projects;
        self.dormant = dormant;
        let mut pending = None;
        if let Some(launch) = boot.launch {
            let home = self.home_for(&launch.cwd);
            self.select_home(home.clone(), window, cx);
            let (pty, events) = spawn(&launch, (80, 24), (8, 16), config(SettingsStore::get(cx)))?;
            let start = Some(launch.cwd.clone());
            let id = self.add(pty, events, launch, (Vec::new(), home, start), window, cx);
            self.show(id, window, cx);
        } else if let Some(folder) = boot.folder {
            let home = self.home_for(&folder);
            log::info!("workspace: {} opens in {home:?}", folder.display());
            self.select_home(home.clone(), window, cx);
            let shell = SettingsStore::get(cx).shell.clone();
            let start = Start::In(folder.clone());
            self.open((&shell, Vec::new()), start, (home, None), None, window, cx)?;
            pending = self.current_id().map(|id| (id, folder));
        } else {
            self.select_home(selected, window, cx);
            if self.tabs.is_empty() && self.dormant.is_empty() {
                self.new_tab(window, cx)?;
            }
        }
        let weak = cx.entity().downgrade();
        window.on_window_should_close(cx, move |_, cx| {
            if let Some(this) = weak.upgrade() {
                this.update(cx, |this, _| this.flush());
            }
            true
        });
        let released = cx.on_release(|this, _| this.flush());
        let quit = cx.on_app_quit(|this, _| {
            this.flush();
            async {}
        });
        self.persist = Some(Persist {
            writer: Writer::new(boot.path, boot.mtime),
            number: 0,
            saving: None,
            closed: false,
            pending,
            _hooks: [released, quit],
        });
        self.dirty(cx);
        if let Some(backup) = corrupt {
            let body = text(
                cx,
                "et.store.corrupt_body",
                &[("path", &backup.display().to_string())],
            );
            notice("et.store.corrupt", anyhow::anyhow!("{body}"), window, cx);
        }
        self.refresh_all(cx);
        Ok(())
    }

    /// What `workspace.json` would hold now.
    fn snapshot(&self) -> State {
        let live = self
            .tabs
            .iter()
            .map(|tab| {
                let shown = self.shown.get(&tab.home) == Some(&tab.id);
                (tab.home.clone(), saved(tab.session()), shown)
            })
            .collect();
        state(&self.projects, live, &self.dormant, &self.selected)
    }

    /// Saves once changes rest, off the main thread.
    pub(super) fn dirty(&mut self, cx: &mut Context<Self>) {
        let Some(persist) = self.persist.as_mut().filter(|persist| !persist.closed) else {
            return;
        };
        persist.saving = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SETTLE).await;
            let next = this.update(cx, |this, _| {
                let state = this.snapshot();
                let persist = this.persist.as_mut()?;
                persist.number += 1;
                Some((persist.writer.clone(), persist.number, state))
            });
            let Ok(Some((writer, number, state))) = next else {
                return;
            };
            let written = cx
                .background_executor()
                .spawn(async move { writer.write(number, &state) })
                .await;
            match written {
                Ok(true) => {}
                Ok(false) => log::debug!("workspace: save {number} passed by a later one"),
                Err(error) => log::error!("{error:#}"),
            }
        }));
    }

    /// Saves now, on this thread, once, as the window closes or the app quits; later changes are the programs ending.
    fn flush(&mut self) {
        let state = self.snapshot();
        let Some(persist) = self.persist.as_mut().filter(|persist| !persist.closed) else {
            return;
        };
        persist.closed = true;
        persist.saving = None;
        persist.number += 1;
        if let Err(error) = persist.writer.write(persist.number, &state) {
            log::error!("{error:#}");
        }
    }

    /// Starts `home`'s saved tabs in their order, unless its folder is gone; the tab it showed, to show. Failures are told at once.
    pub(super) fn wake(
        &mut self,
        home: &Home,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<TabId> {
        let asleep = self.dormant.remove(home)?;
        if let Home::Tree { path, .. } = home
            && !path.is_dir()
        {
            log::warn!("workspace: {} is missing; its tabs wait", path.display());
            self.dormant.insert(home.clone(), asleep);
            return None;
        }
        log::info!("workspace: {} tabs of {home:?} wake", asleep.tabs.len());
        let (mut woken, mut failed, mut kept) = (Vec::new(), Vec::new(), Vec::new());
        for (at, tab) in asleep.tabs.into_iter().enumerate() {
            let SavedTab::Terminal {
                name,
                shell,
                args,
                folder,
                claude,
            } = tab.clone();
            let start = folder.clone().map_or(Start::Home, Start::In);
            let place = (home.clone(), None);
            // A WSL tab's transcripts lie inside its distribution, out of sight.
            let wsl = Path::new(&shell)
                .file_stem()
                .is_some_and(|stem| stem.eq_ignore_ascii_case("wsl"));
            let claude = claude.map(|id| {
                let mode = if wsl {
                    Mode::Resume
                } else {
                    self.modes.of(&id)
                };
                (id, mode)
            });
            let shown = folder
                .as_deref()
                .map_or_else(|| "~".into(), |at| at.display().to_string());
            match self.begin((&shell, args), start, place, claude, window, cx) {
                Ok(id) => {
                    let tab = self.index(id).map(|at| &mut self.tabs[at]);
                    if let Some(tab) = tab {
                        let session = tab.session_mut();
                        session.name = name;
                        session
                            .terminal
                            .update(cx, |view, _| view.set_front(false, window));
                    }
                    woken.push((at, id));
                }
                Err(error) => {
                    log::error!("workspace: a tab in {shown} did not start: {error:#}");
                    failed.push(format!("{shown}: {error:#}"));
                    kept.push(tab);
                }
            }
        }
        // A tab that did not start waits until its home is selected again or forgotten.
        if !kept.is_empty() {
            let waiting = Dormant {
                shown: 0,
                tabs: kept,
                failed: true,
            };
            self.dormant.insert(home.clone(), waiting);
        }
        if !failed.is_empty() {
            notice(
                "et.store.wake_failed",
                anyhow::anyhow!(failed.join("\n")),
                window,
                cx,
            );
        }
        self.dirty(cx);
        let shown = woken.iter().find(|(at, _)| *at == asleep.shown);
        shown.or(woken.first()).map(|(_, id)| *id)
    }

    /// Drops the selected home's tabs that wait for its folder.
    fn forget(&mut self, cx: &mut Context<Self>) {
        if let Some(asleep) = self.dormant.remove(&self.selected) {
            log::info!(
                "workspace: {} waiting tabs of {:?} forgotten",
                asleep.tabs.len(),
                self.selected
            );
            self.dirty(cx);
            cx.notify();
        }
    }

    /// Homes the command line's tab again once no listing runs, should its folder belong elsewhere than the cache said.
    pub(super) fn rehome(&mut self, cx: &mut Context<Self>) {
        if self.projects.iter().any(|project| project.listing.flight) {
            return;
        }
        let Some((id, folder)) = self.persist.as_mut().and_then(|p| p.pending.take()) else {
            return;
        };
        let home = self.home_for(&folder);
        let Some(at) = self.index(id) else {
            return;
        };
        let old = self.tabs[at].home.clone();
        if old == home {
            return;
        }
        log::info!("workspace: tab {} moves from {old:?} to {home:?}", id.0);
        let left: Vec<TabId> = strip(self.tabs.iter().map(|tab| &tab.home), &old)
            .into_iter()
            .map(|at| self.tabs[at].id)
            .collect();
        if self.shown.get(&old) == Some(&id) {
            match neighbor(&left, id) {
                Some(next) => self.shown.insert(old.clone(), next),
                None => self.shown.remove(&old),
            };
        }
        if self.selected == old {
            self.selected = home.clone();
        }
        self.tabs[at].home = home.clone();
        self.shown.insert(home, id);
        self.dirty(cx);
        cx.notify();
    }

    /// What shows while the selected home's tabs wait for its missing folder, or failed to start.
    pub(super) fn asleep(&self, cx: &Context<Self>) -> AnyElement {
        let path = match &self.selected {
            Home::Tree { path, .. } => path.display().to_string(),
            Home::Others => String::new(),
        };
        let failed = self
            .dormant
            .get(&self.selected)
            .is_some_and(|asleep| asleep.failed);
        let (title, hint) = if failed {
            ("et.empty.failed", "et.empty.failed_hint")
        } else {
            ("et.empty.missing", "et.empty.missing_hint")
        };
        let (workspace, retried) = (cx.entity(), cx.entity());
        let forget = Button::new("forget-tabs", text(cx, "et.empty.forget", &[]))
            .icon(IconName::Trash2)
            .variant(ButtonVariant::Ghost)
            .on_click(move |_, _, cx| workspace.update(cx, |this, cx| this.forget(cx)));
        let retry = Button::new("retry-tabs", text(cx, "et.empty.retry", &[]))
            .icon(IconName::RefreshCw)
            .on_click(move |_, window, cx| {
                retried.update(cx, |this, cx| {
                    this.select_home(this.selected.clone(), window, cx)
                })
            });
        let actions = div().flex().gap_2().child(retry).child(forget);
        let title = text(cx, title, &[]);
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                EmptyState::new("folder-missing", IconName::TriangleAlert, title)
                    .body(text(cx, hint, &[("path", &path)]))
                    .action(actions),
            )
            .into_any_element()
    }
}

mod shape;
#[cfg(test)]
mod tests;
