use std::{fs, path::PathBuf};

use super::migrate::{FORK_KEYS, Outcome, migrate};
use crate::settings::Settings;

/// A fresh folder holding `old`, upstream's, and the path of `new`, the fork's.
fn folders(name: &str) -> (PathBuf, PathBuf) {
    let base =
        std::env::temp_dir().join(format!("elyterminal-migrate-{name}-{}", std::process::id()));
    if base.exists() {
        fs::remove_dir_all(&base).expect("a stale test folder goes");
    }
    let old = base.join("ELYTerminal");
    fs::create_dir_all(&old).expect("the old folder");
    (old, base.join("ELYTerminal Insiro"))
}

const FORKED: &str = "{\n  \"font_size\": 14,\n  \"claude_prelude\": \"nvm use\",\n  \"shell\": \"\",\n  \"usage_summary\": false,\n  \"git_tree\": true\n}\n";

#[test]
fn copies_settings_and_workspace_and_strips_fork_keys() {
    let (old, new) = folders("copy");
    fs::write(old.join("settings.json"), FORKED).unwrap();
    fs::write(old.join("workspace.json"), "{\"version\": 1}").unwrap();
    fs::create_dir(old.join("shell-integration")).unwrap();
    assert_eq!(migrate(&old, &new, 7), Outcome::Copied { repaired: true });
    assert_eq!(
        fs::read_to_string(new.join("settings.json")).unwrap(),
        FORKED
    );
    assert_eq!(
        fs::read_to_string(new.join("workspace.json")).unwrap(),
        "{\"version\": 1}"
    );
    assert!(!new.join("shell-integration").exists());
    assert_eq!(
        fs::read_to_string(old.join("settings.json")).unwrap(),
        "{\n  \"font_size\": 14,\n  \"shell\": \"\"\n}\n"
    );
    assert_eq!(
        fs::read_to_string(old.join("settings.json.insiro-7.bak")).unwrap(),
        FORKED
    );
    assert_eq!(
        fs::read_to_string(old.join("workspace.json")).unwrap(),
        "{\"version\": 1}"
    );
}

#[test]
fn runs_once() {
    let (old, new) = folders("once");
    fs::write(old.join("settings.json"), FORKED).unwrap();
    assert_eq!(migrate(&old, &new, 1), Outcome::Copied { repaired: true });
    fs::write(old.join("settings.json"), FORKED).unwrap();
    assert_eq!(migrate(&old, &new, 2), Outcome::Present);
    assert_eq!(
        fs::read_to_string(old.join("settings.json")).unwrap(),
        FORKED
    );
    assert!(!old.join("settings.json.insiro-2.bak").exists());
}

#[test]
fn a_standing_fork_folder_is_left_alone() {
    let (old, new) = folders("present");
    fs::write(old.join("settings.json"), FORKED).unwrap();
    fs::create_dir(&new).unwrap();
    fs::write(new.join("settings.json"), "{}").unwrap();
    assert_eq!(migrate(&old, &new, 3), Outcome::Present);
    assert_eq!(fs::read_to_string(new.join("settings.json")).unwrap(), "{}");
    assert_eq!(
        fs::read_to_string(old.join("settings.json")).unwrap(),
        FORKED
    );
}

#[test]
fn a_bad_old_file_is_copied_but_left_alone() {
    let (old, new) = folders("bad");
    fs::write(old.join("settings.json"), "{ not json").unwrap();
    assert_eq!(migrate(&old, &new, 4), Outcome::Copied { repaired: false });
    assert_eq!(
        fs::read_to_string(old.join("settings.json")).unwrap(),
        "{ not json"
    );
    assert!(!old.join("settings.json.insiro-4.bak").exists());
}

#[test]
fn upstream_settings_stay_as_written() {
    let (old, new) = folders("clean");
    let text = "{\"font_size\":14}";
    fs::write(old.join("settings.json"), text).unwrap();
    assert_eq!(migrate(&old, &new, 5), Outcome::Copied { repaired: false });
    assert_eq!(fs::read_to_string(old.join("settings.json")).unwrap(), text);
}

#[test]
fn no_old_settings_does_nothing() {
    let (old, new) = folders("nothing");
    assert_eq!(migrate(&old, &new, 6), Outcome::Nothing);
    assert!(!new.exists());
}

#[test]
fn each_fork_key_is_a_settings_field() {
    let fields = serde_json::to_value(Settings::default()).expect("settings encode");
    for key in FORK_KEYS {
        assert!(fields.get(key).is_some(), "{key} is no settings field");
    }
}
