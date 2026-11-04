use ely_gpui_component::{
    i18n::text,
    menus::{Menu, MenuItem},
    primitives::IconName,
};
use gpui::{App, Entity};

use crate::{projects::TabId, workspace::Workspace};

/// What the strip's ⋯ offers for tab `id`, the one shown: a rename, and with `full` a duplicate and a close.
pub fn tab(workspace: &Entity<Workspace>, id: TabId, full: bool, cx: &App) -> Menu {
    let (rename, duplicate, close) = (workspace.clone(), workspace.clone(), workspace.clone());
    let menu = Menu::new().item(
        MenuItem::new(text(cx, "et.tab.rename", &[]))
            .icon(IconName::Pencil)
            .on_click(move |window, cx| rename.update(cx, |this, cx| this.rename(id, window, cx))),
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
            .on_click(move |window, cx| close.update(cx, |this, cx| this.close(id, window, cx))),
    )
}
