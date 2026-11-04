//! How the workspace's projects and tabs map to `workspace.json`'s state and back.

use std::{collections::HashMap, path::Path};

use crate::{
    projects::{
        Home, Worktree,
        store::{SavedProject, SavedSpace, SavedTab, SavedTree, State},
    },
    sessions::Session,
    workspace::projects::{Listing, Project, Tree, Trees, folder_name},
};

/// The saved tabs of a home not woken yet, and the one it showed.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::workspace) struct Dormant {
    pub shown: usize,
    pub tabs: Vec<SavedTab>,
    /// They failed to start, not waited for a missing folder; not saved.
    pub failed: bool,
}

/// The order homes save in: projects' by path, Others last.
fn order(home: &Home) -> (bool, Option<(&Path, &Path)>) {
    match home {
        Home::Tree { project, path } => (false, Some((project.as_path(), path.as_path()))),
        Home::Others => (true, None),
    }
}

/// What the file caches of a project's worktrees.
fn cache(trees: &Trees) -> Vec<Worktree> {
    match trees {
        Trees::Git(trees) => trees
            .iter()
            .map(|tree| Worktree {
                path: tree.path.clone(),
                branch: tree.branch.clone(),
                head: tree.head.clone(),
            })
            .collect(),
        Trees::Pending | Trees::Plain => Vec::new(),
    }
}

/// The worktrees a cache gives until git lists them, none missing, since the listing at start checks the disk; none cached is not listed yet.
fn cached(worktrees: Vec<Worktree>) -> Trees {
    if worktrees.is_empty() {
        return Trees::Pending;
    }
    Trees::Git(
        worktrees
            .into_iter()
            .map(|tree| Tree {
                missing: false,
                label: tree
                    .branch
                    .clone()
                    .or_else(|| {
                        tree.head
                            .as_ref()
                            .map(|head| head.chars().take(7).collect())
                    })
                    .unwrap_or_else(|| folder_name(&tree.path)),
                path: tree.path,
                branch: tree.branch,
                head: tree.head,
            })
            .collect(),
    )
}

/// The state to save: `live` tabs in order, each with its home and whether it shows, then the dormant ones.
pub(in crate::workspace) fn state(
    projects: &[Project],
    live: Vec<(Home, SavedTab, bool)>,
    dormant: &HashMap<Home, Dormant>,
    selected: &Home,
) -> State {
    let mut spaces: Vec<(Home, Dormant)> = Vec::new();
    for (home, tab, shown) in live {
        let at = match spaces.iter().position(|(of, _)| *of == home) {
            Some(at) => at,
            None => {
                let empty = Dormant {
                    shown: 0,
                    tabs: Vec::new(),
                    failed: false,
                };
                spaces.push((home, empty));
                spaces.len() - 1
            }
        };
        let space = &mut spaces[at].1;
        if shown {
            space.shown = space.tabs.len();
        }
        space.tabs.push(tab);
    }
    for (home, asleep) in dormant.iter().filter(|(_, asleep)| !asleep.tabs.is_empty()) {
        match spaces.iter_mut().find(|(of, _)| of == home) {
            Some((_, space)) => space.tabs.extend(asleep.tabs.iter().cloned()),
            None => spaces.push((home.clone(), asleep.clone())),
        }
    }
    spaces.sort_by(|(left, _), (right, _)| order(left).cmp(&order(right)));
    let mut state = State::empty();
    state.selected = selected.clone();
    state.projects = projects
        .iter()
        .map(|project| SavedProject {
            folder: project.folder.clone(),
            name: project.name.clone(),
            open: project.open,
            worktrees: cache(&project.trees),
            spaces: Vec::new(),
        })
        .collect();
    for (home, space) in spaces {
        let Dormant { shown, tabs, .. } = space;
        match home {
            Home::Others => state.others = SavedSpace { shown, tabs },
            Home::Tree { project, path } => {
                match state
                    .projects
                    .iter_mut()
                    .find(|saved| saved.folder == project)
                {
                    Some(saved) => saved.spaces.push(SavedTree { path, shown, tabs }),
                    None => log::warn!(
                        "workspace: {} tabs of {} have no project and are not saved",
                        tabs.len(),
                        path.display()
                    ),
                }
            }
        }
    }
    state
}

/// The projects, dormant tabs and selected home `state` holds, ids counted from `next`; a selected home whose project is gone is Others.
pub(in crate::workspace) fn parts(
    state: State,
    next: &mut u64,
) -> (Vec<Project>, HashMap<Home, Dormant>, Home) {
    let mut dormant = HashMap::new();
    let mut projects = Vec::new();
    for saved in state.projects {
        for tree in saved
            .spaces
            .into_iter()
            .filter(|tree| !tree.tabs.is_empty())
        {
            let home = Home::Tree {
                project: saved.folder.clone(),
                path: tree.path,
            };
            let asleep = Dormant {
                shown: tree.shown,
                tabs: tree.tabs,
                failed: false,
            };
            dormant.insert(home, asleep);
        }
        projects.push(Project {
            id: *next,
            folder: saved.folder,
            name: saved.name,
            open: saved.open,
            trees: cached(saved.worktrees),
            listing: Listing::default(),
        });
        *next += 1;
    }
    if !state.others.tabs.is_empty() {
        let asleep = Dormant {
            shown: state.others.shown,
            tabs: state.others.tabs,
            failed: false,
        };
        dormant.insert(Home::Others, asleep);
    }
    let selected = match state.selected {
        Home::Tree { project, .. } if !projects.iter().any(|kept| kept.folder == project) => {
            log::info!(
                "workspace: the selected project {} is gone; Others is selected",
                project.display()
            );
            Home::Others
        }
        home => home,
    };
    (projects, dormant, selected)
}

/// A session as `workspace.json` keeps it: where it started, not where it is.
pub(in crate::workspace) fn saved(session: &Session) -> SavedTab {
    SavedTab::Terminal {
        name: session.name.clone(),
        shell: session.shell.clone(),
        args: session.args.clone(),
        folder: session.start.clone(),
        claude: session.claude.clone(),
    }
}
