//! A linked worktree removed through git, its tabs, saved state and listing following.

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use ely_gpui_component::{i18n::text, overlays::ConfirmDialog};
use gpui::{AnyElement, App, Context, IntoElement, Window};

use super::{Asking, Workspace, projects::notice};
use crate::{git, projects::Home};

/// A worktree the user asked to remove, as its dialog tells it.
pub(super) struct Removal {
    /// The project whose repository git removes it through.
    pub project: u64,
    pub path: PathBuf,
    pub branch: Option<String>,
    /// It holds changes or untracked files, so only `--force` removes it.
    pub dirty: bool,
    /// The ignored entries git deletes with it, force or not.
    pub ignored: Vec<String>,
    /// Its folder is gone already, so git only forgets it.
    pub missing: bool,
}

/// How long the closed tabs' programs get to end before git removes their folder.
const EXIT_WAIT: Duration = Duration::from_secs(3);

/// Whether process `pid` still runs.
#[cfg(unix)]
fn alive(pid: u32) -> bool {
    // Signal 0 checks without sending; EPERM means it runs as another user.
    let sent = unsafe { libc::kill(pid as libc::pid_t, 0) } == 0;
    sent || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Whether process `pid` still runs.
#[cfg(windows)]
fn alive(pid: u32) -> bool {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_TIMEOUT},
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        return false;
    }
    let waiting = unsafe { WaitForSingleObject(handle, 0) } == WAIT_TIMEOUT;
    unsafe { CloseHandle(handle) };
    waiting
}

/// Waits up to `limit` for `pids` to end; the ones still running.
fn wait_gone(mut pids: Vec<u32>, limit: Duration) -> Vec<u32> {
    let deadline = Instant::now() + limit;
    loop {
        pids.retain(|pid| alive(*pid));
        if pids.is_empty() || Instant::now() >= deadline {
            return pids;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

impl Workspace {
    /// The tabs of project `project`'s worktree `path`.
    fn tree_home(&self, project: u64, path: &Path) -> Option<Home> {
        let project = self.project(project)?;
        Some(Home::Tree {
            project: project.folder.clone(),
            path: path.to_path_buf(),
        })
    }

    /// Checks whether worktree `path` is dirty, off the main thread, then asks before removing it.
    pub(super) fn ask_remove_worktree(
        &mut self,
        (project, path): (u64, PathBuf),
        branch: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        log::info!("app: checking worktree {} before removal", path.display());
        let checked = path.clone();
        cx.spawn_in(window, async move |this, cx| {
            let loss = cx
                .background_executor()
                .spawn(async move {
                    match checked.is_dir() {
                        true => git::worktree_loss(&checked).map(|loss| (loss, false)),
                        false => Ok((git::Loss::default(), true)),
                    }
                })
                .await;
            let asked = this.update_in(cx, |this, window, cx| match loss {
                Ok((loss, missing)) => {
                    log::debug!(
                        "app: worktree {} dirty: {}, {} ignored, missing: {missing}",
                        path.display(),
                        loss.dirty,
                        loss.ignored.len()
                    );
                    let removal = Removal {
                        project,
                        path,
                        branch,
                        dirty: loss.dirty,
                        ignored: loss.ignored,
                        missing,
                    };
                    this.asking = Some(Asking::RemoveWorktree(removal));
                    cx.notify();
                }
                Err(error) => notice("et.worktree.check_failed", error, window, cx),
            });
            if let Err(error) = asked {
                log::debug!("app: worktree check for a closed window: {error:#}");
            }
        })
        .detach();
    }

    /// The question before `removal`: its path and branch, the tabs that close, and the changes lost.
    pub(super) fn removal_dialog(
        &self,
        removal: &Removal,
        closed: impl Fn(&mut Window, &mut App) + 'static,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let home = self.tree_home(removal.project, &removal.path)?;
        let path = removal.path.display().to_string();
        let name = super::projects::folder_name(&removal.path);
        let title = text(cx, "et.worktree.remove_title", &[("name", &name)]);
        let mut parts = vec![match &removal.branch {
            _ if removal.missing => text(cx, "et.worktree.remove_missing", &[("path", &path)]),
            Some(branch) => text(
                cx,
                "et.worktree.remove_body",
                &[("path", &path), ("branch", branch)],
            ),
            None => text(cx, "et.worktree.remove_detached", &[("path", &path)]),
        }];
        let open: Vec<_> = self.tabs.iter().filter(|tab| tab.home == home).collect();
        if !open.is_empty() {
            let count = open.len().to_string();
            parts.push(text(cx, "et.worktree.remove_tabs", &[("count", &count)]));
            let claude = open
                .iter()
                .filter(|tab| tab.term().is_some_and(|s| s.claude.is_some()))
                .count();
            if claude > 0 {
                let count = claude.to_string();
                parts.push(text(cx, "et.projects.remove_claude", &[("count", &count)]));
            }
        }
        if removal.dirty {
            parts.push(text(cx, "et.worktree.remove_dirty", &[]));
        }
        if !removal.ignored.is_empty() {
            let count = removal.ignored.len().to_string();
            let mut names: Vec<&str> = removal.ignored.iter().take(3).map(AsRef::as_ref).collect();
            if removal.ignored.len() > names.len() {
                names.push("…");
            }
            let names = names.join(", ");
            parts.push(text(
                cx,
                "et.worktree.remove_ignored",
                &[("count", &count), ("names", &names)],
            ));
        }
        let message: Vec<&str> = parts.iter().map(AsRef::as_ref).collect();
        let message = message.join(" ");
        let confirm = match (removal.missing, removal.dirty) {
            (true, _) => "et.worktree.remove_forget",
            (false, true) => "et.worktree.remove_force",
            (false, false) => "et.worktree.remove_confirm",
        };
        let (submit, project, tree, force) = (
            cx.entity(),
            removal.project,
            removal.path.clone(),
            removal.dirty,
        );
        Some(
            ConfirmDialog::new("remove-worktree", title, message, closed)
                .confirm(text(cx, confirm, &[]))
                .destructive()
                .on_confirm(move |window, cx| {
                    submit.update(cx, |this, cx| {
                        this.remove_worktree((project, tree.clone()), force, window, cx)
                    })
                })
                .into_any_element(),
        )
    }

    /// Closes the worktree's tabs, waits a while for their programs to end, then runs `git worktree remove` off the main thread; a failure shows git's words.
    fn remove_worktree(
        &mut self,
        (project, path): (u64, PathBuf),
        force: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(main) = self.project(project).map(|p| p.folder.clone()) else {
            log::debug!("app: the worktree's project is gone");
            return;
        };
        log::info!(
            "app: removing worktree {} of {}, force {force}",
            path.display(),
            main.display()
        );
        let pids = self.close_tree_tabs(project, &path, window, cx);
        let tree = path.clone();
        cx.spawn_in(window, async move |this, cx| {
            let removed = cx
                .background_executor()
                .spawn(async move {
                    let left = wait_gone(pids, EXIT_WAIT);
                    if !left.is_empty() {
                        log::warn!(
                            "app: programs {left:?} still run; git removes the worktree anyway"
                        );
                    }
                    git::remove_worktree(&main, &tree, force)
                })
                .await;
            let done = this.update_in(cx, |this, window, cx| match removed {
                Ok(()) => this.worktree_gone(project, &path, cx),
                Err(error) => {
                    this.refresh(project, cx);
                    notice("et.worktree.remove_failed", error, window, cx)
                }
            });
            if let Err(error) = done {
                log::debug!("app: worktree removed for a closed window: {error:#}");
            }
        })
        .detach();
    }

    /// Closes worktree `path`'s tabs, the main worktree shown if it showed; the closed terminals' program ids.
    fn close_tree_tabs(
        &mut self,
        project: u64,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<u32> {
        let Some(home) = self.tree_home(project, path) else {
            return Vec::new();
        };
        if self.selected == home
            && let Some(main) = self.project(project).map(|p| p.home())
        {
            self.select_home(main, window, cx);
        }
        let closing: Vec<_> = self
            .tabs
            .iter()
            .filter(|tab| tab.home == home)
            .map(|tab| (tab.id, tab.term().map(|s| s.terminal.read(cx).pid())))
            .collect();
        log::info!(
            "app: worktree {} goes, {} tabs close first",
            path.display(),
            closing.len()
        );
        let mut pids = Vec::new();
        for (id, pid) in closing {
            pids.extend(pid);
            self.close(id, window, cx);
        }
        pids
    }

    /// Worktree `path` is gone: its saved state goes, and the project lists again.
    fn worktree_gone(&mut self, project: u64, path: &Path, cx: &mut Context<Self>) {
        let Some(home) = self.tree_home(project, path) else {
            return;
        };
        log::info!("app: worktree {} removed", path.display());
        self.forget_panels(|kept| *kept == home, vec![path.to_path_buf()], cx);
        self.shown.remove(&home);
        self.dormant.remove(&home);
        self.drop_parked(|parked| *parked == home);
        self.refresh(project, cx);
        self.dirty(cx);
        cx.notify();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn a_running_program_is_named_and_an_ended_one_is_not() {
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("sleep runs");
        let pid = child.id();
        assert_eq!(wait_gone(vec![pid], Duration::from_millis(120)), vec![pid]);
        child.kill().expect("a kill");
        child.wait().expect("a reap");
        assert!(wait_gone(vec![pid], Duration::from_secs(1)).is_empty());
    }
}
