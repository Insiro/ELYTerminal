use std::path::{Path, PathBuf};

use super::*;
use crate::projects::store::State;

const ID: &str = "0b5f8c4e-6b7a-4d2e-9f1a-2c3d4e5f6a7b";

fn parked(claude: Option<&str>) -> Parked {
    Parked {
        home: Home::Tree {
            project: "/p".into(),
            path: "/p/wt".into(),
        },
        name: "build".into(),
        renamed: true,
        folder: Some("/p/wt/src".into()),
        shell: "/bin/zsh".into(),
        args: vec!["-l".into()],
        claude: claude.map(str::to_string),
        saved: 1_700_000_000,
        screen: "$ make\nok".into(),
    }
}

#[test]
fn a_saved_tab_round_trips_and_a_file_without_any_loads() {
    let mut state = State::empty();
    state.parked = vec![parked(Some(ID)), parked(None)];
    let text = serde_json::to_string(&state).unwrap();
    assert_eq!(serde_json::from_str::<State>(&text).unwrap(), state);
    let empty = serde_json::to_string(&State::empty()).unwrap();
    assert!(!empty.contains("parked"), "none saved writes none");
    let old = r#"{"version":1,"selected":"others","projects":[],"others":{"shown":0,"tabs":[]}}"#;
    assert!(
        serde_json::from_str::<State>(old)
            .unwrap()
            .parked
            .is_empty()
    );
}

#[test]
fn a_shell_resumes_in_its_folder_else_its_home_s() {
    let tab = parked(None);
    let all = resume(&tab, |_| true);
    assert_eq!(all.folder, Some(PathBuf::from("/p/wt/src")));
    assert!(!all.moved && all.claude.is_none());
    let gone = resume(&tab, |path: &Path| path == Path::new("/p/wt"));
    assert_eq!(gone.folder, Some(PathBuf::from("/p/wt")));
    assert!(gone.moved);
    let none = resume(&tab, |_| false);
    assert_eq!((none.folder, none.moved), (None, true));
}

#[test]
fn claude_resumes_by_its_id() {
    let plan = resume(&parked(Some(ID)), |_| true);
    assert_eq!(plan.claude, Some((ID.to_string(), Mode::Resume)));
    let moved = resume(&parked(Some(ID)), |path: &Path| path == Path::new("/p/wt"));
    assert!(
        moved.moved && moved.claude.is_none(),
        "Claude ties its id to the folder"
    );
    assert_eq!(
        crate::projects::claude::env(ID, Mode::Resume),
        format!("resume:{ID}")
    );
}

#[test]
fn only_absolute_utf8_folders_are_kept() {
    assert_eq!(keepable(Path::new("/a")), Some(PathBuf::from("/a")));
    assert_eq!(keepable(Path::new("a")), None);
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let bad = Path::new(std::ffi::OsStr::from_bytes(b"/a\xff"));
        assert_eq!(keepable(bad), None);
    }
}

#[test]
fn past_fifty_the_oldest_go() {
    let mut kept: Vec<Parked> = (0..KEPT as u64 + 3)
        .map(|at| Parked {
            saved: 1_000 + (at * 7) % 53,
            name: format!("t{at}"),
            ..parked(None)
        })
        .collect();
    let oldest: Vec<u64> = {
        let mut saved: Vec<u64> = kept.iter().map(|p| p.saved).collect();
        saved.sort();
        saved[..3].to_vec()
    };
    assert_eq!(cap(&mut kept), 3);
    assert_eq!(kept.len(), KEPT);
    assert!(kept.iter().all(|p| !oldest.contains(&p.saved)));
    assert_eq!(cap(&mut kept), 0);
}
