//! The statusLine wrapper: written to the app's folder, put into Claude's settings only on Apply, and taken back.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{
    atomic,
    docs::read::{Stamp, stamp},
};

pub const SCRIPT: &str = include_str!("../../assets/usage/claude-statusline.sh");

/// Claude's settings file and the app's usage folder.
#[derive(Clone, Debug, PartialEq)]
pub struct Install {
    /// Claude's `settings.json`; none where no home is named.
    pub settings: Option<PathBuf>,
    pub usage: PathBuf,
}

/// Where the wrapper stands.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Off,
    On,
    /// Ours was put in, then the line was changed by someone else.
    Elsewhere,
    /// No Claude settings file to change.
    Missing,
    /// The settings file did not read as JSON settings.
    Error(String),
}

/// What Apply writes, shown before it does.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub path: PathBuf,
    pub before: Option<Value>,
    pub after: Value,
}

impl Plan {
    pub fn shown(value: Option<&Value>) -> String {
        value.map_or_else(|| "(none)".into(), pretty)
    }
}

/// What the app keeps of its own change, so it can take it back.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    settings: PathBuf,
    original: Option<Value>,
    installed: String,
    at: i64,
}

impl Install {
    pub fn script(&self) -> PathBuf {
        self.usage.join("claude-statusline.sh")
    }

    fn inner(&self) -> PathBuf {
        self.usage.join("claude-inner")
    }

    pub fn state(&self) -> PathBuf {
        self.usage.join("claude-install.json")
    }

    fn data(&self) -> PathBuf {
        self.usage.join("claude-rate.json")
    }

    /// The command Claude runs: the script's path, quoted for its shell.
    pub fn command(&self) -> anyhow::Result<String> {
        let script = self.script();
        let text = script
            .to_str()
            .with_context(|| format!("usage: {} is not UTF-8", script.display()))?;
        ensure!(
            !text.contains(['"', '$', '`', '\\']),
            "usage: {text} holds a character the shell would read"
        );
        Ok(format!("\"{text}\""))
    }

    fn settings_path(&self) -> anyhow::Result<&Path> {
        self.settings
            .as_deref()
            .context("usage: no home names Claude's settings")
    }

    pub fn status(&self) -> Status {
        let Some(path) = &self.settings else {
            return Status::Missing;
        };
        let line = match read(path) {
            Ok(Some((settings, _))) => settings.get("statusLine").cloned(),
            Ok(None) => return Status::Missing,
            Err(error) => return Status::Error(format!("{error:#}")),
        };
        match self.kept() {
            Ok(None) => Status::Off,
            Ok(Some(state)) if command(line.as_ref()) == Some(&state.installed) => Status::On,
            Ok(Some(_)) => Status::Elsewhere,
            Err(error) => Status::Error(format!("{error:#}")),
        }
    }

    fn kept(&self) -> anyhow::Result<Option<State>> {
        let path = self.state();
        match fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text)
                .map(Some)
                .with_context(|| format!("usage: {} is not the app's state", path.display())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => {
                Err(error).with_context(|| format!("usage: cannot read {}", path.display()))
            }
        }
    }

    /// The line as it stands and as Apply would leave it.
    pub fn plan(&self) -> anyhow::Result<Plan> {
        let path = self.settings_path()?;
        let (settings, _) =
            read(path)?.with_context(|| format!("usage: {} is missing", path.display()))?;
        let before = settings.get("statusLine").cloned();
        let installed = self.command()?;
        let mut after = match &before {
            None => Map::new(),
            Some(Value::Object(line)) => line.clone(),
            Some(_) => bail!("usage: {}'s statusLine is not an object", path.display()),
        };
        if let Some(before) = &before {
            ensure!(
                before.get("type").and_then(Value::as_str) == Some("command"),
                "usage: {}'s statusLine is not a command",
                path.display()
            );
            ensure!(
                command(Some(before)) != Some(&installed),
                "usage: {}'s statusLine runs the wrapper already",
                path.display()
            );
        }
        after.insert("type".into(), "command".into());
        after.insert("command".into(), installed.into());
        Ok(Plan {
            path: path.to_path_buf(),
            before,
            after: Value::Object(after),
        })
    }

    /// Writes the script, the original line and the state, not Claude's settings.
    pub fn prepare(&self, plan: &Plan, now: i64) -> anyhow::Result<()> {
        fs::create_dir_all(&self.usage)
            .with_context(|| format!("usage: cannot create {}", self.usage.display()))?;
        private(&self.usage, 0o700)?;
        let script = self.script();
        if fs::read_to_string(&script).ok().as_deref() != Some(SCRIPT) {
            atomic::write(&script, SCRIPT, "usage")?;
        }
        private(&script, 0o700)?;
        let inner = command(plan.before.as_ref()).cloned().unwrap_or_default();
        put(&self.inner(), &inner)?;
        let state = State {
            settings: plan.path.clone(),
            original: plan.before.clone(),
            installed: self.command()?,
            at: now,
        };
        put(&self.state(), &pretty(&serde_json::to_value(state)?))?;
        Ok(())
    }

    /// Puts the wrapper into Claude's settings, as `plan` showed, or writes nothing.
    pub fn apply(&self, plan: &Plan, now: i64) -> anyhow::Result<()> {
        self.apply_with(plan, now, || {})
    }

    /// As `apply`; `meddle` runs just before the rename, so a test may change the file there.
    pub fn apply_with(&self, plan: &Plan, now: i64, meddle: impl FnOnce()) -> anyhow::Result<()> {
        let (settings, seen) = read(&plan.path)?
            .with_context(|| format!("usage: {} is missing", plan.path.display()))?;
        ensure!(
            settings.get("statusLine") == plan.before.as_ref(),
            "usage: {}'s statusLine changed since you opened this",
            plan.path.display()
        );
        self.prepare(plan, now)?;
        let written = rewrite(
            &plan.path,
            settings,
            Some(plan.after.clone()),
            seen,
            now,
            meddle,
        );
        if let Err(error) = written {
            self.remove(&[self.state(), self.inner()]);
            return Err(error);
        }
        log::info!("usage: claude statusLine wrapped ({})", plan.path.display());
        Ok(())
    }

    /// Puts the original line back while ours stands; else writes nothing and says so.
    pub fn revert(&self, now: i64) -> anyhow::Result<Status> {
        let state = self
            .kept()?
            .context("usage: the wrapper was never put in")?;
        let (settings, seen) = read(&state.settings)?
            .with_context(|| format!("usage: {} is missing", state.settings.display()))?;
        if command(settings.get("statusLine")) != Some(&state.installed) {
            log::warn!(
                "usage: {}'s statusLine changed elsewhere; it stays",
                state.settings.display()
            );
            return Ok(Status::Elsewhere);
        }
        rewrite(&state.settings, settings, state.original, seen, now, || {})?;
        self.remove(&[self.state(), self.inner(), self.data()]);
        log::info!(
            "usage: claude statusLine restored ({})",
            state.settings.display()
        );
        Ok(Status::Off)
    }

    /// Removes the app's own files; Claude's settings stay untouched.
    pub fn forget(&self) {
        self.remove(&[self.state(), self.inner(), self.data(), self.script()]);
        log::info!("usage: the wrapper's files are forgotten");
    }

    fn remove(&self, paths: &[PathBuf]) {
        for path in paths {
            match fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => log::error!("usage: {} stays: {error}", path.display()),
            }
        }
    }
}

/// A settings object and the stamp it was read at.
type Seen = (Map<String, Value>, Option<Stamp>);

/// The settings object and its stamp; none when the file is missing.
fn read(path: &Path) -> anyhow::Result<Option<Seen>> {
    let seen = stamp(path).with_context(|| format!("usage: cannot look at {}", path.display()))?;
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| format!("usage: cannot read {}", path.display()));
        }
    };
    match serde_json::from_str(&text) {
        Ok(Value::Object(settings)) => Ok(Some((settings, seen))),
        Ok(_) => bail!("usage: {} is not a JSON object", path.display()),
        Err(error) => Err(error).with_context(|| format!("usage: {} is not JSON", path.display())),
    }
}

/// Backs the file up, then writes it with `line` as its statusLine, unless it changed since `seen`.
fn rewrite(
    path: &Path,
    mut settings: Map<String, Value>,
    line: Option<Value>,
    seen: Option<Stamp>,
    now: i64,
    meddle: impl FnOnce(),
) -> anyhow::Result<()> {
    let name = path
        .file_name()
        .context("usage: settings have no file name")?;
    let (backup, copy) = (1..100)
        .find_map(|n| {
            let tail = if n == 1 {
                String::new()
            } else {
                format!("-{n}")
            };
            let name = format!("{}.elyterminal-{now}{tail}.bak", name.to_string_lossy());
            let backup = path.with_file_name(name);
            let file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&backup);
            match file {
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => None,
                file => Some((backup, file)),
            }
        })
        .context("usage: every backup name is taken")?;
    let mut copy = copy.with_context(|| format!("usage: cannot create {}", backup.display()))?;
    io::copy(&mut fs::File::open(path)?, &mut copy)
        .with_context(|| format!("usage: cannot write {}", backup.display()))?;
    log::info!(
        "usage: {} backed up as {}",
        path.display(),
        backup.display()
    );
    let mode = fs::metadata(path)?.permissions();
    match line {
        Some(line) => settings.insert("statusLine".into(), line),
        None => settings.remove("statusLine"),
    };
    let text = pretty(&Value::Object(settings)) + "\n";
    atomic::write_if(path, &text, "usage", |temp| {
        fs::set_permissions(temp, mode)?;
        meddle();
        let now = stamp(path)?;
        ensure!(
            now == seen,
            "usage: {} changed while it was written; nothing is",
            path.display()
        );
        Ok(())
    })
}

/// Writes `text`, readable by the user alone.
fn put(path: &Path, text: &str) -> anyhow::Result<()> {
    atomic::write(path, text, "usage")?;
    private(path, 0o600)
}

#[cfg(unix)]
fn private(path: &Path, mode: u32) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .with_context(|| format!("usage: cannot set {}'s mode", path.display()))
}

#[cfg(not(unix))]
fn private(_: &Path, _: u32) -> anyhow::Result<()> {
    Ok(())
}

/// A statusLine's command, if it is one.
fn command(line: Option<&Value>) -> Option<&String> {
    match line?.get("command")? {
        Value::String(command) => Some(command),
        _ => None,
    }
}

/// JSON as Claude writes its settings, 2 spaces and the keys in their order.
pub fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).expect("usage: a JSON value always encodes")
}
