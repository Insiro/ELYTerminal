//! A linked worktree removed through git, its tabs, saved state and listing following.

use std::path::{Path, PathBuf};

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
            let dirty = cx
                .background_executor()
                .spawn(
                    async move { anyhow::Ok(checked.is_dir() && git::worktree_dirty(&checked)?) },
                )
                .await;
            let asked = this.update_in(cx, |this, window, cx| match dirty {
                Ok(dirty) => {
                    log::debug!("app: worktree {} dirty: {dirty}", path.display());
                    let removal = Removal {
                        project,
                        path,
                        branch,
                        dirty,
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
        let message: Vec<&str> = parts.iter().map(AsRef::as_ref).collect();
        let message = message.join(" ");
        let confirm = if removal.dirty {
            "et.worktree.remove_force"
        } else {
            "et.worktree.remove_confirm"
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

    /// Runs `git worktree remove` off the main thread, then closes the worktree's tabs and lists the project again; a failure shows git's words.
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
        let tree = path.clone();
        cx.spawn_in(window, async move |this, cx| {
            let removed = cx
                .background_executor()
                .spawn(async move { git::remove_worktree(&main, &tree, force) })
                .await;
            let done = this.update_in(cx, |this, window, cx| match removed {
                Ok(()) => this.worktree_gone(project, &path, window, cx),
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

    /// Worktree `path` is gone: its tabs close, its saved state goes, and the main worktree shows if it showed.
    fn worktree_gone(
        &mut self,
        project: u64,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(home) = self.tree_home(project, path) else {
            return;
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
            .map(|tab| tab.id)
            .collect();
        log::info!(
            "app: worktree {} removed, {} tabs close",
            path.display(),
            closing.len()
        );
        for id in closing {
            self.close(id, window, cx);
        }
        self.shown.remove(&home);
        self.dormant.remove(&home);
        self.panels.remove(&home);
        self.refresh(project, cx);
        self.dirty(cx);
        cx.notify();
    }
}
