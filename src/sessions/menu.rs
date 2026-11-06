use ely_gpui_component::{
    i18n::text,
    menus::{Menu, MenuItem},
    primitives::IconName,
};
use gpui::{App, Entity};

use crate::{projects::TabId, workspace::Workspace};

/// What the strip's ⋯ offers for tab `id`, the one shown: a rename, with `full` a duplicate and a close, and Claude's id while it runs one.
pub fn tab(
    workspace: &Entity<Workspace>,
    id: TabId,
    (full, claude): (bool, Option<String>),
    cx: &App,
) -> Menu {
    let (rename, duplicate, close) = (workspace.clone(), workspace.clone(), workspace.clone());
    let mut menu = Menu::new().item(
        MenuItem::new(text(cx, "et.tab.rename", &[]))
            .icon(IconName::Pencil)
            .on_click(move |window, cx| rename.update(cx, |this, cx| this.rename(id, window, cx))),
    );
    if claude.is_some() {
        let copied = workspace.clone();
        menu = menu.item(
            MenuItem::new(text(cx, "et.claude.copy_id", &[]))
                .icon(IconName::Copy)
                .on_click(move |_, cx| copied.update(cx, |this, cx| this.copy_claude(id, cx))),
        );
    }
    let parked = workspace.clone();
    let menu = menu.item(
        MenuItem::new(text(cx, "et.parked.save", &[]))
            .icon(IconName::Archive)
            .on_click(move |window, cx| {
                parked.update(cx, |this, cx| this.ask_park(id, window, cx))
            }),
    );
    if !full {
        return menu;
    }
    menu.item(
        MenuItem::new(text(cx, "et.tab.duplicate", &[]))
            .icon(IconName::Copy)
            .on_click(move |window, cx| {
                duplicate.update(cx, |this, cx| this.duplicate(id, window, cx))
            }),
    )
    .separator()
    .item(
        MenuItem::new(text(cx, "et.session.close", &[]))
            .icon(IconName::X)
            .on_click(move |window, cx| {
                close.update(cx, |this, cx| this.ask_close(id, window, cx))
            }),
    )
}
