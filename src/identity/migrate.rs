//! At start: the fork's files copied out of upstream's folder until one copy finishes, and upstream's settings cleared of the fork's keys whenever they hold them.

use std::{
    fs,
    io::{self, Write as _},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Context as _;

use super::{NAME, UPSTREAM};

/// Settings keys only the fork knows, which upstream's `deny_unknown_fields` refuses.
pub const FORK_KEYS: [&str; 7] = [
    "claude_prelude",
    "codex_usage",
    "claude_api_usage",
    "codex_live_usage",
    "usage_summary",
    "git_tree",
    "panel_scale",
];

/// The file the fork's folder holds once a copy finished whole.
pub const MARKER: &str = "migrated";

/// What a start's copy did.
#[derive(Debug, PartialEq)]
pub enum Copy {
    /// It ran before, or the fork's folder holds settings of its own.
    Present,
    /// Upstream's folder holds no settings to take.
    Nothing,
    /// The files were copied; `dropped` names the settings keys the fork does not know.
    Copied { dropped: Vec<String> },
    /// The copy failed; the fork starts fresh and the next start tries again.
    Failed,
}

/// What a migration did: the copy, and whether upstream's settings lost fork keys.
#[derive(Debug, PartialEq)]
pub struct Outcome {
    pub copy: Copy,
    pub repaired: bool,
}

/// Migrates from the platform's config folder; never fails the start.
pub fn run() {
    let Some(base) = dirs::config_dir() else {
        log::warn!("migrate: the platform names no config folder");
        return;
    };
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_secs());
    migrate(&base.join(UPSTREAM), &base.join(NAME), secs);
}

/// Copies `old`'s settings, workspace and usage into `new` until a copy finishes, then, at every start, strips the fork's keys from `old`'s settings; a failed copy leaves them for the next try.
pub fn migrate(old: &Path, new: &Path, secs: u64) -> Outcome {
    let settings = old.join("settings.json");
    let copy = if new.join(MARKER).exists() || new.join("settings.json").exists() {
        log::debug!("migrate: {} holds its own settings; no copy", new.display());
        Copy::Present
    } else if !settings.is_file() {
        log::info!("migrate: {} holds no settings to take", old.display());
        Copy::Nothing
    } else {
        match copy(old, new) {
            Ok(dropped) => {
                log::info!("migrate: copied {} into {}", old.display(), new.display());
                Copy::Copied { dropped }
            }
            Err(error) => {
                log::warn!("{error:#}");
                Copy::Failed
            }
        }
    };
    let repaired = match &copy {
        Copy::Failed => {
            log::warn!(
                "migrate: {} keeps the fork's keys until a copy takes them",
                settings.display()
            );
            false
        }
        _ if !settings.is_file() => false,
        _ => repair(&settings, secs).unwrap_or_else(|error| {
            log::warn!("{error:#}");
            false
        }),
    };
    Outcome { copy, repaired }
}

/// Copies into `new` what it lacks, each piece whole through a temp name, settings last, then the marker; the settings keys dropped.
fn copy(old: &Path, new: &Path) -> anyhow::Result<Vec<String>> {
    let temp = |name: &str| new.join(format!("{name}.migrating-{}", std::process::id()));
    let copied = (|| {
        fs::create_dir_all(new).with_context(|| format!("cannot create {}", new.display()))?;
        let (from, to) = (old.join("workspace.json"), new.join("workspace.json"));
        if from.is_file() && !to.exists() {
            fs::copy(&from, temp("workspace.json"))
                .with_context(|| format!("cannot copy {}", from.display()))?;
            fs::rename(temp("workspace.json"), &to)
                .with_context(|| format!("cannot rename into {}", to.display()))?;
        }
        let (from, to) = (old.join("usage"), new.join("usage"));
        if from.is_dir() && !to.exists() {
            tree(&from, &temp("usage"))?;
            fs::rename(temp("usage"), &to)
                .with_context(|| format!("cannot rename into {}", to.display()))?;
            log::info!(
                "migrate: usage copied; a statusLine wrapper put in before still runs from {}, until Settings › Usage applies it again",
                from.display()
            );
        }
        let from = old.join("settings.json");
        let text =
            fs::read_to_string(&from).with_context(|| format!("cannot read {}", from.display()))?;
        let (text, dropped) = known(text)?;
        crate::atomic::write(&new.join("settings.json"), &text, "migrate")?;
        fs::write(new.join(MARKER), format!("copied from {}\n", old.display()))
            .with_context(|| format!("cannot write {}", new.join(MARKER).display()))?;
        anyhow::Ok(dropped)
    })();
    if copied.is_err() {
        for name in ["workspace.json", "usage"] {
            let left = temp(name);
            let gone = match left.is_dir() {
                true => fs::remove_dir_all(&left),
                false => fs::remove_file(&left),
            };
            if left.exists()
                && let Err(error) = gone
            {
                log::warn!("migrate: {} stays behind: {error}", left.display());
            }
        }
    }
    copied.context("migrate: the copy failed; the fork starts fresh and tries again next start")
}

/// Copies folder `from` to `to`, each file and folder with its permissions; links pass by.
fn tree(from: &Path, to: &Path) -> anyhow::Result<()> {
    fs::create_dir(to).with_context(|| format!("cannot create {}", to.display()))?;
    for entry in fs::read_dir(from).with_context(|| format!("cannot read {}", from.display()))? {
        let entry = entry.with_context(|| format!("cannot read {}", from.display()))?;
        let (path, kind) = (entry.path(), entry.file_type()?);
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            tree(&path, &target)?;
        } else if kind.is_file() {
            // fs::copy keeps the file's permissions.
            fs::copy(&path, &target).with_context(|| format!("cannot copy {}", path.display()))?;
        } else {
            log::warn!("migrate: {} is no file or folder; left out", path.display());
        }
    }
    let mode = fs::metadata(from)?.permissions();
    fs::set_permissions(to, mode).with_context(|| format!("cannot set {}'s mode", to.display()))
}

/// `text` without the top-level keys the fork's settings do not know, which would stop its start; a file that is no JSON object stays as it is.
fn known(text: String) -> anyhow::Result<(String, Vec<String>)> {
    let Ok(serde_json::Value::Object(mut map)) = serde_json::from_str(&text) else {
        return Ok((text, Vec::new()));
    };
    let fields = serde_json::to_value(crate::settings::Settings::default())
        .context("migrate: the settings do not encode")?;
    let dropped: Vec<String> = (map.keys())
        .filter(|key| fields.get(key.as_str()).is_none())
        .cloned()
        .collect();
    if dropped.is_empty() {
        return Ok((text, dropped));
    }
    log::warn!(
        "migrate: the fork knows no {}; left out of its copy",
        dropped.join(", ")
    );
    for key in &dropped {
        map.shift_remove(key);
    }
    let mut text = serde_json::to_string_pretty(&map).context("migrate: cannot encode")?;
    text.push('\n');
    Ok((text, dropped))
}

/// Removes the fork's keys from `path` after a backup beside it; a file that is no JSON object stays.
fn repair(path: &Path, secs: u64) -> anyhow::Result<bool> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("migrate: cannot read {}", path.display()))?;
    let Ok(serde_json::Value::Object(mut map)) = serde_json::from_str(&text) else {
        log::warn!("migrate: {} is no JSON object; left alone", path.display());
        return Ok(false);
    };
    let found: Vec<&str> = FORK_KEYS
        .into_iter()
        .filter(|key| map.contains_key(*key))
        .collect();
    if found.is_empty() {
        return Ok(false);
    }
    let backup = backup(path, secs, &text)?;
    for key in &found {
        map.shift_remove(*key);
    }
    let mut text = serde_json::to_string_pretty(&map).context("migrate: cannot encode")?;
    text.push('\n');
    crate::atomic::write(path, &text, "migrate")?;
    log::info!(
        "migrate: {} lost {} (backup {})",
        path.display(),
        found.join(", "),
        backup.display()
    );
    Ok(true)
}

/// Writes `text` to a new backup beside `path`, named by `secs`, a count added where one stands.
fn backup(path: &Path, secs: u64, text: &str) -> anyhow::Result<PathBuf> {
    for n in 0..100 {
        let name = match n {
            0 => format!("settings.json.insiro-{secs}.bak"),
            n => format!("settings.json.insiro-{secs}-{n}.bak"),
        };
        let backup = path.with_file_name(name);
        let opened = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&backup);
        let mut file = match opened {
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            opened => {
                opened.with_context(|| format!("migrate: cannot write {}", backup.display()))?
            }
        };
        file.write_all(text.as_bytes())
            .and_then(|()| file.sync_all())
            .with_context(|| format!("migrate: cannot write {}", backup.display()))?;
        return Ok(backup);
    }
    anyhow::bail!(
        "migrate: 100 backups of {} stand for {secs}",
        path.display()
    )
}
