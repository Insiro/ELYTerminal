# FORK.md

Insiro's fork of ELYTerminal (upstream `ELY-Open-Source/ELYTerminal`). It keeps the experience of Orca, an agent workspace app, without its weight: projects and their git worktrees in the sidebar, tabs per workspace, document previews, agent usage, simple git. Where this file and `AGENTS.md` differ, this file wins. `AGENTS.md`, `PRD.md` and `TASKS.md` stay as upstream writes them, so syncs merge cleanly.

## Goals, in order

1. Workspaces: projects and their worktrees in the left column, a tab strip per workspace, `workspace.json`, Claude sessions resumed by id.
2. File preview in the right column, a document tab in the main area: Markdown, JSON, YAML, XML, images, text; HTML through Ely's `WebView` on macOS, its source elsewhere.
3. Agent usage: Claude's rate limits through a statusLine wrapper the user turns on, Codex's from its session files.
4. Git: push, pull, fetch, undo commit, checkout, new branch, merge and abort, history paging.

Out of scope: an editor, creating worktrees, skills, automations, plugins, chat UI, remote control, cloud sync.

## Layout changes

- `src/projects.rs`: `Home` (a workspace: Others, or a project's folder or worktree), `TabId`, `normal` and `home_of`. `projects/store` reads and writes `workspace.json` beside `settings.json`, a corrupt file moved aside as `workspace.json.broken-SECS`; `projects/claude` makes v4 session ids and finds their transcripts.
- `src/atomic.rs`: the temp-file-and-rename save that settings and `workspace.json` share.
- `src/git/worktree.rs`: `git worktree list --porcelain`, `-z` from git 2.36 and lines before it; a repository's subfolder counts as a plain folder.
- `src/workspace/tabs.rs`: tabs, each in a home, the shown one per home. `workspace/strip.rs`: Ely's `EditorTabs` over the terminal, `+`, profiles and the ⋯ menu. `workspace/dialogs.rs`: the rename prompt. `workspace/sidebar.rs`: the left column's rows.
- Gone: `workspace/rename.rs`, `workspace/reorder.rs`, `sessions::row`; the sessions column is the projects column.

## Decisions

- 2026-11-04: the left column lists projects and their worktrees, not sessions; a tab strip over the terminal shows the selected workspace's tabs. A press on a project's row opens its main worktree; worktrees are listed, never created.
- 2026-11-04: closing the last tab keeps the window; only its close button or Quit closes it.
- 2026-11-04: a tab shows its state by its icon's shape (hourglass, X, loader, check, bell), since Ely's `EditorTabs` colors every icon as its title; the colored marks live on the left column's rows. Rename, duplicate and close sit in the ⋯ menu beside the strip; no Ely fork.
- 2026-11-04: settings keep `drag_sessions`, `session_menu` and `sessions_width`, now meaning tabs and the projects column, so settings files from upstream still load.
- 2026-11-04: a Claude tab is started by the shell integration at its first prompt, from `ELY_CLAUDE=new:ID` or `resume:ID`, after an optional user command such as `nvm use default`.
- 2026-11-04: Windows is checked later, in one pass.
