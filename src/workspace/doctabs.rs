//! Document tabs: a file opened in the selected home, once per home, and what its ⋯ menu offers.

use std::path::{Path, PathBuf};

use ely_gpui_component::{
    i18n::text,
    menus::{Menu, MenuItem},
    primitives::IconName,
};
use gpui::{App, AppContext, ClipboardItem, Context, Entity, Window};

use super::{
    Workspace,
    projects::notice,
    tabs::{Body, Tab},
};
use crate::{
    docs::{DocEvent, Document, Place, kind::Kind},
    projects::{Home, TabId},
};

/// The tab of `home` that shows `path` already, among each doc tab's home, path and id.
pub fn twin<'a>(
    docs: impl IntoIterator<Item = (&'a Home, &'a Path, TabId)>,
    home: &Home,
    path: &Path,
) -> Option<TabId> {
    docs.into_iter()
        .find(|(of, shown, _)| *of == home && *shown == path)
        .map(|(_, _, id)| id)
}

impl Workspace {
    /// Shows `path` as a tab of the selected home: its tab there, else a new one after the shown tab.
    pub fn open_doc(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        // workspace.json keeps paths as text.
        if path.to_str().is_none() {
            log::error!("app: {} is not UTF-8; it opens as no tab", path.display());
            let error = anyhow::anyhow!("{}", path.display());
            notice("et.doc.bad_path", error, window, cx);
            return;
        }
        let home = self.selected.clone();
        let docs: Vec<(Home, PathBuf, TabId)> = self
            .tabs
            .iter()
            .filter_map(|tab| Some((tab.home.clone(), tab.doc()?.read(cx).path().into(), tab.id)))
            .collect();
        let found = twin(
            docs.iter()
                .map(|(home, path, id)| (home, path.as_path(), *id)),
            &home,
            &path,
        );
        if let Some(id) = found {
            log::info!("app: {} shows in tab {} already", path.display(), id.0);
            self.show(id, window, cx);
            return;
        }
        let after = self.current_id();
        let id = self.add_doc(path, home, window, cx);
        if let Some(at) = after.and_then(|after| self.index(after)) {
            let tab = self.tabs.pop().expect("the tab just added");
            self.tabs.insert(at + 1, tab);
        }
        self.show(id, window, cx);
        self.dirty(cx);
    }

    /// A document tab of `path` in `home`, last in the list, out of view.
    pub(super) fn add_doc(
        &mut self,
        path: PathBuf,
        home: Home,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> TabId {
        let id = TabId(self.next_tab);
        self.next_tab += 1;
        log::info!("app: tab {} opens {} in {home:?}", id.0, path.display());
        let doc = cx.new(|cx| Document::new(path, Place::Tab, false, window, cx));
        self.follow_links(&doc, window, cx);
        self.tabs.push(Tab {
            id,
            home,
            body: Body::Doc(doc),
        });
        id
    }

    /// A link in `doc` to another file opens it as a tab; a change, a file gone among them, draws the strip again.
    pub(super) fn follow_links(
        &self,
        doc: &Entity<Document>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.subscribe_in(doc, window, |this, _, DocEvent::Open(path), window, cx| {
            this.open_doc(path.clone(), window, cx)
        })
        .detach();
        cx.observe(doc, |_, _, cx| cx.notify()).detach();
    }
}

/// A document tab's ⋯ menu.
pub fn menu(workspace: &Entity<Workspace>, id: TabId, doc: &Document, cx: &App) -> Menu {
    let path = doc.path().to_path_buf();
    let (copy, system, browser) = (path.clone(), path.clone(), path);
    let close = workspace.clone();
    let mut menu = Menu::new()
        .item(
            MenuItem::new(text(cx, "et.files.copy_path", &[]))
                .icon(IconName::Copy)
                .on_click(move |_, cx| match copy.to_str() {
                    Some(path) => {
                        log::info!("app: copied {path}");
                        cx.write_to_clipboard(ClipboardItem::new_string(path.to_string()));
                    }
                    None => log::error!("app: {} is no UTF-8 to copy", copy.display()),
                }),
        )
        .item(
            MenuItem::new(text(cx, "et.doc.open_system", &[]))
                .icon(IconName::ExternalLink)
                .on_click(move |_, cx| {
                    log::info!("app: {} opens in the system's app", system.display());
                    cx.open_with_system(&system)
                }),
        );
    if doc.kind() == Kind::Html {
        menu = menu.item(
            MenuItem::new(text(cx, "et.doc.open_browser", &[]))
                .icon(IconName::Globe)
                .on_click(move |_, cx| {
                    log::info!("app: {} opens in the browser", browser.display());
                    cx.open_with_system(&browser)
                }),
        );
    }
    menu.separator().item(
        MenuItem::new(text(cx, "et.session.close", &[]))
            .icon(IconName::X)
            .on_click(move |window, cx| close.update(cx, |this, cx| this.close(id, window, cx))),
    )
}

#[cfg(test)]
mod tests;
