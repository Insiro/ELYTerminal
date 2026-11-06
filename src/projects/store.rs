//! `workspace.json`: the projects and tabs kept between runs.
use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Context as _;
use serde::{Deserialize, Serialize};

use super::{Home, Worktree, claude};

const VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub version: u32,
    pub selected: Home,
    pub projects: Vec<SavedProject>,
    pub others: SavedSpace,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedProject {
    pub folder: PathBuf,
    pub name: Option<String>,
    pub open: bool,
    pub worktrees: Vec<Worktree>,
    pub spaces: Vec<SavedTree>,
}

/// The tabs of one worktree.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedTree {
    pub path: PathBuf,
    pub shown: usize,
    pub tabs: Vec<SavedTab>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedSpace {
    pub shown: usize,
    pub tabs: Vec<SavedTab>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SavedTab {
    Terminal {
        name: String,
        shell: String,
        args: Vec<String>,
        /// The folder it started in; none for home, which `wsl` takes as its distribution's.
        folder: Option<PathBuf>,
        claude: Option<String>,
    },
    /// A document tab of the file at `path`.
    Doc { path: PathBuf },
}

/// What the file gave.
#[derive(Debug, PartialEq)]
pub enum Loaded {
    /// There is no file.
    Fresh(State),
    Ok(State),
    /// The file was unusable and moved to `backup`.
    Corrupt {
        backup: PathBuf,
    },
}

/// `workspace.json` in the platform config folder, beside `settings.json`.
pub fn default_path() -> anyhow::Result<PathBuf> {
    Ok(crate::identity::config_dir()?.join("workspace.json"))
}

impl State {
    pub fn empty() -> Self {
        Self {
            version: VERSION,
            selected: Home::Others,
            projects: Vec::new(),
            others: SavedSpace::default(),
        }
    }

    /// The first thing wrong with the state, if any.
    fn check(&self) -> Result<(), String> {
        if self.version != VERSION {
            return Err(format!("version {} is not {VERSION}", self.version));
        }
        if let Home::Tree { project, path } = &self.selected {
            absolute(project)?;
            absolute(path)?;
        }
        for project in &self.projects {
            absolute(&project.folder)?;
            for tree in &project.worktrees {
                absolute(&tree.path)?;
            }
            for tree in &project.spaces {
                absolute(&tree.path)?;
                check_tabs(&tree.tabs)?;
            }
        }
        check_tabs(&self.others.tabs)
    }
}

fn check_tabs(tabs: &[SavedTab]) -> Result<(), String> {
    for tab in tabs {
        match tab {
            SavedTab::Terminal { folder, claude, .. } => {
                if let Some(folder) = folder {
                    absolute(folder)?;
                }
                if let Some(id) = claude
                    && !claude::valid(id)
                {
                    return Err(format!("claude id `{id}` is not a UUID"));
                }
            }
            SavedTab::Doc { path } => absolute(path)?,
        }
    }
    Ok(())
}

fn absolute(path: &Path) -> Result<(), String> {
    if path.is_absolute() {
        Ok(())
    } else {
        Err(format!("path {} is not absolute", path.display()))
    }
}

/// Reads the file; an unusable one is moved aside and reported, a file that cannot be read is an error.
pub fn load(path: &Path) -> anyhow::Result<Loaded> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            log::info!("workspace: no file at {}, fresh", path.display());
            return Ok(Loaded::Fresh(State::empty()));
        }
        Err(error) => {
            return Err(error)
                .with_context(|| format!("workspace: cannot read {}", path.display()));
        }
    };
    let state = serde_json::from_str::<State>(&text)
        .map_err(|error| error.to_string())
        .and_then(|state| state.check().map(|()| state));
    match state {
        Ok(state) => {
            log::info!("workspace: loaded {}", path.display());
            Ok(Loaded::Ok(state))
        }
        Err(reason) => {
            let secs = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |time| time.as_secs());
            let backup = path.with_file_name(format!("workspace.json.broken-{secs}"));
            log::error!(
                "workspace: {} is unusable ({reason}), moved to {}",
                path.display(),
                backup.display()
            );
            fs::rename(path, &backup).with_context(|| {
                format!(
                    "workspace: cannot move {} to {}",
                    path.display(),
                    backup.display()
                )
            })?;
            Ok(Loaded::Corrupt { backup })
        }
    }
}

/// Saves the state through a temp file renamed over `path`.
pub fn save(path: &Path, state: &State) -> anyhow::Result<()> {
    let mut text = serde_json::to_string_pretty(state).context("workspace: cannot encode")?;
    text.push('\n');
    crate::atomic::write(path, &text, "workspace")?;
    log::info!("workspace: saved {}", path.display());
    Ok(())
}
