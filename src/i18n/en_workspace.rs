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
    (
        "et.worktree.remove_tabs",
        "Its {count} open tabs close first, their programs ended.",
    ),
    (
        "et.worktree.remove_dirty",
        "It holds uncommitted changes or untracked files, which will be lost.",
    ),
    ("et.worktree.remove_confirm", "Remove"),
    ("et.worktree.remove_force", "Remove anyway"),
    ("et.worktree.remove_failed", "Cannot remove the worktree"),
    ("et.worktree.check_failed", "Cannot check the worktree"),
    ("et.parked.save", "Save for later"),
    ("et.parked.busy_title", "Save the tab for later?"),
    (
        "et.parked.busy_body",
        "Its shell runs a command, which ends as the tab closes. Its folder, program and last screen are kept.",
    ),
    ("et.parked.resume", "Resume"),
    ("et.parked.show_screen", "Show last screen"),
    ("et.parked.no_screen", "Nothing was on the screen."),
    ("et.parked.delete", "Delete…"),
    ("et.parked.delete_title", "Delete {name}?"),
    (
        "et.parked.delete_body",
        "The saved tab and its last screen are forgotten.",
    ),
    ("et.parked.delete_confirm", "Delete"),
    ("et.parked.age_now", "saved just now"),
    ("et.parked.age_minutes", "saved {n} min ago"),
    ("et.parked.age_hours", "saved {n} h ago"),
    ("et.parked.age_days", "saved {n} days ago"),
    (
        "et.parked.folder_gone",
        "The saved folder is gone; the tab opened in its workspace's folder",
    ),
    ("et.parked.resume_failed", "Cannot resume the saved tab"),
    (
        "et.worktree.remove_missing",
        "Its folder {path} is gone already; git forgets the worktree.",
    ),
    (
        "et.worktree.remove_ignored",
        "{count} ignored entries ({names}) are deleted too, force or not.",
    ),
    ("et.worktree.remove_forget", "Forget"),
    (
        "et.git.loss_under_new",
        "The unstaged changes of its {count} files are lost; {new} new files among them are deleted.",
    ),
];
