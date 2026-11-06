use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, bail};

use super::{git, path_of, run, said};

/// A worktree `git worktree list` names.
#[derive(Debug, Clone, PartialEq)]
pub struct Worktree {
    pub path: PathBuf,
    /// The branch, `refs/heads/` stripped.
    pub branch: Option<String>,
    pub head: Option<String>,
    pub detached: bool,
    pub prunable: bool,
    pub locked: bool,
}

impl Worktree {
    /// The branch, else the head's first 7 characters, else `?`.
    pub fn label(&self) -> String {
        match (&self.branch, &self.head) {
            (Some(branch), _) => branch.clone(),
            (None, Some(head)) => head.chars().take(7).collect(),
            (None, None) => "?".into(),
        }
    }
}

/// The worktrees of the repository whose top is `folder`, the main one first; none for a plain folder, a subfolder of a work tree included.
pub fn worktrees(folder: &Path) -> anyhow::Result<Option<Vec<Worktree>>> {
    let answer = git(folder, &["rev-parse", "--show-toplevel"], &[])?;
    if !answer.status.success() {
        let error = said(&answer);
        if error.contains("not a git repository") || error.contains("must be run in a work tree") {
            return Ok(None);
        }
        bail!("git: rev-parse in {} failed: {error}", folder.display());
    }
    let top = answer.stdout.strip_suffix(b"\n").with_context(|| {
        let printed = String::from_utf8_lossy(&answer.stdout);
        format!("git: rev-parse in {} printed {printed:?}", folder.display())
    })?;
    let top = path_of(top)?;
    #[cfg(windows)]
    let top = match super::wsl::split(folder) {
        Some((share, _)) => super::wsl::join(share, &top),
        None => top,
    };
    let same = |path: &Path| {
        path.canonicalize()
            .with_context(|| format!("git: {} cannot be resolved", path.display()))
    };
    if same(&top)? != same(folder)? {
        return Ok(None);
    }
    let list = ["worktree", "list", "--porcelain"];
    let with_z = ["worktree", "list", "--porcelain", "-z"];
    let answer = git(folder, &with_z, &[])?;
    let (bytes, nul) = if answer.status.success() {
        (answer.stdout, true)
    } else {
        // git before 2.36 knows no -z.
        let error = said(&answer);
        if !(error.contains("unknown switch")
            || error.contains("unknown option")
            || error.contains("usage:"))
        {
            bail!(
                "git: `git worktree list` failed in {}: {error}",
                folder.display()
            );
        }
        (run(folder, &list, &[])?, false)
    };
    #[allow(unused_mut)]
    let mut found = parse(&bytes, nul)?;
    #[cfg(windows)]
    if let Some((share, _)) = super::wsl::split(folder) {
        for worktree in &mut found {
            worktree.path = super::wsl::join(share, &worktree.path);
        }
    }
    log::debug!(
        "git: {} worktrees listed in {}",
        found.len(),
        folder.display()
    );
    Ok(Some(found))
}

/// The worktree at `tree` holds changes or untracked files.
pub fn worktree_dirty(tree: &Path) -> anyhow::Result<bool> {
    let args = ["status", "--porcelain", "--untracked-files=normal"];
    Ok(!run(tree, &args, &[])?.is_empty())
}

/// Removes the linked worktree `tree` through the repository at `main`, its changes too with `force`; its branch stays.
pub fn remove_worktree(main: &Path, tree: &Path, force: bool) -> anyhow::Result<()> {
    #[cfg(windows)]
    let tree = &match super::wsl::split(tree) {
        Some((_, inside)) => PathBuf::from(inside),
        None => tree.to_path_buf(),
    };
    let mut args = vec![OsStr::new("worktree"), OsStr::new("remove")];
    if force {
        args.push(OsStr::new("--force"));
    }
    args.push(tree.as_os_str());
    run(main, &args, &[])?;
    log::info!("git: worktree {} removed, force {force}", tree.display());
    Ok(())
}

/// Reads `git worktree list --porcelain` output, NUL-ended fields or lines; a bare entry is left out.
pub(crate) fn parse(bytes: &[u8], nul: bool) -> anyhow::Result<Vec<Worktree>> {
    let sep = if nul { 0 } else { b'\n' };
    let mut found = Vec::new();
    let mut current: Option<Worktree> = None;
    let mut bare = false;
    let mut end = |current: &mut Option<Worktree>, bare: &mut bool| {
        if let Some(worktree) = current.take()
            && !*bare
        {
            found.push(worktree);
        }
        *bare = false;
    };
    for field in bytes.split(|&byte| byte == sep) {
        if field.is_empty() {
            end(&mut current, &mut bare);
            continue;
        }
        let (key, value) = match field.iter().position(|&byte| byte == b' ') {
            Some(at) => (&field[..at], Some(&field[at + 1..])),
            None => (field, None),
        };
        let text = |value: Option<&[u8]>| value.map(|v| String::from_utf8_lossy(v).into_owned());
        if key == b"worktree" {
            end(&mut current, &mut bare);
            let value = value.context("git: worktree record without a path")?;
            current = Some(Worktree {
                path: path_of(value)?,
                branch: None,
                head: None,
                detached: false,
                prunable: false,
                locked: false,
            });
            continue;
        }
        let Some(worktree) = current.as_mut() else {
            bail!(
                "git: worktree field {:?} before any path",
                String::from_utf8_lossy(field)
            );
        };
        match key {
            b"HEAD" => worktree.head = text(value),
            b"branch" => {
                let branch = text(value).unwrap_or_default();
                worktree.branch =
                    Some(branch.strip_prefix("refs/heads/").unwrap_or(&branch).into());
            }
            b"detached" => worktree.detached = true,
            b"bare" => bare = true,
            b"locked" => worktree.locked = true,
            b"prunable" => worktree.prunable = true,
            _ => {}
        }
    }
    end(&mut current, &mut bare);
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn records(sep: &str) -> Vec<u8> {
        let end = if sep == "\0" { "\0" } else { "" };
        let lines = [
            "worktree /p/my app",
            "HEAD aaaaaaaaaaaa",
            "branch refs/heads/main",
            "",
            "worktree /p/linked",
            "HEAD bbbbbbbbbbbb",
            "branch refs/heads/feat/x",
            "locked why",
            "",
            "worktree /p/det",
            "HEAD ccccccccccccc",
            "detached",
            "prunable gone",
            "",
            "worktree /p/bare.git",
            "bare",
            "",
        ];
        let mut out = lines.join(sep);
        out.push_str(sep);
        out.push_str(end);
        out.into_bytes()
    }

    fn check(nul: bool, sep: &str) {
        let found = parse(&records(sep), nul).unwrap();
        assert_eq!(found.len(), 3);
        assert_eq!(found[0].path, PathBuf::from("/p/my app"));
        assert_eq!(found[0].label(), "main");
        assert_eq!(found[1].branch.as_deref(), Some("feat/x"));
        assert!(found[1].locked && !found[1].prunable);
        assert!(found[2].detached && found[2].prunable && found[2].branch.is_none());
        assert_eq!(found[2].label(), "ccccccc");
    }

    #[test]
    fn nul_records() {
        check(true, "\0");
    }

    #[test]
    fn newline_records() {
        check(false, "\n");
    }

    #[test]
    fn field_before_path_fails() {
        assert!(parse(b"HEAD abc\n", false).is_err());
    }
}
