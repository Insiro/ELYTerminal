use std::{fs, path::PathBuf};

use super::{
    Home, Worktree, claude, home_of, normal,
    store::{Loaded, SavedPanel, SavedProject, SavedSpace, SavedTab, SavedTree, State, load, save},
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
        folder: Some("/p/app-feat".into()),
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
        panels: vec![SavedPanel {
            home: Home::Others,
            tab: Some(crate::settings::PanelTab::Files),
            preview: Some("/p/app/README.md".into()),
            open: vec!["/p/app/src".into()],
        }],
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
    corrupt(
        "ws-relative-doc",
        &json(|state| {
            state.others.tabs = vec![SavedTab::Doc {
                path: "docs/README.md".into(),
            }]
        }),
    );
}

#[test]
fn terminal_and_document_tabs_load_back_in_order() {
    let folder = folder("ws-docs");
    let path = folder.join("workspace.json");
    let mut saved = state(None);
    let doc = |path: &str| SavedTab::Doc { path: path.into() };
    saved.projects[0].spaces[0].tabs = vec![doc("/p/app-feat/README.md"), tab(None)];
    saved.others.tabs = vec![tab(None), doc("/gone/notes.json")];
    saved.others.shown = 1;
    save(&path, &saved).expect("saves");
    let text = fs::read_to_string(&path).expect("reads");
    assert!(text.contains(r#""kind": "doc""#), "{text}");
    assert_eq!(load(&path).expect("loads"), Loaded::Ok(saved));
    fs::remove_dir_all(folder).expect("cleanup");
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
    assert_eq!(claude::transcript(&id, &config), None, "no projects folder");
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

#[test]
fn a_restored_id_resumes_unless_its_transcript_is_surely_absent() {
    use claude::{Mode, mode};
    assert_eq!(mode(Some(true)), Mode::Resume);
    assert_eq!(mode(Some(false)), Mode::New);
    assert_eq!(mode(None), Mode::Resume);
    let mut none = claude::Modes::find(Vec::new());
    assert_eq!(none.of(&claude::new_id()), Mode::Resume);
}

#[test]
fn claude_s_exit_is_the_end_of_the_command_a_prompt_start_leads_into() {
    use claude::{End, Heard::*, Watch};
    let ends = |marks: &[claude::Heard]| {
        let mut watch = Watch::default();
        let ends: Vec<End> = marks.iter().filter_map(|mark| watch.hear(*mark)).collect();
        (ends, watch)
    };
    let exited = (vec![End::Exited], Watch::Off);
    // The scripts start Claude's command right after a prompt start of its own.
    assert_eq!(ends(&[Prompt, Output, Done(Some(3))]), exited);
    assert_eq!(
        ends(&[Prompt, Input, Prompt, Output, Done(Some(130))]),
        exited
    );
    // A line typed ahead runs first: its own, not Claude's.
    assert_eq!(
        ends(&[
            Prompt,
            Input,
            Output,
            Done(Some(0)),
            Prompt,
            Input,
            Prompt,
            Output,
            Done(Some(3))
        ]),
        exited
    );
    // A command an rc file ran before the first prompt is not Claude.
    let (none, watch) = ends(&[Output, Done(Some(0)), Prompt, Input]);
    assert!(none.is_empty() && watch.waiting());
    // A start with no status leaves Claude running.
    let (none, watch) = ends(&[Prompt, Output, Done(None)]);
    assert!(none.is_empty() && watch == Watch::Running);
    // The integration's D before Claude's command: it does not start; another's bare D is no word of it.
    let skipped = (vec![End::Skipped], Watch::Off);
    assert_eq!(ends(&[Unstarted, Prompt, Output, Done(Some(0))]), skipped);
    assert_eq!(ends(&[Prompt, Input, Unstarted]), skipped);
    assert_eq!(ends(&[Done(None), Prompt, Output, Done(Some(0))]), exited);
    // A suspended Claude runs on.
    #[cfg(unix)]
    {
        let suspended = Done(Some(128 + libc::SIGTSTP));
        assert_eq!(ends(&[Prompt, Output, suspended, Done(Some(0))]), exited);
    }
    // The timer arms at the first prompt, not before.
    let (_, watch) = ends(&[Output, Done(Some(0))]);
    assert!(!watch.prompted());
    let (_, watch) = ends(&[Prompt, Input]);
    assert!(watch.prompted() && watch.waiting());
    // Once only.
    assert_eq!(
        ends(&[Prompt, Output, Done(Some(0)), Prompt, Output, Done(Some(0))]),
        exited
    );
    let mut blind = Watch::Blind;
    assert_eq!(blind.hear(Done(Some(0))), None);
    assert!(!blind.waiting() && !blind.listening());
}

#[test]
fn cmd_runs_the_prelude_then_claude_and_refuses_what_it_would_read_apart() {
    let id = claude::new_id();
    assert_eq!(
        claude::cmd_line(&id, claude::Mode::New, "").expect("plain"),
        format!("claude --session-id {id}")
    );
    assert_eq!(
        claude::cmd_line(&id, claude::Mode::Resume, "nvm use 20").expect("a prelude"),
        format!("nvm use 20 & claude --resume {id}")
    );
    for bad in ["echo \"x\"", "set P=%PATH%"] {
        assert!(
            claude::cmd_line(&id, claude::Mode::New, bad).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn a_file_from_before_panels_loads_with_none() {
    let folder = folder("ws-no-panels");
    let path = folder.join("workspace.json");
    let mut old = state(None);
    old.panels.clear();
    save(&path, &old).expect("saves");
    let text = fs::read_to_string(&path).expect("reads");
    assert!(!text.contains("panels"), "no panels, no key: {text}");
    assert_eq!(load(&path).expect("loads"), Loaded::Ok(old));
    fs::remove_dir_all(folder).expect("cleanup");
}

#[test]
fn a_relative_panel_path_is_corrupt() {
    let mut bad = state(None);
    bad.panels[0].open = vec!["src".into()];
    let text = serde_json::to_string(&bad).expect("serializes");
    corrupt("ws-panel-relative", &text);
}
