//! The projects the left column lists, and what adding, renaming, removing and moving them does.

use std::path::{Path, PathBuf};

use ely_gpui_component::i18n::text;
use gpui::{App, Context, PathPromptOptions, PromptLevel, Window};

use super::{Asking, Workspace, folder};
use crate::projects::{Home, TabId, home_of, normal};

/// A folder the user added, and the worktrees git lists in it.
pub(super) struct Project {
    /// Its row's key, kept while it lives.
    pub id: u64,
    /// Its folder, as `normal` gives it.
    pub folder: PathBuf,
    /// The name the user gave it; none shows the folder's.
    pub name: Option<String>,
    /// Its linked worktrees show.
    pub open: bool,
    pub trees: Trees,
    pub listing: Listing,
}

/// What git said of a project's folder.
#[derive(Debug, PartialEq)]
pub(super) enum Trees {
    /// Not listed yet.
    Pending,
    /// A plain folder, or a repository's subfolder.
    Plain,
    /// The worktrees git lists.
    Git(Vec<Tree>),
}

/// A worktree as a row shows it.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Tree {
    /// Its folder, as `normal` gives it.
    pub path: PathBuf,
    /// Its branch, else its head.
    pub label: String,
    /// What git listed, which `workspace.json` caches.
    pub branch: Option<String>,
    pub head: Option<String>,
    /// Git calls it prunable, or its folder is gone.
    pub missing: bool,
}

/// The listing under way, another asked meanwhile, and the last one's failure.
#[derive(Default)]
pub(super) struct Listing {
    pub flight: bool,
    pub again: bool,
    /// The last listing failed so; the trees before it stay.
    pub error: Option<String>,
}

impl Project {
    /// What its row says: its name, else its folder's.
    pub fn label(&self) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| folder_name(&self.folder))
    }

    /// The home its own row selects: the main worktree, or the plain folder.
    pub fn home(&self) -> Home {
        Home::Tree {
            project: self.folder.clone(),
            path: self.folder.clone(),
        }
    }

    /// The branch of the worktree that is its own folder, while git listed one.
    pub fn branch(&self) -> Option<&str> {
        match &self.trees {
            Trees::Git(trees) => trees
                .iter()
                .find(|tree| tree.path == self.folder)
                .map(|tree| tree.label.as_str()),
            _ => None,
        }
    }

    /// Its worktrees' folders, for `home_of`.
    fn paths(&self) -> Vec<PathBuf> {
        match &self.trees {
            Trees::Git(trees) => trees.iter().map(|tree| tree.path.clone()).collect(),
            _ => Vec::new(),
        }
    }
}

/// The last part of `folder`, else all of it.
pub(super) fn folder_name(folder: &Path) -> String {
    folder.file_name().map_or_else(
        || folder.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// A row of the left column, by what its mark gathers.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Row {
    /// A project's own row: its main home, and while shut its worktrees' too.
    Project {
        folder: PathBuf,
        open: bool,
    },
    Tree(Home),
    Others,
}

impl Row {
    /// Tabs of `home` count toward this row's mark.
    pub fn covers(&self, home: &Home) -> bool {
        match (self, home) {
            (Row::Project { folder, open }, Home::Tree { project, path }) => {
                project == folder && (path == folder || !open)
            }
            (Row::Tree(row), home) => row == home,
            (Row::Others, Home::Others) => true,
            _ => false,
        }
    }
}

/// A project's rows under its own: each other worktree listed, the main one too when the project is a linked one, a missing one only while it holds tabs, then each home of a tab git no longer lists.
pub(super) fn children<'a>(
    project: &Project,
    homes: impl IntoIterator<Item = &'a Home>,
) -> Vec<Tree> {
    let held: Vec<&PathBuf> = homes
        .into_iter()
        .filter_map(|home| match home {
            Home::Tree { project: of, path }
                if *of == project.folder && *path != project.folder =>
            {
                Some(path)
            }
            _ => None,
        })
        .collect();
    let listed: &[Tree] = match &project.trees {
        Trees::Git(trees) => trees,
        _ => &[],
    };
    let mut rows: Vec<Tree> = listed
        .iter()
        .filter(|tree| tree.path != project.folder)
        .filter(|tree| !tree.missing || held.contains(&&tree.path))
        .cloned()
        .collect();
    for path in held {
        if !rows.iter().any(|tree| tree.path == *path) {
            rows.push(Tree {
                path: path.clone(),
                label: folder_name(path),
                branch: None,
                head: None,
                missing: true,
            });
        }
    }
    rows
}

/// The tabs that live in project `folder`'s homes.
pub(super) fn tabs_of<'a>(
    tabs: impl IntoIterator<Item = (TabId, &'a Home)>,
    folder: &Path,
) -> Vec<TabId> {
    tabs.into_iter()
        .filter(|(_, home)| matches!(home, Home::Tree { project, .. } if project == folder))
        .map(|(id, _)| id)
        .collect()
}

impl Workspace {
    pub(super) fn project(&self, id: u64) -> Option<&Project> {
        self.projects.iter().find(|project| project.id == id)
    }

    /// The home `folder` belongs to among the projects and their worktrees.
    pub(super) fn home_for(&self, folder: &Path) -> Home {
        let projects: Vec<(PathBuf, Vec<PathBuf>)> = self
            .projects
            .iter()
            .map(|project| (project.folder.clone(), project.paths()))
            .collect();
        home_of(&projects, &normal(folder))
    }

    /// What `home` is called: its project and its worktree's branch, as `name (branch)`; none for Others.
    pub(super) fn home_label(&self, home: &Home) -> Option<String> {
        let Home::Tree { project, path } = home else {
            return None;
        };
        let Some(owner) = self.projects.iter().find(|p| p.folder == *project) else {
            return Some(folder_name(path));
        };
        let branch = if path == project {
            owner.branch().map(str::to_string)
        } else {
            let tree = children(owner, [home])
                .into_iter()
                .find(|tree| tree.path == *path);
            Some(tree.map_or_else(|| folder_name(path), |tree| tree.label))
        };
        Some(match branch {
            Some(branch) => format!("{} ({branch})", owner.label()),
            None => owner.label(),
        })
    }

    /// Asks the system for a folder to add as a project.
    pub(super) fn choose_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        log::info!("app: choosing a project folder");
        let chosen = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(text(cx, "et.projects.choose", &[])),
        });
        cx.spawn_in(window, async move |workspace, cx| {
            let result = match chosen.await {
                Ok(Ok(paths)) => {
                    let checked = cx.background_executor();
                    checked.spawn(async move { folder::selected(paths) }).await
                }
                Ok(Err(error)) => Err(error.context("app: cannot choose a project folder")),
                Err(error) => Err(anyhow::Error::new(error).context("app: chooser disconnected")),
            };
            let added = workspace.update_in(cx, |this, window, cx| match result {
                Ok(Some(folder)) => this.add_project(&folder, window, cx),
                Ok(None) => log::info!("app: project choice cancelled"),
                Err(error) => notice("et.projects.add_failed", error, window, cx),
            });
            if let Err(error) = added {
                log::debug!("app: project choice owner closed: {error:#}");
            }
        })
        .detach();
    }

    /// Adds `folder` as a project and selects it; one already added is selected.
    pub(super) fn add_project(
        &mut self,
        folder: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let folder = normal(folder);
        let id = match self
            .projects
            .iter()
            .find(|project| project.folder == folder)
        {
            Some(project) => {
                log::info!("app: {} is a project already", folder.display());
                project.id
            }
            None => {
                let id = self.next_project;
                self.next_project += 1;
                log::info!("app: project {} added", folder.display());
                self.projects.push(Project {
                    id,
                    folder,
                    name: None,
                    open: true,
                    trees: Trees::Pending,
                    listing: Listing::default(),
                });
                self.refresh(id, cx);
                self.dirty(cx);
                id
            }
        };
        if let Some(home) = self.project(id).map(Project::home) {
            self.select_home(home, window, cx);
        }
    }

    /// Opens or shuts project `id`'s worktrees; opening lists them again.
    pub(super) fn toggle_project(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(project) = self.projects.iter_mut().find(|project| project.id == id) else {
            return;
        };
        project.open = !project.open;
        log::debug!(
            "app: project {} open: {}",
            project.folder.display(),
            project.open
        );
        if project.open {
            self.refresh(id, cx);
        }
        self.dirty(cx);
        cx.notify();
    }

    /// Moves project `from` to place `to`, as a drag asks.
    pub(super) fn reorder_projects(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        if from == to || from >= self.projects.len() || to >= self.projects.len() {
            return;
        }
        let project = self.projects.remove(from);
        log::info!("app: project {} moves to {to}", project.folder.display());
        self.projects.insert(to, project);
        self.dirty(cx);
        cx.notify();
    }

    /// Names project `id` `name`, trimmed; empty gives it its folder's name again.
    pub(super) fn name_project(
        &mut self,
        id: u64,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.projects.iter_mut().find(|project| project.id == id) else {
            log::debug!("app: the project renamed is gone");
            return;
        };
        let name = name.trim();
        project.name = (!name.is_empty()).then(|| name.to_string());
        log::info!(
            "app: project {} is named {:?}",
            project.folder.display(),
            project.name
        );
        self.dirty(cx);
        self.retitle(window, cx);
        cx.notify();
    }

    /// Asks before project `id` leaves the list.
    pub(super) fn ask_remove(&mut self, id: u64, cx: &mut Context<Self>) {
        if self.project(id).is_none() {
            log::debug!("app: the project to remove is gone");
            return;
        }
        self.asking = Some(Asking::RemoveProject { id });
        cx.notify();
    }

    /// Closes project `id`'s tabs and drops it from the list; nothing on disk changes.
    pub(super) fn remove_project(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(at) = self.projects.iter().position(|project| project.id == id) else {
            return;
        };
        let folder = self.projects[at].folder.clone();
        let inside = |home: &Home| matches!(home, Home::Tree { project, .. } if *project == folder);
        let asked = match &self.asking {
            Some(Asking::RenameProject { id: asked, .. } | Asking::RemoveProject { id: asked }) => {
                *asked == id
            }
            Some(Asking::RemoveWorktree(removal)) => removal.project == id,
            _ => false,
        };
        if asked {
            self.asking = None;
        }
        if inside(&self.selected) {
            self.select_home(Home::Others, window, cx);
        }
        let closing = tabs_of(self.tabs.iter().map(|tab| (tab.id, &tab.home)), &folder);
        log::info!(
            "app: project {} removed, {} tabs close",
            folder.display(),
            closing.len()
        );
        for tab in closing {
            self.close(tab, window, cx);
        }
        let mut roots = vec![folder.clone()];
        if let Trees::Git(trees) = &self.projects[at].trees {
            roots.extend(trees.iter().map(|tree| tree.path.clone()));
        }
        self.forget_panels(inside, roots, cx);
        self.projects.remove(at);
        self.shown.retain(|home, _| !inside(home));
        self.dormant.retain(|home, _| !inside(home));
        self.drop_parked(inside);
        self.dirty(cx);
        cx.notify();
    }
}

/// Tells the user what failed, under the title `key` names.
pub(super) fn notice(key: &str, error: anyhow::Error, window: &mut Window, cx: &mut App) {
    log::error!("app: {key}: {error:#}");
    let title = text(cx, key, &[]);
    let close = text(cx, "et.files.dismiss", &[]);
    let answer = window.prompt(
        PromptLevel::Critical,
        &title,
        Some(&format!("{error:#}")),
        &[close.as_ref()],
        cx,
    );
    cx.spawn(async move |_| {
        let _ = answer.await;
    })
    .detach();
}

#[cfg(test)]
mod tests;
