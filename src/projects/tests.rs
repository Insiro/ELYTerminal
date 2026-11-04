use std::{fs, path::PathBuf};

use super::{
    Home, Worktree, claude, home_of, normal,
    store::{Loaded, SavedProject, SavedSpace, SavedTab, SavedTree, State, load, save},
};

fn folder(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("elyterminal-{name}-{}", std::process::id()));
    if folder.exists() {
        fs::remove_dir_all(&folder).expect("a stale test folder goes");
    }
    fs::create_dir_all(&folder).expect("test folder");
    folder
}

fn tab(claude: Option<&str>) -> SavedTab {
    SavedTab::Terminal {
        name: String::new(),
        shell: "/bin/zsh".into(),
        args: vec!["-l".into()],
        folder: "/p/app-feat".into(),
        claude: claude.map(Into::into),
    }
}

fn state(claude: Option<&str>) -> State {
    State {
        version: 1,
        selected: Home::Tree {
            project: "/p/app".into(),
            path: "/p/app-feat".into(),
        },
        projects: vec![SavedProject {
            folder: "/p/app".into(),
            name: Some("App".into()),
            open: true,
            worktrees: vec![Worktree {
                path: "/p/app".into(),
                branch: Some("main".into()),
                head: Some("ab12cd3".into()),
            }],
            spaces: vec![SavedTree {
                path: "/p/app-feat".into(),
                shown: 0,
                tabs: vec![tab(claude)],
            }],
        }],
        others: SavedSpace::default(),
    }
}

#[test]
fn a_saved_state_loads_back_and_leaves_no_temp() {
    let folder = folder("ws-round");
    let path = folder.join("workspace.json");
    let saved = state(Some("6f1c2b3a-4d5e-4f60-8a7b-9c0d1e2f3a4b"));
    save(&path, &saved).expect("saves");
    assert_eq!(load(&path).expect("loads"), Loaded::Ok(saved));
    assert_eq!(fs::read_dir(&folder).expect("folder").count(), 1);
    fs::remove_dir_all(folder).expect("cleanup");
}

#[test]
fn no_file_is_fresh() {
    let folder = folder("ws-none");
    let loaded = load(&folder.join("workspace.json")).expect("loads");
    assert_eq!(loaded, Loaded::Fresh(State::empty()));
    fs::remove_dir_all(folder).expect("cleanup");
}

fn corrupt(name: &str, text: &str) {
    let folder = folder(name);
    let path = folder.join("workspace.json");
    fs::write(&path, text).expect("writes");
    let Loaded::Corrupt { backup } = load(&path).expect("loads") else {
        panic!("{name}: expected Corrupt");
    };
    assert!(backup.exists(), "{name}: backup stays");
    assert!(!path.exists(), "{name}: the file moved");
    assert_eq!(fs::read_to_string(backup).expect("reads"), text);
    fs::remove_dir_all(folder).expect("cleanup");
}

fn json(edit: impl FnOnce(&mut State)) -> String {
    let mut state = state(None);
    edit(&mut state);
    serde_json::to_string(&state).expect("encodes")
}

#[test]
fn a_broken_file_moves_aside() {
    corrupt("ws-bad-json", "{ not json");
    corrupt("ws-version", &json(|state| state.version = 2));
    corrupt(
        "ws-uuid",
        &json(|state| state.others.tabs = vec![tab(Some("NOT-A-UUID"))]),
    );
    corrupt(
        "ws-relative",
        &json(|state| state.projects[0].folder = "p/app".into()),
    );
}

fn projects() -> Vec<(PathBuf, Vec<PathBuf>)> {
    vec![
        (
            "/p/app".into(),
            vec![
                "/p/app".into(),
                "/p/app-feat".into(),
                "/p/app/.wt/deep".into(),
            ],
        ),
        ("/p/app2".into(), vec![]),
    ]
}

fn tree(project: &str, path: &str) -> Home {
    Home::Tree {
        project: project.into(),
        path: path.into(),
    }
}

#[test]
fn the_deepest_match_wins() {
    let all = projects();
    assert_eq!(
        home_of(&all, "/p/app/src".as_ref()),
        tree("/p/app", "/p/app")
    );
    assert_eq!(
        home_of(&all, "/p/app/.wt/deep/x".as_ref()),
        tree("/p/app", "/p/app/.wt/deep")
    );
    assert_eq!(
        home_of(&all, "/p/app-feat/src".as_ref()),
        tree("/p/app", "/p/app-feat")
    );
}

#[test]
fn a_sibling_name_is_no_match() {
    let all = projects();
    assert_eq!(
        home_of(&all, "/p/app2/x".as_ref()),
        tree("/p/app2", "/p/app2")
    );
    assert_eq!(home_of(&all, "/p/app3".as_ref()), Home::Others);
    assert_eq!(home_of(&all, "/q".as_ref()), Home::Others);
}

#[test]
fn a_worktree_two_projects_list_belongs_to_the_one_it_is_whatever_the_order() {
    let main = (
        PathBuf::from("/p/app"),
        vec![PathBuf::from("/p/app"), PathBuf::from("/p/app-feat")],
    );
    let linked = (
        PathBuf::from("/p/app-feat"),
        vec![PathBuf::from("/p/app"), PathBuf::from("/p/app-feat")],
    );
    for all in [vec![main.clone(), linked.clone()], vec![linked, main]] {
        assert_eq!(
            home_of(&all, "/p/app-feat/src".as_ref()),
            tree("/p/app-feat", "/p/app-feat")
        );
        assert_eq!(home_of(&all, "/p/app".as_ref()), tree("/p/app", "/p/app"));
    }
}

#[test]
fn a_gone_folder_keeps_its_name_under_its_parent_s_form() {
    let base = std::env::temp_dir();
    let gone = base.join("ely-no-such-folder").join("deeper");
    assert_eq!(
        normal(&gone),
        normal(&base).join("ely-no-such-folder").join("deeper")
    );
}

#[test]
fn ids_are_v4_and_strict() {
    let id = claude::new_id();
    assert!(claude::valid(&id), "{id}");
    assert_eq!(id.as_bytes()[14], b'4');
    assert!(matches!(id.as_bytes()[19], b'8' | b'9' | b'a' | b'b'));
    assert_ne!(id, claude::new_id());
    assert!(!claude::valid(&id.to_uppercase()));
    assert!(!claude::valid(&id[1..]));
    assert!(!claude::valid(&id.replace('-', "_")));
    assert!(!claude::valid("zzzzzzzz-zzzz-zzzz-zzzz-zzzzzzzzzzzz"));
    assert_eq!(claude::env(&id, claude::Mode::New), format!("new:{id}"));
    assert_eq!(
        claude::env(&id, claude::Mode::Resume),
        format!("resume:{id}")
    );
}

#[test]
fn a_transcript_is_found_by_id() {
    let config = folder("ws-claude");
    let id = claude::new_id();
    assert_eq!(claude::transcript(&id, &config), Some(false));
    fs::create_dir_all(config.join("projects/-p-app")).expect("project folder");
    assert_eq!(claude::transcript(&id, &config), Some(false));
    fs::write(
        config.join("projects/-p-app").join(format!("{id}.jsonl")),
        "{}",
    )
    .expect("writes");
    assert_eq!(claude::transcript(&id, &config), Some(true));
    fs::remove_dir_all(config).expect("cleanup");
}
