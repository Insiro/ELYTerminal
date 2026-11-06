//! Simplified Chinese for the workspace's later words, in `en_workspace`'s order.

pub const MESSAGES: &[(&str, &str)] = &[
    ("et.worktree.remove", "移除工作树…"),
    ("et.worktree.remove_title", "移除工作树 {name}？"),
    (
        "et.worktree.remove_body",
        "git 将从磁盘删除 {path}，其分支 {branch} 保留。",
    ),
    ("et.worktree.remove_detached", "git 将从磁盘删除 {path}。"),
    (
        "et.worktree.remove_tabs",
        "其 {count} 个打开的标签页将关闭。",
    ),
    (
        "et.worktree.remove_dirty",
        "其中有未提交的更改或未跟踪的文件，它们将丢失。",
    ),
    ("et.worktree.remove_confirm", "移除"),
    ("et.worktree.remove_force", "仍然移除"),
    ("et.worktree.remove_failed", "无法移除工作树"),
    ("et.worktree.check_failed", "无法检查工作树"),
];
