use std::{
    env, fs,
    path::{Path, PathBuf},
};

use super::{
    kind::{Kind, TEXT_MAX, binary, language},
    links::{Link, link},
    read::{Found, Read, look, read, stamp},
};

/// A fresh folder under the temp dir.
fn folder(name: &str) -> PathBuf {
    let folder = env::temp_dir().join(format!("elyterminal-docs-{name}-{}", std::process::id()));
    if folder.exists() {
        fs::remove_dir_all(&folder).expect("a stale test folder goes");
    }
    fs::create_dir_all(&folder).expect("a test folder");
    folder
}

#[test]
fn kinds_follow_the_extension_ignoring_case() {
    let cases = [
        ("README.md", Kind::Markdown),
        ("notes.MDX", Kind::Markdown),
        ("a.markdown", Kind::Markdown),
        ("package.json", Kind::Json),
        ("tsconfig.jsonc", Kind::Json),
        ("map.GeoJSON", Kind::Json),
        ("x.json5", Kind::Json),
        ("ci.yml", Kind::Yaml),
        ("ci.YAML", Kind::Yaml),
        ("pom.xml", Kind::Xml),
        ("Info.plist", Kind::Xml),
        ("index.html", Kind::Html),
        ("old.HTM", Kind::Html),
        ("page.xhtml", Kind::Html),
        ("logo.svg", Kind::Image),
        ("shot.PNG", Kind::Image),
        ("p.jpeg", Kind::Image),
        ("f.ico", Kind::Image),
        ("scan.tiff", Kind::Image),
        ("main.rs", Kind::Text),
        ("Dockerfile", Kind::Text),
        (".gitignore", Kind::Text),
        ("archive.tar.gz", Kind::Text),
    ];
    for (name, kind) in cases {
        assert_eq!(Kind::of(Path::new(name)), kind, "{name}");
    }
}

#[test]
fn images_may_be_larger_than_text() {
    assert_eq!(Kind::Text.max(), TEXT_MAX);
    assert!(Kind::Image.max() > Kind::Markdown.max());
}

#[test]
fn a_nul_in_the_first_8_kib_is_binary() {
    assert!(!binary(b"plain text\n"));
    assert!(binary(b"PK\x03\x04\0\0"));
    let mut late = vec![b'a'; 8 * 1024];
    late.push(0);
    assert!(!binary(&late), "a NUL past 8 KiB is not looked for");
}

#[test]
fn languages_come_from_the_extension_or_the_name() {
    assert_eq!(language(Path::new("src/main.rs")), "Rust");
    assert_eq!(language(Path::new("Cargo.TOML")), "TOML");
    assert_eq!(language(Path::new("Dockerfile")), "Dockerfile");
    assert_eq!(language(Path::new("notes")), "Plain Text");
}

#[test]
fn reads_text_and_tells_what_it_cannot_show() {
    let root = folder("read");
    let text = root.join("a.md");
    fs::write(&text, "# Hi\n").expect("a file");
    assert_eq!(read(&text, Kind::Markdown), Read::Text("# Hi\n".into()));
    let nul = root.join("b.txt");
    fs::write(&nul, b"a\0b").expect("a file");
    assert_eq!(read(&nul, Kind::Text), Read::Binary);
    let latin = root.join("c.txt");
    fs::write(&latin, b"caf\xe9").expect("a file");
    assert_eq!(read(&latin, Kind::Text), Read::Binary, "not UTF-8");
    let big = root.join("d.txt");
    fs::write(&big, vec![b'a'; TEXT_MAX as usize + 1]).expect("a file");
    assert_eq!(read(&big, Kind::Text), Read::TooLarge(TEXT_MAX + 1));
    let image = root.join("e.png");
    fs::write(&image, b"\x89PNG").expect("a file");
    assert_eq!(read(&image, Kind::Image), Read::Image);
    assert!(matches!(
        read(&root.join("gone.md"), Kind::Markdown),
        Read::Failed(_)
    ));
    assert!(
        matches!(read(&root, Kind::Text), Read::Failed(_)),
        "a folder"
    );
    fs::remove_dir_all(&root).ok();
}

#[test]
fn a_stamp_changes_with_the_file_and_is_none_while_missing() {
    let root = folder("stamp");
    let path = root.join("a.txt");
    assert_eq!(stamp(&path).expect("a stamp"), None);
    fs::write(&path, "one").expect("a file");
    let first = stamp(&path).expect("a stamp").expect("present");
    assert_eq!(first.len, 3);
    fs::write(&path, "three").expect("a file");
    assert_ne!(stamp(&path).expect("a stamp"), Some(first));
    fs::remove_dir_all(&root).ok();
}

#[test]
fn links_lead_out_to_a_doc_or_to_a_heading() {
    let base = Path::new("/repo/docs");
    let cases = [
        (
            "https://example.com/a",
            Link::External("https://example.com/a".into()),
        ),
        ("HTTP://x.y", Link::External("HTTP://x.y".into())),
        ("mailto:a@b.c", Link::External("mailto:a@b.c".into())),
        ("#usage", Link::Anchor("usage".into())),
        ("guide.md", Link::Doc("/repo/docs/guide.md".into())),
        (
            "./guide.md#install",
            Link::Doc("/repo/docs/guide.md".into()),
        ),
        ("../README.md?plain=1", Link::Doc("/repo/README.md".into())),
        ("my%20notes.md", Link::Doc("/repo/docs/my notes.md".into())),
        (
            "data/config.json",
            Link::Doc("/repo/docs/data/config.json".into()),
        ),
        ("/etc/hosts", Link::Doc("/etc/hosts".into())),
        ("file:///tmp/a%23b.md", Link::Doc("/tmp/a#b.md".into())),
        ("javascript:alert(1)", Link::Bad),
        ("ftp://host/x", Link::Bad),
        ("bad%2", Link::Bad),
        ("bad%ff.md", Link::Bad),
        ("?q=1", Link::Bad),
        ("file://localhost/tmp/a.md", Link::Doc("/tmp/a.md".into())),
        ("file://server/share/a.md", Link::Bad),
        ("//server/share/a.md", Link::Bad),
        (r"\\server\share\a.md", Link::Bad),
        ("", Link::Bad),
    ];
    for (href, expected) in cases {
        assert_eq!(link(base, href), expected, "{href:?}");
    }
}

#[test]
fn a_look_reads_only_a_changed_stamp() {
    let root = folder("look");
    let path = root.join("a.json");
    assert_eq!(look(&path, Kind::Json, None), Found::Gone);
    fs::write(&path, "{}").expect("a file");
    let Found::Changed(first, Read::Text(text)) = look(&path, Kind::Json, None) else {
        panic!("a new file reads");
    };
    assert_eq!(text, "{}");
    assert_eq!(look(&path, Kind::Json, Some(first)), Found::Same);
    fs::write(&path, "[1, 2]").expect("a file");
    assert!(matches!(
        look(&path, Kind::Json, Some(first)),
        Found::Changed(_, Read::Text(_))
    ));
    fs::remove_file(&path).expect("gone");
    assert_eq!(look(&path, Kind::Json, Some(first)), Found::Gone);
    fs::remove_dir_all(&root).ok();
}
