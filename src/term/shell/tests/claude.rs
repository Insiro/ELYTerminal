//! A Claude tab's shell runs `claude` once at its first prompt, as a command line of its own on the terminal.

use std::sync::atomic::AtomicUsize;

use crate::projects::claude::{self as session, End, Heard, Watch};

use super::*;

/// A fake `claude`, which logs its arguments and whether it holds the terminal, and a prelude, each logging to the folder's `log`.
fn fakes(name: &str) -> (PathBuf, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    static FAKES: AtomicUsize = AtomicUsize::new(0);
    let at = FAKES.fetch_add(1, Ordering::Relaxed);
    let bin = folder(&format!("claude-bin-{name}-{at}"));
    let log = bin.join("log");
    let claude = concat!(
        "t0=no t1=no\n[ -t 0 ] && t0=yes\n[ -t 1 ] && t1=yes\n",
        "printf 'claude %s tty %s %s\\n' \"$*\" $t0 $t1 >> \"$ELY_TEST_LOG\"\n",
        "printf 'env %s|%s\\n' \"${ELY_CLAUDE-unset}\" \"${ELY_CLAUDE_PRELUDE-unset}\" >> \"$ELY_TEST_LOG\"\n",
        "[ $t0 = yes ] && [ $t1 = yes ] || exit 9\nexit 3\n",
    );
    let scripts = [
        ("claude", claude),
        ("claude-pre", "echo pre >> \"$ELY_TEST_LOG\"\n"),
    ];
    for (name, body) in scripts {
        let path = bin.join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}")).expect("a fake");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("755");
    }
    (bin, log)
}

/// Starts `shell` asked for `ask`, with the prelude and `files` under HOME, and its log.
fn start(shell: &str, ask: &str, files: &[(&str, &str)]) -> (Run, PathBuf, PathBuf) {
    let (bin, log) = fakes(shell.rsplit('/').next().expect("a name"));
    let path = format!("{}:{}", bin.display(), env::var("PATH").unwrap_or_default());
    let env = vec![
        ("PATH".into(), path),
        ("ELY_TEST_LOG".into(), log.display().to_string()),
        ("ELY_CLAUDE".into(), ask.into()),
        ("ELY_CLAUDE_PRELUDE".into(), "claude-pre".into()),
    ];
    (Run::with(shell, files, env), bin, log)
}

/// What the watch makes of `marks`, and the mark that ended it.
fn watched(marks: &[Mark]) -> Vec<(End, Heard)> {
    let mut watch = Watch::default();
    let heard = marks.iter().map(|mark| match mark {
        Mark::Prompt(_) => Heard::Prompt,
        Mark::Input => Heard::Input,
        Mark::Output => Heard::Output,
        Mark::Done(status) => Heard::Done(*status),
        Mark::Unstarted => Heard::Unstarted,
    });
    heard
        .filter_map(|mark| watch.hear(mark).map(|end| (end, mark)))
        .collect()
}

/// Runs Claude in `shell` once, on the terminal, its exit heard, then a command of the user's; `files` slow the start, and with `ahead` a line is typed before it.
fn runs_once(shell: &str, mode: session::Mode, files: &[(&str, &str)], ahead: bool) {
    let word = match mode {
        session::Mode::New => "--session-id",
        session::Mode::Resume => "--resume",
    };
    let id = session::new_id();
    let (mut run, bin, log) = start(shell, &session::env(&id, mode), files);
    if ahead {
        run.pty.write(b"echo AHE''AD\r".to_vec());
    }
    // fish runs Claude's line at the key the app sends once its first prompt reads.
    if shell.ends_with("fish") {
        run.until("the first prompt's input", |run| {
            run.marks.contains(&Mark::Input)
        });
        run.pty.write(session::KEY.as_bytes().to_vec());
    }
    let read = |log: &Path| fs::read_to_string(log).unwrap_or_default();
    run.until("claude's end", |run| {
        run.marks.contains(&Mark::Done(Some(3))) || run.marks.contains(&Mark::Done(Some(9)))
    });
    if ahead {
        // The fake reads no keys, so the line runs after it, a real claude takes them; PowerShell reads its Enter as a newline in the line.
        run.until("the line typed ahead", |run| run.shows("AHE''AD"));
        if !run.shows("\nAHEAD") {
            run.send("");
        }
        run.until("the line typed ahead run", |run| run.shows("\nAHEAD"));
    }
    run.send("echo AF''TER");
    run.until("the next command", |run| run.shows("\nAFTER"));
    assert_eq!(
        read(&log),
        format!("pre\nclaude {word} {id} tty yes yes\nenv unset|unset\n"),
        "{shell}:\n{}",
        run.screen()
    );
    let claude = (End::Exited, Heard::Done(Some(3)));
    assert_eq!(watched(&run.marks), [claude], "{shell}: {:?}", run.marks);
    run.finish();
    fs::remove_dir_all(bin).expect("cleanup");
}

/// A malformed id starts nothing, says so, and tells the terminal.
fn refuses(shell: &str) {
    let (mut run, bin, log) = start(shell, "new:$(touch /tmp/x)", &[]);
    run.until("the refusal", |run| run.shows("names no Claude session"));
    run.send("echo AF''TER");
    run.until("the next command", |run| run.shows("\nAFTER"));
    run.until("the word that claude does not start", |run| {
        !watched(&run.marks).is_empty()
    });
    assert!(!log.exists(), "{shell} ran something");
    let skipped = (End::Skipped, Heard::Unstarted);
    assert_eq!(watched(&run.marks), [skipped], "{shell}: {:?}", run.marks);
    run.finish();
    fs::remove_dir_all(bin).expect("cleanup");
}

#[test]
fn zsh_runs_claude_once() {
    let zsh = installed("zsh");
    runs_once(&zsh, session::Mode::New, &[], false);
    runs_once(&zsh, session::Mode::Resume, &[(".zshrc", "sleep 1")], true);
    refuses(&zsh);
}

#[test]
fn bash_runs_claude_once() {
    let bash = installed("bash");
    runs_once(&bash, session::Mode::New, &[], false);
    let slow = [(".bashrc", "sleep 1"), (".bash_profile", "sleep 1")];
    runs_once(&bash, session::Mode::Resume, &slow, true);
    refuses(&bash);
}

#[test]
fn fish_runs_claude_once() {
    let fish = installed("fish");
    runs_once(&fish, session::Mode::New, &[], false);
    let slow = [(".config/fish/config.fish", "sleep 1")];
    runs_once(&fish, session::Mode::Resume, &slow, true);
    refuses(&fish);
}

#[test]
fn pwsh_runs_claude_once() {
    let pwsh = installed("pwsh");
    runs_once(&pwsh, session::Mode::New, &[], false);
    let slow = [(
        ".config/powershell/Microsoft.PowerShell_profile.ps1",
        "Start-Sleep 1",
    )];
    runs_once(&pwsh, session::Mode::Resume, &slow, true);
    refuses(&pwsh);
}

#[test]
fn bash_runs_claude_after_an_array_prompt_command() {
    let bash = installed("bash");
    let rc = "ua() { echo ua >> \"$ELY_TEST_LOG\"; }\nub() { echo ub >> \"$ELY_TEST_LOG\"; }\nPROMPT_COMMAND=(ua ub)\n";
    let id = session::new_id();
    let ask = session::env(&id, session::Mode::New);
    let (mut run, bin, log) = start(&bash, &ask, &[(".bashrc", rc), (".bash_profile", rc)]);
    run.until("claude's end", |run| {
        run.marks.contains(&Mark::Done(Some(3)))
    });
    run.send("echo AF''TER");
    run.until("the next command", |run| run.shows("\nAFTER"));
    // bash 5.1 runs every element, ours last; older ones run the first alone.
    let version = std::process::Command::new(&bash)
        .args(["-c", "echo $((BASH_VERSINFO[0] * 100 + BASH_VERSINFO[1]))"])
        .output()
        .expect("bash runs");
    let arrays = String::from_utf8_lossy(&version.stdout)
        .trim()
        .parse::<u32>()
        .expect("a version")
        >= 501;
    let before = if arrays { "ua\nub\n" } else { "ua\n" };
    let text = fs::read_to_string(&log).expect("the log");
    let first =
        format!("{before}pre\nclaude --session-id {id} tty yes yes\nenv unset|unset\n{before}");
    assert!(text.starts_with(&first), "{text}");
    assert_eq!(text.matches("claude ").count(), 1, "{text}");
    run.finish();
    fs::remove_dir_all(bin).expect("cleanup");
}
