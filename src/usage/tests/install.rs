use std::{
    fs,
    io::Write as _,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use serde_json::{Value, json};

use super::{
    super::install::{Install, SCRIPT, Status},
    folder,
};

/// A Claude folder and the app's usage folder, both under a fresh temp folder.
fn setup(name: &str, settings: Option<&str>) -> (PathBuf, Install) {
    let root = folder(name);
    let claude = root.join("claude");
    fs::create_dir_all(&claude).unwrap();
    if let Some(text) = settings {
        fs::write(claude.join("settings.json"), text).unwrap();
    }
    let install = Install {
        settings: Some(claude.join("settings.json")),
        usage: root.join("app usage"),
    };
    (root, install)
}

const SETTINGS: &str = r#"{
    "model": "opus",
    "statusLine": {"type": "command", "command": "node ~/.claude/hud/omc-hud.mjs", "padding": 1, "refreshInterval": 5},
    "hooks": {"Stop": []},
    "zeta": 1
}"#;

fn on_disk(install: &Install) -> Value {
    serde_json::from_str(&fs::read_to_string(install.settings.as_ref().unwrap()).unwrap()).unwrap()
}

fn backups(install: &Install) -> Vec<PathBuf> {
    let folder = install.settings.as_ref().unwrap().parent().unwrap();
    let mut found: Vec<_> = fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.to_string_lossy().ends_with(".bak"))
        .collect();
    found.sort();
    found
}

#[test]
fn apply_wraps_the_line_and_keeps_every_other_key_in_order() {
    let (_root, install) = setup("install-apply", Some(SETTINGS));
    assert_eq!(install.status(), Status::Off);
    let plan = install.plan().unwrap();
    install.apply(&plan, 100).unwrap();
    let settings = on_disk(&install);
    let keys: Vec<_> = settings.as_object().unwrap().keys().cloned().collect();
    assert_eq!(keys, ["model", "statusLine", "hooks", "zeta"]);
    let line = &settings["statusLine"];
    assert_eq!(line["command"], json!(install.command().unwrap()));
    assert_eq!(
        (line["padding"].clone(), line["refreshInterval"].clone()),
        (json!(1), json!(5))
    );
    assert_eq!(install.status(), Status::On);
    let backup = backups(&install);
    assert_eq!(backup.len(), 1);
    assert_eq!(fs::read_to_string(&backup[0]).unwrap(), SETTINGS);
    let inner = fs::read_to_string(install.usage.join("claude-inner")).unwrap();
    assert_eq!(inner, "node ~/.claude/hud/omc-hud.mjs");
    assert_eq!(fs::read_to_string(install.script()).unwrap(), SCRIPT);
}

#[test]
fn revert_puts_the_original_back_exactly() {
    let (_root, install) = setup("install-revert", Some(SETTINGS));
    let original = on_disk(&install);
    install.apply(&install.plan().unwrap(), 100).unwrap();
    assert_eq!(install.revert(100).unwrap(), Status::Off);
    let back = on_disk(&install);
    assert_eq!(
        serde_json::to_string(&back).unwrap(),
        serde_json::to_string(&original).unwrap()
    );
    assert_eq!(install.status(), Status::Off);
    assert_eq!(backups(&install).len(), 2, "each write backs up first");
    assert!(!install.usage.join("claude-inner").exists());
}

#[test]
fn no_line_before_is_removed_again() {
    let (_root, install) = setup("install-none", Some("{\"model\": \"opus\"}"));
    let plan = install.plan().unwrap();
    assert_eq!(plan.before, None);
    install.apply(&plan, 7).unwrap();
    assert_eq!(on_disk(&install)["statusLine"]["type"], json!("command"));
    assert_eq!(
        fs::read_to_string(install.usage.join("claude-inner")).unwrap(),
        ""
    );
    install.revert(8).unwrap();
    assert_eq!(on_disk(&install), json!({"model": "opus"}));
}

#[test]
fn a_change_since_the_dialog_opened_writes_nothing() {
    let (_root, install) = setup("install-since", Some(SETTINGS));
    let plan = install.plan().unwrap();
    let path = install.settings.clone().unwrap();
    let edited = SETTINGS.replace("omc-hud", "other-hud");
    fs::write(&path, &edited).unwrap();
    let error = install.apply(&plan, 100).unwrap_err();
    assert!(format!("{error:#}").contains("changed since"), "{error:#}");
    assert_eq!(fs::read_to_string(&path).unwrap(), edited);
    assert!(backups(&install).is_empty());
}

#[test]
fn a_write_racing_the_rename_writes_nothing() {
    let (_root, install) = setup("install-race", Some(SETTINGS));
    let plan = install.plan().unwrap();
    let path = install.settings.clone().unwrap();
    let raced = SETTINGS.replace("opus", "sonnet-raced");
    let error = install
        .apply_with(&plan, 100, || fs::write(&path, &raced).unwrap())
        .unwrap_err();
    assert!(format!("{error:#}").contains("changed while"), "{error:#}");
    assert_eq!(fs::read_to_string(&path).unwrap(), raced);
    assert_eq!(
        install.status(),
        Status::Off,
        "the state goes with the failed write"
    );
}

#[test]
fn a_foreign_edit_stays_untouched() {
    let (_root, install) = setup("install-foreign", Some(SETTINGS));
    install.apply(&install.plan().unwrap(), 100).unwrap();
    let path = install.settings.clone().unwrap();
    let foreign = "{\"statusLine\": {\"type\": \"command\", \"command\": \"mine\"}}";
    fs::write(&path, foreign).unwrap();
    assert_eq!(install.status(), Status::Elsewhere);
    assert_eq!(install.revert(101).unwrap(), Status::Elsewhere);
    assert_eq!(fs::read_to_string(&path).unwrap(), foreign);
    install.forget();
    assert_eq!(install.status(), Status::Off);
    assert_eq!(fs::read_to_string(&path).unwrap(), foreign);
}

#[test]
fn bad_or_missing_settings_are_refused() {
    let (_root, bad) = setup("install-bad", Some("{\"statusLine\": "));
    assert!(matches!(bad.status(), Status::Error(_)));
    assert!(format!("{:#}", bad.plan().unwrap_err()).contains("not JSON"));
    let (_root, missing) = setup("install-missing", None);
    assert_eq!(missing.status(), Status::Missing);
    assert!(missing.plan().is_err());
    let (_root, odd) = setup(
        "install-odd",
        Some("{\"statusLine\": {\"type\": \"static\"}}"),
    );
    assert!(format!("{:#}", odd.plan().unwrap_err()).contains("not a command"));
}

#[test]
fn applying_twice_never_wraps_the_wrapper() {
    let (_root, install) = setup("install-twice", Some(SETTINGS));
    install.apply(&install.plan().unwrap(), 100).unwrap();
    install.forget();
    assert!(format!("{:#}", install.plan().unwrap_err()).contains("already"));
}

/// Runs the installed script as Claude would, giving its output, status and the data file.
fn run(install: &Install, input: &str) -> (String, Option<i32>, Option<String>) {
    let mut child = Command::new("/bin/sh")
        .arg("-c")
        .arg(install.command().unwrap())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let done = child.wait_with_output().unwrap();
    let data = fs::read_to_string(install.usage.join("claude-rate.json")).ok();
    (
        String::from_utf8(done.stdout).unwrap(),
        done.status.code(),
        data,
    )
}

fn mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn the_script_keeps_limits_and_passes_the_line_through() {
    let inner = r#"{"statusLine": {"type": "command", "command": "read -r line; printf 'hud:%s\\n' \"$line\"; echo second; exit 3"}}"#;
    let (_root, install) = setup("install-script", Some(inner));
    install.apply(&install.plan().unwrap(), 1).unwrap();
    let (out, status, data) = run(&install, "{\"cwd\":\"/\"}");
    assert_eq!(
        (out.as_str(), status, data),
        ("hud:{\"cwd\":\"/\"}\nsecond\n", Some(3), None)
    );
    let limits = "{\"rate_limits\":{\"five_hour\":{\"used_percentage\":5,\"resets_at\":9}},\"x\":\"$HOME `id` \\\\\"}";
    let (out, status, data) = run(&install, limits);
    assert_eq!(
        (out.lines().next(), status),
        (Some(format!("hud:{limits}").as_str()), Some(3))
    );
    assert_eq!(data.as_deref(), Some(limits));
    assert_eq!(mode(&install.usage.join("claude-rate.json")), 0o600);
    assert_eq!(
        (mode(&install.usage), mode(&install.script())),
        (0o700, 0o700)
    );
    let (_, _, kept) = run(&install, "{\"cwd\":\"/\"}");
    assert_eq!(
        kept.as_deref(),
        Some(limits),
        "input without limits never replaces them"
    );
}

#[test]
fn the_script_without_an_original_prints_nothing() {
    let (_root, install) = setup("install-bare", Some("{}"));
    install.apply(&install.plan().unwrap(), 1).unwrap();
    assert_eq!(
        run(&install, "{\"rate_limits\":{}}"),
        (String::new(), Some(0), Some("{\"rate_limits\":{}}".into()))
    );
}
