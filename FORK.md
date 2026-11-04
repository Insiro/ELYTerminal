# FORK.md

Insiro's fork of ELYTerminal (upstream `ELY-Open-Source/ELYTerminal`). It keeps the experience of Orca, an agent workspace app, without its weight: projects and their git worktrees in the sidebar, tabs per workspace, document previews, agent usage, simple git. Where this file and `AGENTS.md` differ, this file wins. `AGENTS.md`, `PRD.md` and `TASKS.md` stay as upstream writes them, so syncs merge cleanly.

## Goals, in order

1. Workspaces: projects and their worktrees in the left column, a tab strip per workspace, `workspace.json`, Claude sessions resumed by id.
2. File preview in the right column, a document tab in the main area: Markdown, JSON, YAML, XML, images, text; HTML through Ely's `WebView` on macOS, its source elsewhere.
3. Agent usage: Claude's rate limits through a statusLine wrapper the user turns on, Codex's from its session files.
4. Git: push, pull, fetch, undo commit, checkout, new branch, merge and abort, history paging.

Out of scope: an editor, creating worktrees, skills, automations, plugins, chat UI, remote control, cloud sync.

## Layout changes

- `src/projects.rs`: `Home` (a workspace: Others, or a project's folder or worktree), `TabId`, `normal` and `home_of`. `projects/store` reads and writes `workspace.json` beside `settings.json`, a corrupt file moved aside as `workspace.json.broken-SECS`; `projects/claude` makes v4 session ids and finds their transcripts; its `Modes` searches restored ids' transcripts off the main thread, and `Watch` follows a Claude tab's marks to Claude's command's end.
- `src/atomic.rs`: the temp-file-and-rename save that settings and `workspace.json` share.
- `src/git/worktree.rs`: `git worktree list --porcelain`, `-z` from git 2.36 and lines before it; a repository's subfolder counts as a plain folder.
- `src/workspace/tabs.rs`: tabs, each in a home, the shown one per home. `workspace/strip.rs`: Ely's `EditorTabs` over the terminal, `+`, profiles and the ⋯ menu. `workspace/dialogs.rs`: the questions over the window: rename a tab or a project, remove a project, close a tab Claude runs in, and notices. `workspace/sidebar.rs`: the left column's rows; `sidebar/menu.rs` what a right press on a project's or a worktree's row offers.
- `src/workspace/projects.rs`: the projects the left column lists, added, renamed, expanded, reordered and removed, their tabs with them. `workspace/refresh.rs`: each project's worktrees listed off the main thread, one listing a project at a time, a late one dropped; a window's activation lists again at most every 2 s.
- `src/workspace/persist.rs`: `workspace.json` kept, a snapshot saved 0.5 s after a change and flushed at close, restored at start, each home's tabs woken as it is selected, a tab that fails to start kept; `persist/shape.rs` maps projects and tabs to the saved state and back.
- `src/workspace/claude.rs`: Claude tabs: a shell handed `ELY_CLAUDE` and the prelude, its marks watched, 20 s from the first prompt for Claude to start, a close confirmed while Claude runs, its id copied.
- `src/settings`: `claude_prelude`, a command a Claude tab's shell runs before `claude`. `src/term/marks.rs`: `Mark::Unstarted`, the integration's bare `D;aid=elyterminal`; the view emits `TerminalEvent::Marked` while a Claude tab watches.
- `assets/shell-integration`: zsh, bash, fish and pwsh run `ELY_CLAUDE`'s `claude` once, at the first prompt, then unset it.
- Tests: `src/term/shell/tests/claude.rs`, each shell runs `claude` once; `src/git/tests/worktrees.rs`, worktree listing; `src/projects/tests.rs`, `workspace/tabs/tests.rs`, `workspace/projects/tests.rs` and `workspace/persist/tests.rs`.
- Gone: `workspace/rename.rs`, `workspace/reorder.rs`, `sessions::row`; the sessions column is the projects column.

## Decisions

- 2026-11-04: the left column lists projects and their worktrees, not sessions; a tab strip over the terminal shows the selected workspace's tabs. A press on a project's row opens its main worktree; worktrees are listed, never created.
- 2026-11-04: closing the last tab keeps the window; only its close button or Quit closes it.
- 2026-11-04: a tab shows its state by its icon's shape (hourglass, X, loader, check, bell), since Ely's `EditorTabs` colors every icon as its title; the colored marks live on the left column's rows. Rename, duplicate and close sit in the ⋯ menu beside the strip; no Ely fork.
- 2026-11-04: settings keep `drag_sessions`, `session_menu` and `sessions_width`, now meaning tabs and the projects column, so settings files from upstream still load.
- 2026-11-04: a Claude tab is started by the shell integration at its first prompt, from `ELY_CLAUDE=new:ID` or `resume:ID`, after an optional user command such as `nvm use default`.
- 2026-11-04: Windows is checked later, in one pass.
- 2026-11-04: a Claude tab runs `claude` as a command line of its own at the first prompt: zsh types it through `zle-line-init`, PowerShell's read hands it over, fish runs it at a private key (U+E000) the app sends once the first prompt reads, since fish aborts if a line runs while its own question to the terminal is open, and bash runs it from the last prompt command. A prompt start leads straight into it, which tells it from a line typed ahead; keys typed before the first prompt reach `claude` itself where it starts at once (zsh, PowerShell). Its end clears the id, as does the integration's `D;aid=elyterminal`, which says it does not start, or 20 s from the first prompt without it; a suspend (128 + SIGTSTP) does not. A restored tab resumes unless the transcript search, given 300 ms, finds none; a WSL tab always resumes. cmd runs it through `/k`, marks no command, and keeps its id until the tab closes; its prelude may hold no `"` or `%`.
