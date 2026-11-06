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
        "其 {count} 个打开的标签页将先关闭，其中的程序随之结束。",
    ),
    (
        "et.worktree.remove_dirty",
        "其中有未提交的更改或未跟踪的文件，它们将丢失。",
    ),
    ("et.worktree.remove_confirm", "移除"),
    ("et.worktree.remove_force", "仍然移除"),
    ("et.worktree.remove_failed", "无法移除工作树"),
    ("et.worktree.check_failed", "无法检查工作树"),
    ("et.parked.save", "稍后继续"),
    ("et.parked.busy_title", "保存标签页以便稍后继续？"),
    (
        "et.parked.busy_body",
        "其 shell 正在运行命令，标签页关闭时命令将结束。其文件夹、程序和最后的屏幕会保留。",
    ),
    ("et.parked.resume", "继续"),
    ("et.parked.show_screen", "显示最后的屏幕"),
    ("et.parked.no_screen", "屏幕上没有内容。"),
    ("et.parked.delete", "删除…"),
    ("et.parked.delete_title", "删除 {name}？"),
    (
        "et.parked.delete_body",
        "保存的标签页及其最后的屏幕将被忘记。",
    ),
    ("et.parked.delete_confirm", "删除"),
    ("et.parked.age_now", "刚刚保存"),
    ("et.parked.age_minutes", "{n} 分钟前保存"),
    ("et.parked.age_hours", "{n} 小时前保存"),
    ("et.parked.age_days", "{n} 天前保存"),
    (
        "et.parked.folder_gone",
        "保存的文件夹已不存在；标签页在其工作区的文件夹中打开",
    ),
    ("et.parked.resume_failed", "无法继续保存的标签页"),
    (
        "et.worktree.remove_missing",
        "其文件夹 {path} 已不存在；git 将忘记该工作树。",
    ),
    (
        "et.worktree.remove_ignored",
        "{count} 个被忽略的条目（{names}）也会被删除，无论是否强制。",
    ),
    ("et.worktree.remove_forget", "忘记"),
    (
        "et.git.loss_under_new",
        "其中 {count} 个文件未暂存的改动会丢失，其中 {new} 个新文件会被删除。",
    ),
];
