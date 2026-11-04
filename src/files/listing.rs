//! A folder's entries as Files reads them, and the keys and marks its tree shows.

use std::{ffi::OsString, fs, io, path::Path, time::SystemTime};

use ely_gpui_component::{
    data_display::Tone,
    lists::{GitStatus, TreeNode},
};
use gpui::SharedString;

/// A folder's entry: name, whether a folder, and modified time.
#[derive(Debug, PartialEq)]
pub(super) struct Entry {
    pub(super) name: OsString,
    pub(super) folder: bool,
    pub(super) modified: SystemTime,
}

/// A folder's entries, or why it could not be read.
pub(super) type Listing = Result<Vec<Entry>, String>;

/// Reads `folder`: folders first, then files, each by name ignoring case.
pub(super) fn list(folder: &Path) -> Listing {
    let read = || -> io::Result<Vec<Entry>> {
        let mut entries = Vec::new();
        for entry in fs::read_dir(folder)? {
            let entry = entry?;
            // An entry gone since the folder listed it is gone.
            let meta = match entry.metadata() {
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                meta => meta?,
            };
            let kind = meta.file_type();
            entries.push(Entry {
                folder: kind.is_dir() || (kind.is_symlink() && entry.path().is_dir()),
                name: entry.file_name(),
                modified: meta.modified()?,
            });
        }
        Ok(entries)
    };
    let mut entries = read().map_err(|error| error.to_string())?;
    entries.sort_by_cached_key(|entry| {
        let name = entry.name.to_string_lossy().to_lowercase();
        (!entry.folder, name, entry.name.clone())
    });
    Ok(entries)
}

/// Git's mark on a node: its letter and tint; a dot on a folder that holds changes.
pub(super) fn badge(node: TreeNode, mark: Option<GitStatus>, dirty: bool) -> TreeNode {
    let (note, tone) = match mark {
        Some(GitStatus::Modified) => ("M", Tone::Warning),
        Some(GitStatus::Added) => ("A", Tone::Success),
        Some(GitStatus::Deleted) => ("D", Tone::Danger),
        Some(GitStatus::Untracked) => ("U", Tone::Success),
        Some(GitStatus::Renamed) => ("R", Tone::Info),
        Some(GitStatus::Conflicted) => ("!", Tone::Danger),
        None if dirty => ("•", Tone::Warning),
        None => return node,
    };
    node.note(note).tone(tone)
}

/// `path`'s tree key, one to a path: a path not in UTF-8 adds its bytes after a NUL, which no path holds.
pub(super) fn key(path: &Path) -> SharedString {
    match path.to_str() {
        Some(text) => text.to_string().into(),
        None => {
            let bytes = path.as_os_str().as_encoded_bytes();
            let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
            format!("{}\0{hex}", path.to_string_lossy()).into()
        }
    }
}
