use std::path::{Path, PathBuf};

use super::twin;
use crate::projects::{Home, TabId};

#[test]
fn a_path_opens_once_per_home() {
    let tree = Home::Tree {
        project: PathBuf::from("/p"),
        path: PathBuf::from("/p"),
    };
    let readme = Path::new("/p/README.md");
    let docs = [
        (&Home::Others, readme, TabId(1)),
        (&tree, Path::new("/p/a.json"), TabId(2)),
        (&tree, readme, TabId(3)),
    ];
    assert_eq!(twin(docs, &tree, readme), Some(TabId(3)));
    assert_eq!(twin(docs, &Home::Others, readme), Some(TabId(1)));
    assert_eq!(twin(docs, &Home::Others, Path::new("/p/a.json")), None);
}
