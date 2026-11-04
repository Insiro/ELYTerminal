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
