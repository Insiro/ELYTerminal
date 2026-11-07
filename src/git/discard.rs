//! Discarding changes from the work tree, one or a folder's at once.

use std::ffi::OsStr;

use anyhow::{bail, ensure};
use ely_gpui_component::{git::ChangeSection, lists::GitStatus};

use super::{Change, Path, names, pathspecs, run};

/// The arguments that delete files only intended to be added, or renamed in the work tree.
const RM: [&str; 5] = [
    "rm",
    "-f",
    "-q",
    "--pathspec-from-file=-",
    "--pathspec-file-nul",
];

/// The arguments that put edits back to the index.
const RESTORE: [&str; 4] = [
    "restore",
    "--worktree",
    "--pathspec-from-file=-",
    "--pathspec-file-nul",
];

/// Runs `git clean` on `changes`; an error when it removed nothing, as git passes over a repository of its own without a word.
fn clean(top: &Path, changes: &[Change]) -> anyhow::Result<()> {
    // No path would take the whole tree.
    ensure!(!changes.is_empty(), "git: no file named");
    let flags = ["clean", "-f", "-d", "--"].map(OsStr::new);
    let paths = changes.iter().map(|change| change.path.as_os_str());
    let args: Vec<&OsStr> = flags.into_iter().chain(paths).collect();
    let removed = run(top, &args, &[])?;
    ensure!(
        !removed.is_empty(),
        "git: clean removed nothing at {}",
        names(changes)
    );
    Ok(())
}

/// Discards `changes` from the work tree, a file's or a folder's: edits go back to the index, new files go; one `git` call for each kind, every kind runs, and what failed is named.
pub fn discard(top: &Path, changes: &[Change], section: ChangeSection) -> anyhow::Result<()> {
    let mut failed = Vec::new();
    let mut done = 0;
    let mut step =
        |kind: &str, group: Vec<Change>, work: &dyn Fn(&[Change]) -> anyhow::Result<()>| {
            if group.is_empty() {
                return;
            }
            match work(&group) {
                Ok(()) => done += group.len(),
                Err(error) => failed.push(format!("{kind} of {}: {error:#}", names(&group))),
            }
        };
    match section {
        ChangeSection::Untracked => step("clean", changes.to_vec(), &|group| clean(top, group)),
        ChangeSection::Unstaged => {
            let (new, edited): (Vec<Change>, Vec<Change>) = changes
                .iter()
                .cloned()
                .partition(|change| matches!(change.status, GitStatus::Added | GitStatus::Renamed));
            let rm = |group: &[Change]| run(top, &RM, &pathspecs(group)?).map(drop);
            let restore = |group: &[Change]| run(top, &RESTORE, &pathspecs(group)?).map(drop);
            step("restore", edited, &restore);
            step("rm", new, &rm);
        }
        _ => bail!("git: nothing to discard in {section:?}"),
    }
    if !failed.is_empty() {
        let total = changes.len();
        bail!(
            "git: {done} of {total} discarded in {}; {}",
            top.display(),
            failed.join("; ")
        );
    }
    log::info!("git: discarded {} in {}", names(changes), top.display());
    Ok(())
}
