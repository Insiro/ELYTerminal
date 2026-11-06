//! What a right press on a project's or a worktree's row offers.

use std::path::PathBuf;

use ely_gpui_component::{
    i18n::text,
    menus::{Menu, MenuItem},
    primitives::IconName,
};
use gpui::{App, ClipboardItem, Entity};

use super::{super::projects::Project, Workspace};
use crate::projects::Home;

/// A new terminal in `home`, selected.
fn new_terminal(workspace: &Entity<Workspace>, home: Home, cx: &App) -> MenuItem {
    let workspace = workspace.clone();
    MenuItem::new(text(cx, "et.session.new", &[]))
        .icon(IconName::Plus)
        .on_click(move |window, cx| {
            workspace.update(cx, |this, cx| {
                this.select_home(home.clone(), window, cx);
                this.new_session(window, cx);
            })
        })
}

/// A new Claude tab in `home`, selected.
fn new_claude(workspace: &Entity<Workspace>, home: Home, cx: &App) -> MenuItem {
    let workspace = workspace.clone();
    MenuItem::new(text(cx, "et.claude.new", &[]))
        .icon(IconName::Bot)
        .on_click(move |window, cx| {
            workspace.update(cx, |this, cx| {
                this.new_claude(Some(home.clone()), window, cx)
            })
        })
}

fn reveal(folder: PathBuf, cx: &App) -> MenuItem {
    MenuItem::new(text(cx, "et.projects.reveal", &[]))
        .icon(IconName::FolderOpen)
        .on_click(move |_, cx| {
            log::info!("app: revealing {}", folder.display());
            cx.open_with_system(&folder);
        })
}

pub(super) fn project(workspace: &Entity<Workspace>, project: &Project, cx: &App) -> Menu {
    let id = project.id;
    let (refresh, rename, remove) = (workspace.clone(), workspace.clone(), workspace.clone());
    Menu::new()
        .item(new_terminal(workspace, project.home(), cx))
        .item(new_claude(workspace, project.home(), cx))
        .item(
            MenuItem::new(text(cx, "et.projects.refresh", &[]))
                .icon(IconName::RefreshCw)
                .on_click(move |_, cx| refresh.update(cx, |this, cx| this.refresh(id, cx))),
        )
        .item(
            MenuItem::new(text(cx, "et.projects.rename", &[]))
                .icon(IconName::Pencil)
                .on_click(move |window, cx| {
                    rename.update(cx, |this, cx| this.ask_rename_project(id, window, cx))
                }),
        )
        .item(reveal(project.folder.clone(), cx))
        .separator()
        .item(
            MenuItem::new(text(cx, "et.projects.remove", &[]))
                .icon(IconName::Trash2)
                .on_click(move |_, cx| remove.update(cx, |this, cx| this.ask_remove(id, cx))),
        )
}

/// A linked worktree's menu; `removable` names its project and branch when git may remove it.
pub(super) fn worktree(
    workspace: &Entity<Workspace>,
    (home, path): (Home, PathBuf),
    removable: Option<(u64, Option<String>)>,
    cx: &App,
) -> Menu {
    let copied = path.clone();
    let remove = removable.map(|(project, branch)| {
        let (workspace, path) = (workspace.clone(), path.clone());
        MenuItem::new(text(cx, "et.worktree.remove", &[]))
            .icon(IconName::Trash2)
            .on_click(move |window, cx| {
                let asked = (project, path.clone());
                workspace.update(cx, |this, cx| {
                    this.ask_remove_worktree(asked, branch.clone(), window, cx)
                })
            })
    });
    let menu = Menu::new()
        .item(new_terminal(workspace, home.clone(), cx))
        .item(new_claude(workspace, home, cx))
        .item(
            MenuItem::new(text(cx, "et.files.copy_path", &[]))
                .icon(IconName::Copy)
                .on_click(move |_, cx| match copied.to_str() {
                    Some(text) => {
                        log::info!("app: copied {text}");
                        cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
                    }
                    None => log::error!("app: {} is no UTF-8 to copy", copied.display()),
                }),
        )
        .item(reveal(path, cx));
    match remove {
        Some(remove) => menu.separator().item(remove),
        None => menu,
    }
}
