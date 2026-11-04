//! A document's file read off the main thread, and the stamp that says it changed.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read as _},
    path::Path,
    time::SystemTime,
};

use super::kind::{Kind, binary};

/// What a file's metadata says of it, so a poll reads it again only once it changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamp {
    pub mtime: Option<SystemTime>,
    pub len: u64,
}

/// `path`'s stamp; `None` while it is missing.
pub fn stamp(path: &Path) -> io::Result<Option<Stamp>> {
    match fs::metadata(path) {
        Ok(meta) => Ok(Some(Stamp {
            mtime: meta.modified().ok(),
            len: meta.len(),
        })),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// What a read found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Read {
    Text(String),
    /// An image within its limit, which the image loader reads itself.
    Image,
    /// A NUL, or bytes not in UTF-8.
    Binary,
    /// Its size, past its kind's limit.
    TooLarge(u64),
    /// It did not read, gone since its stamp among the reasons.
    Failed(String),
}

/// Opens `path` without waiting on a pipe or device behind it.
fn open(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    options.open(path)
}

/// Reads `path` as `kind` shows it, no more than its limit and one byte.
pub fn read(path: &Path, kind: Kind) -> Read {
    let failed = |error: io::Error| Read::Failed(error.to_string());
    let file = match open(path) {
        Ok(file) => file,
        Err(error) => return failed(error),
    };
    let meta = match file.metadata() {
        Ok(meta) => meta,
        Err(error) => return failed(error),
    };
    if !meta.is_file() {
        return Read::Failed(format!("{} is not a file", path.display()));
    }
    if meta.len() > kind.max() {
        return Read::TooLarge(meta.len());
    }
    if kind == Kind::Image {
        return Read::Image;
    }
    let mut bytes = Vec::new();
    if let Err(error) = file.take(kind.max() + 1).read_to_end(&mut bytes) {
        return failed(error);
    }
    // It may have grown since its metadata read.
    if bytes.len() as u64 > kind.max() {
        return Read::TooLarge(bytes.len() as u64);
    }
    if binary(&bytes) {
        return Read::Binary;
    }
    String::from_utf8(bytes).map_or(Read::Binary, Read::Text)
}

/// What a poll found, against the stamp last read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Found {
    Same,
    Gone,
    Changed(Stamp, Read),
    Failed(String),
}

/// Reads `path` again only when its stamp is not `known`.
pub fn look(path: &Path, kind: Kind, known: Option<Stamp>) -> Found {
    match stamp(path) {
        Err(error) => Found::Failed(error.to_string()),
        Ok(None) => Found::Gone,
        Ok(Some(now)) if Some(now) == known => Found::Same,
        Ok(Some(now)) => Found::Changed(now, read(path, kind)),
    }
}
