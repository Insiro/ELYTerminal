//! A Claude tab's shell runs `claude` once at its first prompt, as a marked command.

use crate::projects::claude::{self as session, Heard, Watch};

use super::*;

/// A fake `claude` and prelude on PATH, each logging to `log`.
fn fakes(name: &str) -> (PathBuf, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let bin = folder(&format!("claude-bin-{name}"));
    let log = bin.join("log");
    let scripts = [
        (
            "claude",
            "printf 'claude %s\\n' \"$*\" >> \"$ELY_TEST_LOG\"\nprintf 'env %s|%s\\n' \"${ELY_CLAUDE-unset}\" \"${ELY_CLAUDE_PRELUDE-unset}\" >> \"$ELY_TEST_LOG\"\nexit 3\n",
        ),
        ("claude-pre", "echo pre >> \"$ELY_TEST_LOG\"\n"),
    ];
    for (name, body) in scripts {
        let path = bin.join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}")).expect("a fake");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("755");
    }
    (bin, log)
}

/// Starts `shell` asked for `ask`, with the prelude, and its log.
fn start(shell: &str, ask: &str) -> (Run, PathBuf, PathBuf) {
    let name = shell.rsplit('/').next().expect("a name");
    let (bin, log) = fakes(&format!("{name}-{}", ask.len()));
    let path = format!("{}:{}", bin.display(), env::var("PATH").unwrap_or_default());
    let env = vec![
        ("PATH".into(), path),
        ("ELY_TEST_LOG".into(), log.display().to_string()),
        ("ELY_CLAUDE".into(), ask.into()),
        ("ELY_CLAUDE_PRELUDE".into(), "claude-pre".into()),
    ];
    (Run::with(shell, &[], env), bin, log)
}

/// Runs Claude in `shell` once, its exit heard, then a command of the user's.
fn runs_once(shell: &str, mode: session::Mode, word: &str) {
    let id = session::new_id();
    let (mut run, bin, log) = start(shell, &session::env(&id, mode));
    let read = |log: &Path| fs::read_to_string(log).unwrap_or_default();
    run.until("claude's end", |run| {
        run.marks.contains(&Mark::Done(Some(3)))
    });
    run.send("echo AF''TER");
    run.until("the next command", |run| run.shows("\nAFTER"));
    assert_eq!(
        read(&log),
        format!("pre\nclaude {word} {id}\nenv unset|unset\n"),
        "{shell}"
    );
    let mut watch = Watch::default();
    let heard = run.marks.iter().map(|mark| match mark {
        Mark::Prompt(_) => Heard::Prompt,
        Mark::Input => Heard::Input,
        Mark::Output => Heard::Output,
        Mark::Done(status) => Heard::Done(*status),
    });
    let ends: Vec<_> = heard.filter(|mark| watch.hear(*mark)).collect();
    assert_eq!(ends, [Heard::Done(Some(3))], "{shell}: {:?}", run.marks);
    run.finish();
    fs::remove_dir_all(bin).expect("cleanup");
}

/// A malformed id starts nothing and says so.
fn refuses(shell: &str) {
    let (mut run, bin, log) = start(shell, "new:$(touch /tmp/x)");
    run.until("the refusal", |run| run.shows("names no Claude session"));
    run.send("echo AF''TER");
    run.until("the next command", |run| run.shows("\nAFTER"));
    assert!(!log.exists(), "{shell} ran something");
    run.finish();
    fs::remove_dir_all(bin).expect("cleanup");
}

#[test]
fn zsh_runs_claude_once() {
    let zsh = installed("zsh");
    runs_once(&zsh, session::Mode::New, "--session-id");
    runs_once(&zsh, session::Mode::Resume, "--resume");
    refuses(&zsh);
}

#[test]
fn bash_runs_claude_once() {
    let bash = installed("bash");
    runs_once(&bash, session::Mode::New, "--session-id");
    runs_once(&bash, session::Mode::Resume, "--resume");
    refuses(&bash);
}

#[test]
fn fish_runs_claude_once() {
    let fish = installed("fish");
    runs_once(&fish, session::Mode::New, "--session-id");
    refuses(&fish);
}

#[test]
fn pwsh_runs_claude_once() {
    let pwsh = installed("pwsh");
    runs_once(&pwsh, session::Mode::Resume, "--resume");
    refuses(&pwsh);
}
