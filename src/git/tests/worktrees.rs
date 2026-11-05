use super::*;

fn commit(root: &Path) {
    write(root, "a.txt", "a");
    prepare(root, &["add", "."]);
    prepare(root, &["commit", "-q", "-m", "one"]);
}

#[test]
fn lists_main_and_linked() {
    let root = repository("wt-list");
    commit(&root);
    let linked = root.with_file_name(format!("wt-linked-{}", std::process::id()));
    let _ = fs::remove_dir_all(&linked);
    prepare(
        &root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feat",
            linked.to_str().unwrap(),
        ],
    );
    let found = worktrees(&root).unwrap().expect("a repository");
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].label(), "main");
    assert_eq!(found[1].branch.as_deref(), Some("feat"));
    assert_eq!(
        found[1].path.canonicalize().unwrap(),
        linked.canonicalize().unwrap()
    );
    fs::remove_dir_all(&linked).unwrap();
}

#[test]
fn plain_folder_and_subfolder_are_none() {
    let plain = folder("wt-plain");
    assert_eq!(worktrees(&plain).unwrap(), None);
    let root = repository("wt-sub");
    commit(&root);
    fs::create_dir_all(root.join("sub")).unwrap();
    assert_eq!(worktrees(&root.join("sub")).unwrap(), None);
}

#[test]
fn a_removed_worktree_leaves_the_list_and_a_deleted_folder_is_prunable() {
    let root = repository("wt-gone");
    commit(&root);
    let make = |name: &str, branch: &str| {
        let path = root.with_file_name(format!("{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        prepare(
            &root,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                branch,
                path.to_str().unwrap(),
            ],
        );
        path
    };
    let removed = make("wt-removed", "b1");
    let deleted = make("wt-deleted", "b2");
    assert_eq!(worktrees(&root).unwrap().expect("a repository").len(), 3);
    prepare(&root, &["worktree", "remove", removed.to_str().unwrap()]);
    fs::remove_dir_all(&deleted).unwrap();
    let found = worktrees(&root).unwrap().expect("a repository");
    assert_eq!(found.len(), 2, "the removed one left the list");
    assert!(
        found[1].prunable && found[1].branch.as_deref() == Some("b2"),
        "a deleted folder stays listed as prunable"
    );
}
