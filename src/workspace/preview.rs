//! The right column's preview: a file chosen in Files, shown under the tree.

use std::path::{Path, PathBuf};

use ely_gpui_component::{
    buttons::{ButtonVariant, IconButton},
    i18n::text,
    layout::SplitPane,
    primitives::IconName,
    theme::ControlSize,
};
use gpui::{
    AnyElement, AppContext, Axis, Context, Entity, IntoElement, ParentElement, StyleRefinement,
    Styled, Window, div, px,
};

use super::{Workspace, tabs::body::doc_title};
use crate::{
    docs::{Document, Place},
    files::{Files, OpenTab, Preview},
    settings::{PanelTab, SettingsStore},
};

/// The tree's share of Files at first.
pub(super) const SHARE: f32 = 0.45;

impl Workspace {
    /// Files' choices: a file previews, or opens as a tab.
    pub(super) fn hook_files(files: &Entity<Files>, window: &mut Window, cx: &mut Context<Self>) {
        cx.subscribe_in(files, window, |this, _, Preview(path), window, cx| {
            this.preview(path.clone(), window, cx)
        })
        .detach();
        cx.subscribe_in(files, window, |this, _, OpenTab(path), window, cx| {
            this.open_doc(path.clone(), window, cx)
        })
        .detach();
    }

    /// Previews `path` under the tree, in place of what showed there.
    pub(super) fn preview(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if (self.preview.as_ref()).is_some_and(|doc| doc.read(cx).path() == path) {
            return;
        }
        let visible = self.preview_visible(cx);
        let doc = cx.new(|cx| Document::new(path, Place::Panel, visible, window, cx));
        self.follow_links(&doc, window, cx);
        self.preview = Some(doc);
        cx.notify();
    }

    /// Closes the preview; the tree chooses nothing, so the same file previews again.
    fn close_preview(&mut self, cx: &mut Context<Self>) {
        if let Some(doc) = self.preview.take() {
            log::info!(
                "app: the preview of {} closes",
                doc.read(cx).path().display()
            );
            self.files.update(cx, |files, cx| files.unchoose(cx));
            cx.notify();
        }
    }

    /// Whether the right column shows Files.
    fn preview_visible(&self, cx: &Context<Self>) -> bool {
        let settings = SettingsStore::get(cx);
        settings.show_panel && settings.panel == PanelTab::Files
    }

    /// The preview polls only while the right column shows Files.
    pub(super) fn sync_preview(&self, window: &mut Window, cx: &mut Context<Self>) {
        let visible = self.preview_visible(cx);
        if let Some(doc) = &self.preview {
            doc.update(cx, |doc, cx| doc.set_visible(visible, window, cx));
        }
    }

    /// Files now shows `folder`: a preview outside it closes.
    pub(super) fn keep_preview(&mut self, folder: &Path, cx: &mut Context<Self>) {
        if (self.preview.as_ref()).is_some_and(|doc| !doc.read(cx).path().starts_with(folder)) {
            self.close_preview(cx);
        }
    }

    /// Files' body: the tree, over the preview once a file is chosen.
    pub(super) fn files_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(doc) = &self.preview else {
            return self.files.clone().into_any_element();
        };
        let path = doc.read(cx).path().to_path_buf();
        let button = |id: &'static str, icon: IconName, tip: &'static str| {
            IconButton::new(id, icon)
                .variant(ButtonVariant::Ghost)
                .size(ControlSize::Sm)
                .tooltip(text(cx, tip, &[]))
        };
        let (tabbed, closed, resized) = (cx.entity(), cx.entity(), cx.entity());
        let tab = button("preview-tab", IconName::AppWindow, "et.files.open_tab").on_click(
            move |_, window, cx| {
                let path = path.clone();
                tabbed.update(cx, |this, cx| this.open_doc(path, window, cx))
            },
        );
        let system_path = doc.read(cx).path().to_path_buf();
        let system = button(
            "preview-system",
            IconName::ExternalLink,
            "et.doc.open_system",
        )
        .on_click(move |_, _, cx| {
            log::info!("app: {} opens in the system's app", system_path.display());
            cx.open_with_system(&system_path)
        });
        let close = button("preview-close", IconName::X, "et.preview.close")
            .on_click(move |_, _, cx| closed.update(cx, |this, cx| this.close_preview(cx)));
        let name = div()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .text_ellipsis()
            .whitespace_nowrap()
            .child(doc_title(doc.read(cx)));
        let header = div()
            .flex()
            .flex_none()
            .items_center()
            .gap_1()
            .px_1()
            .child(name)
            .child(tab)
            .child(system)
            .child(close);
        let fill = StyleRefinement::default().size_full();
        let shown = div()
            .size_full()
            .flex()
            .flex_col()
            .child(header)
            .child(div().flex_1().min_h_0().child(doc.clone().cached(fill)));
        SplitPane::new("files-split", Axis::Vertical, px(80.))
            .sizes(&[self.preview_share, 1. - self.preview_share])
            .on_resize(move |shares, _, cx| {
                resized.update(cx, |this, _| this.preview_share = shares[0])
            })
            .pane(div().size_full().child(self.files.clone()))
            .pane(shown)
            .into_any_element()
    }
}
