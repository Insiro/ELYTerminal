use std::path::PathBuf;

use super::*;

fn tree(project: &str, path: &str) -> Home {
    Home::Tree {
        project: PathBuf::from(project),
        path: PathBuf::from(path),
    }
}

fn worktree(path: &str, label: &str, missing: bool) -> Tree {
    Tree {
        path: PathBuf::from(path),
        label: label.into(),
        branch: None,
        head: None,
        missing,
    }
}

fn project(trees: Trees) -> Project {
    Project {
        id: 0,
        folder: PathBuf::from("/p/app"),
        name: None,
        open: true,
        trees,
        listing: Listing::default(),
    }
}

#[test]
fn a_project_row_gathers_its_main_home_and_its_worktrees_while_shut() {
    let open = Row::Project {
        folder: PathBuf::from("/p/app"),
        open: true,
    };
    let shut = Row::Project {
        folder: PathBuf::from("/p/app"),
        open: false,
    };
    let main = tree("/p/app", "/p/app");
    let linked = tree("/p/app", "/p/app-feat");
    let other = tree("/p/app2", "/p/app2");
    assert!(open.covers(&main) && shut.covers(&main));
    assert!(
        !open.covers(&linked),
        "an open project's worktree marks its own row"
    );
    assert!(
        shut.covers(&linked),
        "a shut project's row marks for its worktrees"
    );
    assert!(!shut.covers(&other) && !shut.covers(&Home::Others));
    assert!(Row::Tree(linked.clone()).covers(&linked));
    assert!(!Row::Tree(linked).covers(&main));
    assert!(Row::Others.covers(&Home::Others) && !Row::Others.covers(&main));
}

#[test]
fn children_list_linked_worktrees_and_hide_a_missing_one_without_tabs() {
    let listed = project(Trees::Git(vec![
        worktree("/p/app", "main", false),
        worktree("/p/app-feat", "feat", false),
        worktree("/p/app-gone", "gone", true),
    ]));
    let none: [&Home; 0] = [];
    let rows = children(&listed, none);
    assert_eq!(
        rows,
        [worktree("/p/app-feat", "feat", false)],
        "main and the missing one stay out"
    );
    let held = tree("/p/app", "/p/app-gone");
    let rows = children(&listed, [&held]);
    assert_eq!(rows.len(), 2);
    assert!(
        rows[1].missing && rows[1].label == "gone",
        "a missing worktree with tabs shows"
    );
}

#[test]
fn a_tab_s_home_git_no_longer_lists_shows_as_missing() {
    let plain = project(Trees::Plain);
    let homes = [
        tree("/p/app", "/p/app"),
        tree("/p/app", "/p/old"),
        tree("/p/other", "/p/x"),
        Home::Others,
    ];
    let rows = children(&plain, &homes);
    assert_eq!(rows, [worktree("/p/old", "old", true)]);
}

#[test]
fn removing_a_project_closes_only_its_tabs() {
    let homes = [
        tree("/p/app", "/p/app"),
        Home::Others,
        tree("/p/app", "/p/app-feat"),
        tree("/p/app2", "/p/app2"),
    ];
    let tabs = homes
        .iter()
        .enumerate()
        .map(|(at, home)| (TabId(at as u64), home));
    assert_eq!(tabs_of(tabs, Path::new("/p/app")), [TabId(0), TabId(2)]);
}

#[test]
fn a_linked_worktree_added_as_a_project_names_its_own_branch_and_lists_the_main_one() {
    let mut linked = project(Trees::Git(vec![
        worktree("/p/main", "main", false),
        worktree("/p/app", "feat", false),
        worktree("/p/app-other", "other", false),
    ]));
    assert_eq!(linked.branch(), Some("feat"), "its own tree, not the first");
    let none: [&Home; 0] = [];
    let rows = children(&linked, none);
    assert_eq!(
        rows,
        [
            worktree("/p/main", "main", false),
            worktree("/p/app-other", "other", false)
        ]
    );
    linked.trees = Trees::Plain;
    assert_eq!(linked.branch(), None);
}

#[test]
fn a_project_is_called_its_name_else_its_folder_s() {
    let mut named = project(Trees::Pending);
    assert_eq!(named.label(), "app");
    named.name = Some("App".into());
    assert_eq!(named.label(), "App");
    assert_eq!(named.home(), tree("/p/app", "/p/app"));
}
