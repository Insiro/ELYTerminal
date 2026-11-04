use std::{collections::HashMap, path::PathBuf};

use super::{shape::*, *};
use crate::{
    projects::{
        Worktree, home_of,
        store::{SavedProject, SavedSpace, SavedTree},
    },
    workspace::projects::Trees,
};

fn tab(folder: &str, name: &str) -> SavedTab {
    SavedTab::Terminal {
        name: name.into(),
        shell: "/bin/zsh".into(),
        args: vec!["-l".into()],
        folder: (!folder.is_empty()).then(|| PathBuf::from(folder)),
        claude: (name == "claude").then(|| "6f1c2b3a-1d2e-4f5a-8b6c-7d8e9f0a1b2c".into()),
    }
}

fn tree(project: &str, path: &str) -> Home {
    Home::Tree {
        project: PathBuf::from(project),
        path: PathBuf::from(path),
    }
}

/// A state as the snapshot writes it: spaces by path, worktrees as labels.
fn saved_state() -> State {
    let mut state = State::empty();
    state.selected = tree("/p/app", "/p/app-feat");
    state.projects = vec![
        SavedProject {
            folder: PathBuf::from("/p/app"),
            name: Some("App".into()),
            open: true,
            worktrees: vec![
                Worktree {
                    path: PathBuf::from("/p/app"),
                    branch: Some("main".into()),
                    head: None,
                },
                Worktree {
                    path: PathBuf::from("/p/app-feat"),
                    branch: None,
                    head: Some("abcdef0123456789".into()),
                },
            ],
            spaces: vec![
                SavedTree {
                    path: PathBuf::from("/p/app"),
                    shown: 0,
                    tabs: vec![tab("/p/app", "")],
                },
                SavedTree {
                    path: PathBuf::from("/p/app-feat"),
                    shown: 1,
                    tabs: vec![tab("/p/app-feat", "one"), tab("/p/app-feat/src", "claude")],
                },
            ],
        },
        SavedProject {
            folder: PathBuf::from("/p/notes"),
            name: None,
            open: false,
            worktrees: Vec::new(),
            spaces: Vec::new(),
        },
    ];
    state.others = SavedSpace {
        shown: 0,
        tabs: vec![tab("/tmp", "x"), tab("", "home")],
    };
    state
}

#[test]
fn a_restored_state_saves_as_it_was_while_dormant() {
    let original = saved_state();
    let mut next = 0;
    let (projects, dormant, selected) = parts(original.clone(), &mut next);
    assert_eq!(next, 2);
    assert_eq!(dormant.len(), 3);
    assert_eq!(selected, tree("/p/app", "/p/app-feat"));
    let Trees::Git(trees) = &projects[0].trees else {
        panic!("the cache lists worktrees");
    };
    assert_eq!(trees[1].label, "abcdef0", "a detached head shows short");
    assert!(
        trees.iter().all(|tree| !tree.missing),
        "no disk read at restore"
    );
    assert_eq!(state(&projects, Vec::new(), &dormant, &selected), original);
}

#[test]
fn live_tabs_save_in_order_with_the_one_shown() {
    let original = saved_state();
    let mut next = 0;
    let (projects, mut dormant, selected) = parts(original.clone(), &mut next);
    // The feature worktree woke: its tabs live, the second shown.
    let feat = tree("/p/app", "/p/app-feat");
    let asleep = dormant.remove(&feat).expect("dormant feature tabs");
    let live = asleep
        .tabs
        .into_iter()
        .enumerate()
        .map(|(at, saved)| (feat.clone(), saved, at == 1))
        .collect();
    assert_eq!(state(&projects, live, &dormant, &selected), original);
}

#[test]
fn live_and_dormant_tabs_of_one_home_both_save() {
    let others = Home::Others;
    let dormant = HashMap::from([(
        others.clone(),
        Dormant {
            shown: 0,
            tabs: vec![tab("/b", "")],
            failed: false,
        },
    )]);
    let live = vec![(others.clone(), tab("/a", ""), true)];
    let saved = state(&[], live, &dormant, &others);
    assert_eq!(saved.others.tabs, vec![tab("/a", ""), tab("/b", "")]);
    assert_eq!(saved.others.shown, 0);
}

#[test]
fn a_selected_home_whose_project_is_gone_is_others() {
    let mut state = State::empty();
    state.selected = tree("/p/gone", "/p/gone");
    let (_, dormant, selected) = parts(state, &mut 0);
    assert_eq!(selected, Home::Others);
    assert!(dormant.is_empty());
}

#[test]
fn the_command_lines_folder_opens_in_its_home() {
    let (projects, ..) = parts(saved_state(), &mut 0);
    let listed: Vec<(PathBuf, Vec<PathBuf>)> = projects
        .iter()
        .map(|project| {
            let paths = match &project.trees {
                Trees::Git(trees) => trees.iter().map(|tree| tree.path.clone()).collect(),
                _ => Vec::new(),
            };
            (project.folder.clone(), paths)
        })
        .collect();
    let home = |folder: &str| home_of(&listed, std::path::Path::new(folder));
    assert_eq!(home("/p/app-feat/src"), tree("/p/app", "/p/app-feat"));
    assert_eq!(home("/p/app/docs"), tree("/p/app", "/p/app"));
    assert_eq!(home("/p/notes"), tree("/p/notes", "/p/notes"));
    assert_eq!(home("/elsewhere"), Home::Others);
}

#[test]
fn saves_land_in_order_and_an_older_one_passes_by() {
    let folder = tempfile_dir();
    let path = folder.join("workspace.json");
    let writer = Writer::new(path.clone(), None);
    let mut first = State::empty();
    first.selected = tree("/p/a", "/p/a");
    let second = State::empty();
    assert!(writer.write(2, &second).expect("the second saves"));
    assert!(!writer.write(1, &first).expect("the first passes by"));
    assert!(!writer.write(2, &first).expect("a repeat passes by"));
    assert!(
        !writer
            .write(3, &second)
            .expect("the same state writes nothing")
    );
    match store::load(&path).expect("the file reads") {
        store::Loaded::Ok(state) => assert_eq!(state, second),
        other => panic!("not saved: {other:?}"),
    }
    std::fs::remove_dir_all(folder).expect("the temp folder goes");
}

#[test]
fn a_corrupt_file_starts_empty_and_names_its_backup() {
    let folder = tempfile_dir();
    let path = folder.join("workspace.json");
    std::fs::write(&path, "{ not json").expect("the file writes");
    let Loaded::Corrupt { backup } = store::load(&path).expect("the file reads") else {
        panic!("a corrupt file loads as corrupt");
    };
    assert!(backup.is_file());
    assert!(!path.exists());
    // The writer saw no file, so the first save writes a fresh one.
    let writer = Writer::new(path.clone(), modified(&path));
    assert!(writer.write(1, &State::empty()).expect("a fresh save"));
    std::fs::remove_dir_all(folder).expect("the temp folder goes");
}

fn tempfile_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let folder = std::env::temp_dir().join(format!("ely-persist-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&folder).expect("the temp folder");
    folder
}

#[test]
fn a_document_whose_file_is_gone_waits_to_wake_as_any_tab() {
    let mut original = State::empty();
    let gone = SavedTab::Doc {
        path: PathBuf::from("/nowhere/elyterminal-gone.md"),
    };
    original.others.tabs = vec![tab("/a", ""), gone.clone()];
    original.others.shown = 1;
    let mut next = 0;
    let (projects, dormant, selected) = parts(original.clone(), &mut next);
    let asleep = &dormant[&Home::Others];
    assert_eq!(asleep.tabs[1], gone);
    assert!(!asleep.failed, "a missing file is no failed start");
    assert_eq!(state(&projects, Vec::new(), &dormant, &selected), original);
}
