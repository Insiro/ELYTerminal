//! Where a link inside a document leads.

use std::path::{Component, Path, PathBuf};

/// A link's target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Link {
    /// A web or mail address, for the system to open.
    External(String),
    /// A file, which opens as a document.
    Doc(PathBuf),
    /// A heading in the same document, without its `#`.
    Anchor(String),
    Bad,
}

/// What `href`, in a document in folder `base`, leads to.
pub fn link(base: &Path, href: &str) -> Link {
    let href = href.trim();
    if let Some(anchor) = href.strip_prefix('#') {
        return Link::Anchor(anchor.to_string());
    }
    let lower = href.to_ascii_lowercase();
    if ["http://", "https://", "mailto:"]
        .iter()
        .any(|scheme| lower.starts_with(scheme))
    {
        return Link::External(href.to_string());
    }
    let local = match lower.strip_prefix("file://") {
        Some(_) => &href["file://".len()..],
        None if scheme(href) => return Link::Bad,
        None => href,
    };
    let local = local.split(['#', '?']).next().unwrap_or_default();
    let Some(decoded) = decode(local).filter(|path| !path.is_empty()) else {
        return Link::Bad;
    };
    Link::Doc(normal(&base.join(decoded)))
}

/// Whether `href` starts with a scheme of two letters or more; one letter is a Windows drive.
fn scheme(href: &str) -> bool {
    let Some((name, _)) = href.split_once(':') else {
        return false;
    };
    name.len() > 1
        && name.starts_with(|c: char| c.is_ascii_alphabetic())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// `text` with each `%XX` as its byte; `None` when one is malformed or the result is not UTF-8.
fn decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let hex = text.get(at + 1..at + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// `path` with `.` and `..` resolved by name, so one file has one path.
fn normal(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() && !out.has_root() {
                    out.push(part);
                }
            }
            part => out.push(part),
        }
    }
    out
}
