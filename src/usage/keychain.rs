//! Claude's OAuth credentials, read only: macOS's Keychain as Claude Code keeps them, else its credentials file.

use std::path::{Path, PathBuf};

use anyhow::Context as _;
use serde::Deserialize;

use super::run::Secret;

#[cfg(target_os = "macos")]
/// The Keychain item Claude Code writes, before its folder's suffix.
const SERVICE: &str = "Claude Code-credentials";

/// A signed-in account's token.
#[derive(Debug, PartialEq)]
pub struct Credentials {
    pub token: Secret,
    /// Unix milliseconds; none when unnamed.
    pub expires_at: Option<i64>,
    pub plan: Option<String>,
}

impl Credentials {
    pub fn expired(&self, now_ms: i64) -> bool {
        self.expires_at.is_some_and(|at| at <= now_ms)
    }
}

#[derive(Deserialize)]
struct File {
    #[serde(rename = "claudeAiOauth")]
    oauth: Option<Oauth>,
}

#[derive(Deserialize)]
struct Oauth {
    #[serde(rename = "accessToken")]
    token: Option<String>,
    #[serde(rename = "expiresAt")]
    expires_at: Option<f64>,
    #[serde(rename = "subscriptionType")]
    plan: Option<String>,
}

/// The token in Claude Code's credentials JSON; none without one.
pub fn parse(text: &str) -> anyhow::Result<Option<Credentials>> {
    let file: File =
        serde_json::from_str(text).context("usage: claude's credentials are not JSON")?;
    let Some(oauth) = file.oauth else {
        return Ok(None);
    };
    Ok(oauth
        .token
        .filter(|token| !token.is_empty())
        .map(|token| Credentials {
            token: Secret(token),
            expires_at: oauth
                .expires_at
                .filter(|at| at.is_finite())
                .map(|at| at as i64),
            plan: oauth.plan.filter(|plan| !plan.is_empty()),
        }))
}

#[cfg(target_os = "macos")]
/// The Keychain services for a `CLAUDE_CONFIG_DIR`: its path and its real path, each hashed as Claude Code hashes it.
pub fn services(config_dir: Option<&Path>) -> Vec<String> {
    let Some(dir) = config_dir else {
        return vec![SERVICE.to_string()];
    };
    let mut paths = vec![dir.to_path_buf()];
    if let Ok(real) = std::fs::canonicalize(dir)
        && real != dir
    {
        paths.push(real);
    }
    paths
        .iter()
        .map(|path| suffixed(&path.to_string_lossy()))
        .collect()
}

#[cfg(target_os = "macos")]
/// `Claude Code-credentials-` and the first 8 hex digits of the NFC path's SHA-256.
pub fn suffixed(dir: &str) -> String {
    use sha2::Digest as _;
    use unicode_normalization::UnicodeNormalization as _;
    let nfc: String = dir.nfc().collect();
    let digest = sha2::Sha256::digest(nfc.as_bytes());
    let hex: String = digest[..4].iter().map(|b| format!("{b:02x}")).collect();
    format!("{SERVICE}-{hex}")
}

#[cfg(target_os = "macos")]
/// The Keychain's account: the user's name, as Claude Code names it.
pub fn account(user: Option<&str>) -> String {
    match user {
        Some(name)
            if !name.is_empty()
                && name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)) =>
        {
            name.to_string()
        }
        _ => "claude-code-user".to_string(),
    }
}

/// Where the credentials file lies: the config folder's `.credentials.json`.
pub fn file(config_dir: &Path) -> PathBuf {
    config_dir.join(".credentials.json")
}

/// The token from the Keychain, then from the file; none when neither holds one.
pub fn read() -> anyhow::Result<Option<Credentials>> {
    let named = std::env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from);
    if let Some(found) = keychain(named.as_deref())? {
        log::debug!("usage: claude's token is the Keychain's");
        return Ok(Some(found));
    }
    let Some(dir) = crate::projects::claude::config_dir() else {
        return Ok(None);
    };
    let path = file(&dir);
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            log::debug!("usage: claude's token is {}'s", path.display());
            parse(&text)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("usage: {}", path.display())),
    }
}

#[cfg(target_os = "macos")]
fn keychain(config_dir: Option<&Path>) -> anyhow::Result<Option<Credentials>> {
    use std::{process::Command, time::Duration};
    let account = account(std::env::var("USER").ok().as_deref());
    for service in services(config_dir) {
        let mut command = Command::new("/usr/bin/security");
        command.args([
            "find-generic-password",
            "-s",
            &service,
            "-a",
            &account,
            "-w",
        ]);
        let output = super::run::run(command, b"", Duration::from_secs(3))
            .context("usage: the Keychain did not answer")?;
        // 44: no such item.
        if output.code == Some(44) {
            continue;
        }
        anyhow::ensure!(
            output.ok,
            "usage: the Keychain refused {service}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        let text = String::from_utf8(output.stdout).context("usage: keychain item not UTF-8")?;
        if let Some(found) = parse(text.trim())? {
            return Ok(Some(found));
        }
    }
    Ok(None)
}

#[cfg(not(target_os = "macos"))]
fn keychain(_: Option<&Path>) -> anyhow::Result<Option<Credentials>> {
    Ok(None)
}
