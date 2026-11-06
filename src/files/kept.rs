//! The trees Files left, by root, so a root shown again opens what it had open.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use gpui::{Context, SharedString};

use super::{Files, listing::key};

/// What a root's tree showed as Files left it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Kept {
    pub(super) open: HashSet<PathBuf>,
    pub(super) chosen: Option<SharedString>,
}

/// The trees left, by root; a tree with only its root open and nothing chosen is not kept.
#[derive(Debug, Default)]
pub(super) struct Trees(HashMap<PathBuf, Kept>);

impl Trees {
    /// Keeps what `root`'s tree showed.
    pub(super) fn leave(&mut self, root: PathBuf, kept: Kept) {
        let bare = kept.chosen.is_none() && kept.open.iter().all(|folder| *folder == root);
        if bare {
            self.0.remove(&root);
        } else {
            self.0.insert(root, kept);
        }
    }

    /// What `root`'s tree showed when left, else its root open alone.
    pub(super) fn enter(&mut self, root: &Path) -> Kept {
        let mut kept = self.0.remove(root).unwrap_or_default();
        kept.open.insert(root.to_path_buf());
        kept
    }

    pub(super) fn get(&self, root: &Path) -> Option<&Kept> {
        self.0.get(root)
    }
}

/// The tree opened or closed a folder.
pub struct Toggled;

impl gpui::EventEmitter<Toggled> for Files {}

impl Files {
    /// The workspace says whether Files shows, so it polls only then.
    pub fn set_showing(&mut self, showing: bool) {
        self.showing = showing;
    }

    /// The folders `root`'s tree shows open, below the root, shown or left.
    pub fn open_in(&self, root: &Path) -> Vec<PathBuf> {
        let open = if root == self.root {
            Some(&self.open)
        } else {
            self.kept.get(root).map(|kept| &kept.open)
        };
        let mut open: Vec<PathBuf> = (open.into_iter().flatten())
            .filter(|folder| folder.as_path() != root)
            .cloned()
            .collect();
        open.sort();
        open
    }

    /// Opens `open` in `root`'s tree, `chosen` chosen, as `workspace.json` kept them.
    pub fn seed(
        &mut self,
        root: PathBuf,
        open: Vec<PathBuf>,
        chosen: Option<&Path>,
        cx: &mut Context<Self>,
    ) {
        let inside: Vec<PathBuf> = open.into_iter().filter(|f| f.starts_with(&root)).collect();
        let chosen = chosen.filter(|path| path.starts_with(&root)).map(key);
        log::debug!(
            "files: {} restored with {} folders open",
            root.display(),
            inside.len()
        );
        if root == self.root {
            self.open.extend(inside);
            self.chosen = chosen.or(self.chosen.take());
            self.read(cx);
            return;
        }
        let mut kept = self.kept.enter(&root);
        kept.open.extend(inside);
        kept.chosen = chosen.or(kept.chosen);
        self.kept.leave(root, kept);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kept(open: &[&str], chosen: Option<&str>) -> Kept {
        Kept {
            open: open.iter().map(PathBuf::from).collect(),
            chosen: chosen.map(SharedString::from),
        }
    }

    #[test]
    fn a_root_left_opens_again_what_it_had_open() {
        let mut trees = Trees::default();
        let left = kept(&["/a", "/a/src", "/a/src/ui"], Some("/a/src/ui/x.rs"));
        trees.leave("/a".into(), left.clone());
        assert_eq!(trees.enter(Path::new("/b")), kept(&["/b"], None));
        assert_eq!(trees.enter(Path::new("/a")), left);
        // Entered, it is no longer kept: it lives in Files again.
        assert_eq!(trees.enter(Path::new("/a")), kept(&["/a"], None));
    }

    #[test]
    fn each_root_keeps_its_own_tree() {
        let mut trees = Trees::default();
        trees.leave("/a".into(), kept(&["/a", "/a/x"], None));
        trees.leave("/a/x".into(), kept(&["/a/x", "/a/x/y"], None));
        assert_eq!(
            trees.enter(Path::new("/a/x")),
            kept(&["/a/x", "/a/x/y"], None)
        );
        assert_eq!(trees.enter(Path::new("/a")), kept(&["/a", "/a/x"], None));
    }

    #[test]
    fn a_bare_tree_is_not_kept_and_drops_an_older_one() {
        let mut trees = Trees::default();
        trees.leave("/a".into(), kept(&["/a", "/a/x"], None));
        trees.leave("/a".into(), kept(&["/a"], None));
        assert!(trees.get(Path::new("/a")).is_none());
        trees.leave("/a".into(), kept(&["/a"], Some("/a/f")));
        assert!(trees.get(Path::new("/a")).is_some());
    }
}
