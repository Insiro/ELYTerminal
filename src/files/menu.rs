use std::path::PathBuf;

use ely_gpui_component::{
    i18n::text,
    menus::{Menu, MenuItem},
    primitives::IconName,
};
use gpui::{App, ClipboardItem, Entity};

use super::{Files, Here, OpenTab, Preview};

/// A right press's menu; `here` starts a session, and a file previews or opens as a tab.
pub fn row(files: &Entity<Files>, path: PathBuf, (here, file): (PathBuf, bool), cx: &App) -> Menu {
    let (open, host, copy) = (path.clone(), files.clone(), path.clone());
    let mut menu = Menu::new();
    if file {
        let (shown, tabbed) = (files.clone(), files.clone());
        let (preview, tab) = (path.clone(), path);
        menu = menu
            .item(
                MenuItem::new(text(cx, "et.files.preview", &[]))
                    .icon(IconName::Eye)
                    .on_click(move |_, cx| {
                        shown.update(cx, |_, cx| cx.emit(Preview(preview.clone())))
                    }),
            )
            .item(
                MenuItem::new(text(cx, "et.files.open_tab", &[]))
                    .icon(IconName::FileText)
                    .on_click(move |_, cx| {
                        tabbed.update(cx, |_, cx| cx.emit(OpenTab(tab.clone())))
                    }),
            );
    }
    menu.item(
        MenuItem::new(text(cx, "et.files.open", &[]))
            .icon(IconName::ExternalLink)
            .on_click(move |_, cx| {
                log::info!("files: opening {}", open.display());
                cx.open_with_system(&open);
            }),
    )
    .item(
        MenuItem::new(text(cx, "et.files.new_session", &[]))
            .icon(IconName::Terminal)
            .on_click(move |_, cx| host.update(cx, |_, cx| cx.emit(Here(here.clone())))),
    )
    .separator()
    .item(
        MenuItem::new(text(cx, "et.files.copy_path", &[]))
            .icon(IconName::Copy)
            .on_click(move |_, cx| match copy.to_str() {
                Some(text) => {
                    log::info!("files: copied {text}");
                    cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
                }
                None => log::error!("files: {} is no UTF-8 to copy", copy.display()),
            }),
    )
}
