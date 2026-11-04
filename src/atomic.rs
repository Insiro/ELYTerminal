use std::{
    fs,
    io::Write,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

use anyhow::Context as _;

/// Writes a temp file beside `path`, then renames it over `path`; `owner` names the caller in errors.
pub fn write(path: &Path, text: &str, owner: &str) -> anyhow::Result<()> {
    let folder = path
        .parent()
        .with_context(|| format!("{owner}: {} has no folder", path.display()))?;
    fs::create_dir_all(folder)
        .with_context(|| format!("{owner}: cannot create {}", folder.display()))?;
    let count = SAVES.fetch_add(1, Ordering::Relaxed);
    let name = path
        .file_name()
        .with_context(|| format!("{owner}: {} has no file name", path.display()))?
        .to_string_lossy();
    let temp = path.with_file_name(format!("{name}.{}-{count}.tmp", std::process::id()));
    let written = write_new(&temp, text, owner).and_then(|()| {
        fs::rename(&temp, path).with_context(|| {
            format!(
                "{owner}: cannot rename {} over {}",
                temp.display(),
                path.display()
            )
        })
    });
    if let Err(error) = written {
        if temp.exists()
            && let Err(left) = fs::remove_file(&temp)
        {
            log::error!("{owner}: {} stays behind: {left}", temp.display());
        }
        return Err(error);
    }
    Ok(())
}

/// Creates new `temp` and writes `text` to the disk.
fn write_new(temp: &Path, text: &str, owner: &str) -> anyhow::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp)
        .with_context(|| format!("{owner}: cannot create {}", temp.display()))?;
    file.write_all(text.as_bytes())
        .and_then(|()| file.sync_all())
        .with_context(|| format!("{owner}: cannot write {}", temp.display()))
}

/// Saves made by this process, so each temp file has its own name.
static SAVES: AtomicU64 = AtomicU64::new(0);
