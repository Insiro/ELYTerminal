use std::{
    cmp::Ordering,
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    path::{Component, Path, PathBuf},
};

use gpui::SharedString;

use super::view::shown;
use crate::git::Change;

/// A folder of changes: its folders by name, and its files with their places in the section.
#[derive(Default)]
struct Folder {
    folders: BTreeMap<OsString, Folder>,
    files: Vec<(OsString, usize)>,
}

/// What a row of the tree shows.
#[derive(Debug, PartialEq)]
pub(super) enum Item {
    /// A folder, or a chain of folders that each hold one folder alone; `key` is `path` shown.
    Folder {
        path: PathBuf,
        key: SharedString,
        name: SharedString,
        open: bool,
        count: usize,
    },
    /// The change at `at` in the section.
    File { at: usize, name: SharedString },
}

/// A row of the tree, `depth` folders in.
#[derive(Debug, PartialEq)]
pub(super) struct Row {
    pub depth: usize,
    pub item: Item,
}

/// Folders before files, each by name ignoring case, then by its bytes.
fn by_name(a: &OsStr, b: &OsStr) -> Ordering {
    let fold = |name: &OsStr| name.to_string_lossy().to_lowercase();
    fold(a)
        .cmp(&fold(b))
        .then_with(|| a.as_encoded_bytes().cmp(b.as_encoded_bytes()))
}

fn count(folder: &Folder) -> usize {
    folder.files.len() + folder.folders.values().map(count).sum::<usize>()
}

/// `changes` as rows of folders and files, VS Code's way; a folder whose key `shut` names hides what it holds.
pub(super) fn rows(changes: &[Change], shut: impl Fn(&SharedString) -> bool) -> Vec<Row> {
    let mut root = Folder::default();
    for (at, change) in changes.iter().enumerate() {
        let mut names: Vec<OsString> = (change.path.components())
            .filter_map(|part| match part {
                Component::Normal(name) => Some(name.to_os_string()),
                _ => None,
            })
            .collect();
        let name = names.pop().unwrap_or_else(|| change.path.clone().into());
        let mut folder = &mut root;
        for part in names {
            folder = folder.folders.entry(part).or_default();
        }
        folder.files.push((name, at));
    }
    let mut out = Vec::new();
    flatten(&root, Path::new(""), 0, &shut, &mut out);
    out
}

fn flatten(
    folder: &Folder,
    at: &Path,
    depth: usize,
    shut: &impl Fn(&SharedString) -> bool,
    out: &mut Vec<Row>,
) {
    let mut folders: Vec<_> = folder.folders.iter().collect();
    folders.sort_by(|a, b| by_name(a.0, b.0));
    for (name, mut inner) in folders {
        let mut path = at.join(name);
        let mut names = vec![shown(Path::new(name))];
        // A folder holding one folder alone shows as one row, `src/term`.
        while inner.files.is_empty() && inner.folders.len() == 1 {
            let (name, next) = inner.folders.iter().next().expect("one folder");
            path.push(name);
            names.push(shown(Path::new(name)));
            inner = next;
        }
        let key = shown(&path);
        let open = !shut(&key);
        let name = names.join("/").into();
        let count = count(inner);
        out.push(Row {
            depth,
            item: Item::Folder {
                path: path.clone(),
                key,
                name,
                open,
                count,
            },
        });
        if open {
            flatten(inner, &path, depth + 1, shut, out);
        }
    }
    let mut files: Vec<_> = folder.files.iter().collect();
    files.sort_by(|a, b| by_name(&a.0, &b.0));
    for (name, at) in files {
        let name = shown(Path::new(name));
        out.push(Row {
            depth,
            item: Item::File { at: *at, name },
        });
    }
}

#[cfg(test)]
mod tests;
