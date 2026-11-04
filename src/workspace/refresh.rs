//! Each project's worktrees, listed off the main thread; a listing that lands late is dropped.

use std::{
    path::Path,
    time::{Duration, Instant},
};

use gpui::Context;

use super::{
    Workspace,
    projects::{Tree, Trees},
};
use crate::{git, projects::normal};

/// The least time between listings an activation of the window starts.
const QUIET: Duration = Duration::from_secs(2);

/// What git says of `folder`'s worktrees, each folder made normal and checked.
fn list(folder: &Path) -> anyhow::Result<Trees> {
    Ok(match git::worktrees(folder)? {
        None => Trees::Plain,
        Some(found) => Trees::Git(
            found
                .into_iter()
                .map(|worktree| Tree {
                    missing: worktree.prunable || !worktree.path.is_dir(),
                    label: worktree.label(),
                    path: normal(&worktree.path),
                })
                .collect(),
        ),
    })
}

impl Workspace {
    /// Lists project `id`'s worktrees again; one listing at a time, another after it if asked meanwhile.
    pub(super) fn refresh(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(project) = self.projects.iter_mut().find(|project| project.id == id) else {
            return;
        };
        if project.listing.flight {
            project.listing.again = true;
            return;
        }
        project.listing.flight = true;
        let folder = project.folder.clone();
        log::debug!("app: listing the worktrees of {}", folder.display());
        cx.spawn(async move |this, cx| {
            let trees = cx
                .background_executor()
                .spawn(async move { list(&folder) })
                .await;
            if let Err(error) = this.update(cx, |this, cx| this.listed(id, trees, cx)) {
                log::debug!("app: worktrees listed for a closed window: {error:#}");
            }
        })
        .detach();
    }

    /// Lists the worktrees of the project whose folder is `folder`.
    pub(super) fn refresh_folder(&mut self, folder: &Path, cx: &mut Context<Self>) {
        let found = self
            .projects
            .iter()
            .find(|project| project.folder == folder);
        if let Some(id) = found.map(|project| project.id) {
            self.refresh(id, cx);
        }
    }

    /// Lists every project's worktrees, at most once in `QUIET`, as the window turns active.
    pub(super) fn refresh_all(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        if self
            .refreshed
            .is_some_and(|last| now.duration_since(last) < QUIET)
        {
            return;
        }
        self.refreshed = Some(now);
        let ids: Vec<u64> = self.projects.iter().map(|project| project.id).collect();
        for id in ids {
            self.refresh(id, cx);
        }
    }

    /// A listing landed; a removed project's is dropped, and a failure keeps the trees listed before.
    fn listed(&mut self, id: u64, trees: anyhow::Result<Trees>, cx: &mut Context<Self>) {
        let Some(project) = self.projects.iter_mut().find(|project| project.id == id) else {
            log::debug!("app: worktrees listed for a project removed");
            return;
        };
        project.listing.flight = false;
        let mut changed = false;
        let folder = project.folder.display().to_string();
        match trees {
            Err(error) => {
                if project.listing.error.is_none() {
                    log::warn!("app: cannot list the worktrees of {folder}: {error:#}");
                }
                project.listing.error = Some(format!("{error:#}"));
            }
            Ok(trees) => {
                if project.listing.error.take().is_some() {
                    log::info!("app: the worktrees of {folder} list again");
                }
                match (&project.trees, &trees) {
                    (Trees::Plain, Trees::Plain) => {}
                    (_, Trees::Plain) => log::info!("app: {folder} is a plain folder"),
                    (Trees::Git(before), Trees::Git(after)) if before.len() == after.len() => {}
                    (_, Trees::Git(after)) => {
                        log::info!("app: {folder} holds {} worktrees", after.len())
                    }
                    (_, Trees::Pending) => {}
                }
                if project.trees != trees {
                    project.trees = trees;
                    changed = true;
                }
            }
        }
        if std::mem::take(&mut project.listing.again) {
            self.refresh(id, cx);
        }
        if changed {
            self.dirty(cx);
        }
        cx.notify();
    }
}
