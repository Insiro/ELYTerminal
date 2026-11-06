use std::{fs, path::PathBuf};

use super::migrate::{Copy, FORK_KEYS, MARKER, Outcome, migrate};
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

const STRIPPED: &str = "{\n  \"font_size\": 14,\n  \"shell\": \"\"\n}\n";

fn copied(repaired: bool) -> Outcome {
    Outcome {
        copy: Copy::Copied { dropped: vec![] },
        repaired,
    }
}

#[test]
fn copies_settings_and_workspace_and_strips_fork_keys() {
    let (old, new) = folders("copy");
    fs::write(old.join("settings.json"), FORKED).unwrap();
    fs::write(old.join("workspace.json"), "{\"version\": 1}").unwrap();
    fs::create_dir(old.join("shell-integration")).unwrap();
    assert_eq!(migrate(&old, &new, 7), copied(true));
    assert_eq!(
        fs::read_to_string(new.join("settings.json")).unwrap(),
        FORKED
    );
    assert_eq!(
        fs::read_to_string(new.join("workspace.json")).unwrap(),
        "{\"version\": 1}"
    );
    assert!(new.join(MARKER).is_file());
    assert!(!new.join("shell-integration").exists());
    assert_eq!(
        fs::read_to_string(old.join("settings.json")).unwrap(),
        STRIPPED
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
fn copies_once_and_repairs_at_every_start() {
    let (old, new) = folders("once");
    fs::write(old.join("settings.json"), FORKED).unwrap();
    assert_eq!(migrate(&old, &new, 1), copied(true));
    fs::write(new.join("settings.json"), "{\"font_size\": 20}").unwrap();
    fs::write(old.join("settings.json"), FORKED).unwrap();
    let again = Outcome {
        copy: Copy::Present,
        repaired: true,
    };
    assert_eq!(migrate(&old, &new, 1), again);
    assert_eq!(
        fs::read_to_string(new.join("settings.json")).unwrap(),
        "{\"font_size\": 20}",
        "the fork's own settings stay"
    );
    assert_eq!(
        fs::read_to_string(old.join("settings.json")).unwrap(),
        STRIPPED
    );
    assert_eq!(
        fs::read_to_string(old.join("settings.json.insiro-1-1.bak")).unwrap(),
        FORKED,
        "a backup of the same second takes a count"
    );
    let clean = Outcome {
        copy: Copy::Present,
        repaired: false,
    };
    assert_eq!(migrate(&old, &new, 2), clean, "a clean file is left alone");
    assert!(!old.join("settings.json.insiro-2.bak").exists());
}

#[test]
fn a_fork_folder_with_settings_is_never_overwritten() {
    let (old, new) = folders("present");
    fs::write(old.join("settings.json"), FORKED).unwrap();
    fs::create_dir(&new).unwrap();
    fs::write(new.join("settings.json"), "{}").unwrap();
    let outcome = migrate(&old, &new, 3);
    assert_eq!(outcome.copy, Copy::Present);
    assert_eq!(fs::read_to_string(new.join("settings.json")).unwrap(), "{}");
    assert!(outcome.repaired, "upstream's start is mended anyway");
}

#[test]
fn a_fork_folder_without_settings_takes_the_copy() {
    let (old, new) = folders("bare");
    fs::write(old.join("settings.json"), FORKED).unwrap();
    fs::write(old.join("workspace.json"), "old").unwrap();
    fs::create_dir(&new).unwrap();
    fs::write(new.join("workspace.json"), "the fork's").unwrap();
    assert_eq!(migrate(&old, &new, 3), copied(true));
    assert_eq!(
        fs::read_to_string(new.join("settings.json")).unwrap(),
        FORKED
    );
    assert_eq!(
        fs::read_to_string(new.join("workspace.json")).unwrap(),
        "the fork's"
    );
}

#[test]
fn a_failed_copy_keeps_the_fork_keys_and_tries_again() {
    let (old, new) = folders("failed");
    fs::write(old.join("settings.json"), FORKED).unwrap();
    fs::write(&new, "a file where the folder goes").unwrap();
    let failed = Outcome {
        copy: Copy::Failed,
        repaired: false,
    };
    assert_eq!(migrate(&old, &new, 4), failed);
    assert_eq!(
        fs::read_to_string(old.join("settings.json")).unwrap(),
        FORKED
    );
    fs::remove_file(&new).unwrap();
    assert_eq!(migrate(&old, &new, 5), copied(true));
    assert_eq!(
        fs::read_to_string(new.join("settings.json")).unwrap(),
        FORKED
    );
}

#[test]
fn usage_is_copied_with_its_modes_and_the_old_stays() {
    let (old, new) = folders("usage");
    fs::write(old.join("settings.json"), "{}").unwrap();
    let usage = old.join("usage");
    fs::create_dir(&usage).unwrap();
    fs::write(usage.join("claude-install.json"), "{}").unwrap();
    fs::create_dir(usage.join("deep")).unwrap();
    fs::write(usage.join("deep/x"), "x").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&usage, fs::Permissions::from_mode(0o700)).unwrap();
        let state = usage.join("claude-install.json");
        fs::set_permissions(&state, fs::Permissions::from_mode(0o600)).unwrap();
    }
    assert_eq!(migrate(&old, &new, 6), copied(false));
    assert_eq!(fs::read_to_string(new.join("usage/deep/x")).unwrap(), "x");
    assert!(
        usage.join("claude-install.json").is_file(),
        "the old wrapper's files stay"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |path: PathBuf| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(new.join("usage")), 0o700);
        assert_eq!(mode(new.join("usage/claude-install.json")), 0o600);
    }
}

#[test]
fn keys_the_fork_does_not_know_are_left_out_of_its_copy() {
    let (old, new) = folders("unknown");
    let text = "{\"font_size\": 14, \"upstream_later\": 1, \"git_tree\": true}";
    fs::write(old.join("settings.json"), text).unwrap();
    let outcome = migrate(&old, &new, 8);
    let dropped = Copy::Copied {
        dropped: vec!["upstream_later".into()],
    };
    assert_eq!((outcome.copy, outcome.repaired), (dropped, true));
    let copy = fs::read_to_string(new.join("settings.json")).unwrap();
    assert_eq!(
        copy, "{\n  \"font_size\": 14,\n  \"git_tree\": true\n}\n",
        "order kept"
    );
    let settings: Settings = serde_json::from_str(&copy).expect("the fork reads its copy");
    assert!(settings.git_tree);
}

#[test]
fn a_bad_old_file_is_copied_but_left_alone() {
    let (old, new) = folders("bad");
    fs::write(old.join("settings.json"), "{ not json").unwrap();
    assert_eq!(migrate(&old, &new, 4), copied(false));
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
    assert_eq!(migrate(&old, &new, 5), copied(false));
    assert_eq!(fs::read_to_string(old.join("settings.json")).unwrap(), text);
}

#[test]
fn no_old_settings_does_nothing() {
    let (old, new) = folders("nothing");
    let nothing = Outcome {
        copy: Copy::Nothing,
        repaired: false,
    };
    assert_eq!(migrate(&old, &new, 6), nothing);
    assert!(!new.exists());
}

#[test]
fn each_fork_key_is_a_settings_field() {
    let fields = serde_json::to_value(Settings::default()).expect("settings encode");
    for key in FORK_KEYS {
        assert!(fields.get(key).is_some(), "{key} is no settings field");
    }
}
