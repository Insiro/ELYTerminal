//! The fork's own name, app id and folders, apart from upstream's ELYTerminal.

use std::path::PathBuf;

use anyhow::Context as _;

pub mod migrate;
#[cfg(test)]
mod tests;

/// The name the fork shows and its folders take.
pub const NAME: &str = "ELYTerminal Insiro";
/// The folder name upstream's ELYTerminal keeps, which the fork once shared.
pub const UPSTREAM: &str = "ELYTerminal";
/// The id the macOS bundle, the Linux desktop entry and Windows' notifications share.
pub const APP_ID: &str = "com.insiro.elyterminal";

/// The fork's folder in the platform's config folder: settings, `workspace.json`, usage.
pub fn config_dir() -> anyhow::Result<PathBuf> {
    Ok(dirs::config_dir()
        .context("identity: the platform names no config folder")?
        .join(NAME))
}

/// The fork's folder in the platform's local data folder: the shell integration.
pub fn data_dir() -> anyhow::Result<PathBuf> {
    Ok(dirs::data_local_dir()
        .context("identity: the platform names no data folder")?
        .join(NAME))
}
