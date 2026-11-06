//! English for the workspace's later words, kept apart so `en.rs` stays under 500 lines.

pub const MESSAGES: &[(&str, &str)] = &[
    ("et.worktree.remove", "Remove worktree…"),
    ("et.worktree.remove_title", "Remove worktree {name}?"),
    (
        "et.worktree.remove_body",
        "git removes {path} from disk. Its branch {branch} stays.",
    ),
    (
        "et.worktree.remove_detached",
        "git removes {path} from disk.",
    ),
    ("et.worktree.remove_tabs", "Its {count} open tabs close."),
    (
        "et.worktree.remove_dirty",
        "It holds uncommitted changes or untracked files, which will be lost.",
    ),
    ("et.worktree.remove_confirm", "Remove"),
    ("et.worktree.remove_force", "Remove anyway"),
    ("et.worktree.remove_failed", "Cannot remove the worktree"),
    ("et.worktree.check_failed", "Cannot check the worktree"),
];
