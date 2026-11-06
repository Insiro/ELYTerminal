//! Once, at start: the fork's files copied out of upstream's folder, and upstream's settings cleared of the fork's keys.

use std::{
    fs,
    io::Write as _,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Context as _;

use super::{NAME, UPSTREAM};

/// Settings keys only the fork knows, which upstream's `deny_unknown_fields` refuses.
pub const FORK_KEYS: [&str; 6] = [
    "claude_prelude",
    "codex_usage",
    "claude_api_usage",
    "codex_live_usage",
    "usage_summary",
    "git_tree",
];

/// The files copied into the fork's folder.
const COPIED: [&str; 2] = ["settings.json", "workspace.json"];

/// What a migration did.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// The fork's folder already stood: it ran before.
    Present,
    /// Upstream's folder holds no settings to take.
    Nothing,
    /// The files were copied; `repaired` says whether upstream's settings lost fork keys.
    Copied { repaired: bool },
    /// The copy failed; the fork starts fresh.
    Failed,
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

/// Copies `old`'s settings and workspace into a new `new`, then strips the fork's keys from `old`'s settings.
pub fn migrate(old: &Path, new: &Path, secs: u64) -> Outcome {
    if new.exists() {
        log::debug!("migrate: {} stands; nothing to do", new.display());
        return Outcome::Present;
    }
    if !old.join("settings.json").is_file() {
        log::info!("migrate: {} holds no settings to take", old.display());
        return Outcome::Nothing;
    }
    if let Err(error) = copy(old, new) {
        log::warn!("{error:#}");
        return Outcome::Failed;
    }
    log::info!("migrate: copied {} into {}", old.display(), new.display());
    let repaired = match repair(&old.join("settings.json"), secs) {
        Ok(repaired) => repaired,
        Err(error) => {
            log::warn!("{error:#}");
            false
        }
    };
    Outcome::Copied { repaired }
}

/// Copies the files into a staging folder renamed to `new`, so `new` stands whole or not at all.
fn copy(old: &Path, new: &Path) -> anyhow::Result<()> {
    let stage = new.with_file_name(format!("{NAME}.migrating-{}", std::process::id()));
    let staged = (|| {
        if stage.exists() {
            fs::remove_dir_all(&stage)
                .with_context(|| format!("cannot clear {}", stage.display()))?;
        }
        fs::create_dir_all(&stage).with_context(|| format!("cannot create {}", stage.display()))?;
        for name in COPIED {
            let from = old.join(name);
            if from.is_file() {
                fs::copy(&from, stage.join(name))
                    .with_context(|| format!("cannot copy {}", from.display()))?;
            }
        }
        fs::rename(&stage, new)
            .with_context(|| format!("cannot rename {} to {}", stage.display(), new.display()))
    })();
    if staged.is_err()
        && stage.exists()
        && let Err(left) = fs::remove_dir_all(&stage)
    {
        log::warn!("migrate: {} stays behind: {left}", stage.display());
    }
    staged.context("migrate: the copy failed; the fork starts fresh")
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
    let backup = path.with_file_name(format!("settings.json.insiro-{secs}.bak"));
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&backup)
        .and_then(|mut file| {
            file.write_all(text.as_bytes())
                .and_then(|()| file.sync_all())
        })
        .with_context(|| format!("migrate: cannot write {}", backup.display()))?;
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
