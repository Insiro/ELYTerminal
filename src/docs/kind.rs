//! What a file is shown as, judged by its name, and how much of it may be read.

use std::path::Path;

use ely_gpui_component::primitives::IconName;

/// The most a text document reads.
pub const TEXT_MAX: u64 = 2 * 1024 * 1024;
/// The most an image reads.
pub const IMAGE_MAX: u64 = 50 * 1024 * 1024;
/// How much of a file's start is searched for a NUL.
const SNIFF: usize = 8 * 1024;

/// How a document shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Markdown,
    Json,
    Yaml,
    Xml,
    Image,
    Html,
    Text,
}

impl Kind {
    /// `path`'s kind by its extension, ignoring case; any other file is text.
    pub fn of(path: &Path) -> Self {
        let Some(extension) = path.extension().and_then(|ext| ext.to_str()) else {
            return Self::Text;
        };
        match extension.to_ascii_lowercase().as_str() {
            "md" | "markdown" | "mdx" => Self::Markdown,
            "json" | "jsonc" | "json5" | "geojson" => Self::Json,
            "yaml" | "yml" => Self::Yaml,
            "xml" | "plist" => Self::Xml,
            "htm" | "html" | "xhtml" => Self::Html,
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "tiff" | "svg" => Self::Image,
            _ => Self::Text,
        }
    }

    /// The most bytes a document of this kind reads.
    pub fn max(self) -> u64 {
        match self {
            Self::Image => IMAGE_MAX,
            _ => TEXT_MAX,
        }
    }

    pub fn icon(self) -> IconName {
        match self {
            Self::Markdown => IconName::FileText,
            Self::Json => IconName::FileJson,
            Self::Yaml => IconName::Braces,
            Self::Xml => IconName::FileCode,
            Self::Image => IconName::FileImage,
            Self::Html => IconName::Globe,
            Self::Text => IconName::File,
        }
    }
}

/// Whether `bytes` hold a NUL within their first 8 KiB, as a binary file does.
pub fn binary(bytes: &[u8]) -> bool {
    memchr::memchr(0, &bytes[..bytes.len().min(SNIFF)]).is_some()
}

/// The language a text file's editor names, by its extension, else its file name.
pub fn language(path: &Path) -> &'static str {
    let extension = path.extension().and_then(|ext| ext.to_str());
    let name = path.file_name().and_then(|name| name.to_str());
    match extension.map(str::to_ascii_lowercase).as_deref() {
        Some("rs") => "Rust",
        Some("toml") => "TOML",
        Some("js" | "mjs" | "cjs") => "JavaScript",
        Some("ts" | "mts" | "cts") => "TypeScript",
        Some("jsx" | "tsx") => "React",
        Some("py") => "Python",
        Some("go") => "Go",
        Some("c" | "h") => "C",
        Some("cc" | "cpp" | "cxx" | "hpp") => "C++",
        Some("java") => "Java",
        Some("kt" | "kts") => "Kotlin",
        Some("swift") => "Swift",
        Some("rb") => "Ruby",
        Some("sh" | "bash" | "zsh") => "Shell",
        Some("fish") => "Fish",
        Some("ps1" | "psm1") => "PowerShell",
        Some("css") => "CSS",
        Some("sql") => "SQL",
        Some("lua") => "Lua",
        Some("ini" | "cfg" | "conf") => "INI",
        Some("csv") => "CSV",
        Some("md" | "markdown" | "mdx") => "Markdown",
        Some("json" | "jsonc" | "json5" | "geojson") => "JSON",
        Some("yaml" | "yml") => "YAML",
        Some("xml" | "plist") => "XML",
        Some("htm" | "html" | "xhtml") => "HTML",
        _ => match name {
            Some("Dockerfile") => "Dockerfile",
            Some("Makefile" | "makefile") => "Makefile",
            _ => "Plain Text",
        },
    }
}
