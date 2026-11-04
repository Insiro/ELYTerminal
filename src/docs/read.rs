//! A document's file read off the main thread, and the stamp that says it changed.

use std::{fs, io, path::Path, time::SystemTime};

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
    Missing,
    Failed(String),
}

/// Reads `path` as `kind` shows it.
pub fn read(path: &Path, kind: Kind) -> Read {
    let meta = match fs::metadata(path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Read::Missing,
        Err(error) => return Read::Failed(error.to_string()),
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
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Read::Missing,
        Err(error) => return Read::Failed(error.to_string()),
    };
    // It may have grown since its metadata read.
    if bytes.len() as u64 > kind.max() {
        return Read::TooLarge(bytes.len() as u64);
    }
    if binary(&bytes) {
        return Read::Binary;
    }
    String::from_utf8(bytes).map_or(Read::Binary, Read::Text)
}
