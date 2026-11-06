use ely_gpui_component::lists::GitStatus;

use super::*;

fn made(paths: &[&str]) -> Vec<Change> {
    let change = |path: &&str| Change {
        path: path.into(),
        from: None,
        status: GitStatus::Modified,
    };
    paths.iter().map(change).collect()
}

/// Each row as `depth name`, a folder ending in `/`, a file in its place.
fn drawn(rows: &[Row]) -> Vec<String> {
    let line = |row: &Row| match &row.item {
        Item::Folder { name, count, .. } => format!("{} {name}/ {count}", row.depth),
        Item::File { at, name } => format!("{} {name} @{at}", row.depth),
    };
    rows.iter().map(line).collect()
}

#[test]
fn folders_come_before_files_each_by_name_ignoring_case() {
    let changes = made(&["b.txt", "Zeta/z.rs", "A.txt", "alpha/a.rs", "alpha/B.rs"]);
    let rows = rows(&changes, |_| false);
    assert_eq!(
        drawn(&rows),
        [
            "0 alpha/ 2",
            "1 a.rs @3",
            "1 B.rs @4",
            "0 Zeta/ 1",
            "1 z.rs @1",
            "0 A.txt @2",
            "0 b.txt @0",
        ]
    );
}

#[test]
fn a_chain_of_lone_folders_compacts_into_one_row() {
    let changes = made(&["src/term/view.rs", "src/term/pty.rs", "src/main.rs"]);
    let rows = rows(&changes, |_| false);
    assert_eq!(
        drawn(&rows),
        [
            "0 src/ 3",
            "1 term/ 2",
            "2 pty.rs @1",
            "2 view.rs @0",
            "1 main.rs @2",
        ],
        "src holds a file, so it stays apart from term"
    );
    let deep = made(&["a/b/c/d.rs", "a/b/c/e.rs"]);
    let rows = super::rows(&deep, |_| false);
    assert_eq!(drawn(&rows), ["0 a/b/c/ 2", "1 d.rs @0", "1 e.rs @1"]);
    let Item::Folder { path, key, .. } = &rows[0].item else {
        panic!("a folder first");
    };
    assert_eq!(path, Path::new("a/b/c"));
    assert_eq!(key.as_ref(), "a/b/c");
}

#[test]
fn a_shut_folder_hides_what_it_holds_and_still_counts_it() {
    let changes = made(&["docs/a.md", "docs/deep/b.md", "src/x.rs"]);
    let rows = rows(&changes, |key| key.as_ref() == "docs");
    assert_eq!(drawn(&rows), ["0 docs/ 2", "0 src/ 1", "1 x.rs @2"]);
    let Item::Folder { open, .. } = rows[0].item else {
        panic!("a folder first");
    };
    assert!(!open);
}

#[test]
fn an_untracked_folder_is_a_leaf_named_without_its_slash() {
    let changes = made(&["notes/", "web/assets/"]);
    let rows = rows(&changes, |_| false);
    assert_eq!(drawn(&rows), ["0 web/ 1", "1 assets @1", "0 notes @0"]);
}

#[cfg(unix)]
#[test]
fn a_name_not_in_utf8_keeps_its_bytes_and_shows_quoted() {
    use std::os::unix::ffi::OsStrExt;
    let raw = |bytes: &[u8]| PathBuf::from(OsStr::from_bytes(bytes));
    let change = |path| Change {
        path,
        from: None,
        status: GitStatus::Modified,
    };
    let changes = [
        change(raw(b"bad\xff/one.txt")),
        change(raw(b"bad\xef\xbf\xbd/two.txt")),
        change(raw(b"line\nbreak/x")),
    ];
    let rows = rows(&changes, |_| false);
    let folders: Vec<(PathBuf, String)> = rows
        .iter()
        .filter_map(|row| match &row.item {
            Item::Folder { path, key, .. } => Some((path.clone(), key.to_string())),
            Item::File { .. } => None,
        })
        .collect();
    assert_eq!(folders.len(), 3, "a lossy name never joins another folder");
    assert!(folders.contains(&(raw(b"bad\xff"), "\"bad\\377\"".into())));
    assert!(folders.contains(&(raw(b"bad\xef\xbf\xbd"), "bad\u{FFFD}".into())));
    assert!(folders.contains(&(raw(b"line\nbreak"), "\"line\\nbreak\"".into())));
}
