//! HTML in a tab on macOS: the file itself in a web view, so styles and pictures beside it load.

use std::{fmt::Write as _, path::Path, time::UNIX_EPOCH};

use super::read::Stamp;

/// `path` as a `file://` address, each byte outside a path's plain set escaped, stamped so a change loads afresh; none for a path not in UTF-8.
pub fn file_url(path: &Path, stamp: &Stamp) -> Option<String> {
    let text = path.to_str()?;
    let mut url = String::from("file://");
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                url.push(byte as char)
            }
            _ => write!(url, "%{byte:02X}").expect("a String takes writes"),
        }
    }
    let nanos = (stamp.mtime)
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_nanos());
    write!(url, "?v={nanos}-{}", stamp.len).expect("a String takes writes");
    Some(url)
}
