//! Projects, their worktrees and the home a folder belongs to.
pub mod claude;
pub mod store;
#[cfg(test)]
mod tests;

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A tab's identity, kept while it lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TabId(pub u64);

/// Where tabs live: a project's checkout, or nowhere in particular.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Home {
    Others,
    /// `path == project` is the main checkout, or a plain folder's own workspace.
    Tree {
        project: PathBuf,
        path: PathBuf,
    },
}

/// A worktree as the saved file caches it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Worktree {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub head: Option<String>,
}

/// The absolute form of `path`; a folder that is gone keeps its name under its parent's form, so a missing worktree still has a home.
pub fn normal(path: &Path) -> PathBuf {
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| {
        let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        match (absolute.parent(), absolute.file_name()) {
            (Some(parent), Some(name)) => normal(parent).join(name),
            _ => absolute,
        }
    });
    #[cfg(windows)]
    {
        let text = path.to_string_lossy();
        if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{rest}"));
        }
        if let Some(rest) = text.strip_prefix(r"\\?\") {
            return PathBuf::from(rest);
        }
    }
    path
}

/// The home of `folder`: the deepest worktree or project folder that holds it, else Others.
pub fn home_of(projects: &[(PathBuf, Vec<PathBuf>)], folder: &Path) -> Home {
    let worktrees = projects
        .iter()
        .flat_map(|(project, trees)| trees.iter().map(move |tree| (project, tree)));
    let folders = projects.iter().map(|(project, _)| (project, project));
    // Deepest first, then a project's own folder, then the first project by path, so order never decides.
    let mut best: Option<(usize, bool, &PathBuf, &PathBuf)> = None;
    for (project, path) in worktrees.chain(folders) {
        let Some(depth) = holds(path, folder) else {
            continue;
        };
        let own = path == project;
        let better = best.is_none_or(|(deepest, owned, chosen, _)| {
            (depth, own) > (deepest, owned)
                || ((depth, own) == (deepest, owned) && project < chosen)
        });
        if better {
            best = Some((depth, own, project, path));
        }
    }
    match best {
        Some((_, _, project, path)) => Home::Tree {
            project: project.clone(),
            path: path.clone(),
        },
        None => Home::Others,
    }
}

/// The component count of `root` when `folder` is it or lies inside it.
fn holds(root: &Path, folder: &Path) -> Option<usize> {
    let mut inside = folder.components();
    let mut depth = 0;
    for part in root.components() {
        if !same(part, inside.next()?) {
            return None;
        }
        depth += 1;
    }
    Some(depth)
}

fn same(left: Component, right: Component) -> bool {
    if cfg!(windows) {
        left.as_os_str().to_string_lossy().to_lowercase()
            == right.as_os_str().to_string_lossy().to_lowercase()
    } else {
        left == right
    }
}
