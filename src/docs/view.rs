//! How a document draws, by what it shows.

use std::{path::Path, sync::Arc};

use ely_gpui_component::{
    buttons::{Button, ButtonVariant},
    devtools::{JsonViewer, XmlViewer, YamlViewer},
    documents::MarkdownRenderer,
    feedback::{EmptyState, InlineMessage},
    i18n::text,
    media::ImageViewer,
    primitives::{IconName, Severity},
};
use gpui::{
    AnyElement, App, Context, ElementId, InteractiveElement, IntoElement, ParentElement, Render,
    SharedString, StatefulInteractiveElement, Styled, Window, div,
};

#[cfg(target_os = "macos")]
use ely_gpui_component::misc::{WebSource, WebView};

use super::{
    DocEvent, Document, Shown,
    kind::Kind,
    links::{Link, link},
};

/// A size in bytes as people read it.
fn size(bytes: u64) -> String {
    const MIB: f64 = 1024. * 1024.;
    format!("{:.1} MiB", bytes as f64 / MIB)
}

/// Opens `path` in the app the system gives its kind.
fn system_button(id: impl Into<ElementId>, path: &Path, cx: &App) -> Button {
    let path = path.to_path_buf();
    Button::new(id, text(cx, "et.doc.open_system", &[]))
        .icon(IconName::ExternalLink)
        .on_click(move |_, _, cx| {
            log::info!("docs: {} opens in the system's app", path.display());
            cx.open_with_system(&path)
        })
}

impl Document {
    /// This document's id for an element named `name`.
    fn id(&self, name: &'static str, cx: &Context<Self>) -> ElementId {
        (name, cx.entity_id().as_u64() as usize).into()
    }

    /// The image viewer's id, new at each read, so a failed decode is tried again.
    fn image_id(&self, cx: &Context<Self>) -> ElementId {
        let entity = cx.entity_id().as_u64();
        ElementId::Name(format!("doc-image-{entity}-{}", self.version).into())
    }

    /// Markdown in a scrolled column: links to files open as documents, web links in the browser.
    fn markdown(&self, body: &SharedString, cx: &mut Context<Self>) -> AnyElement {
        let base: Arc<Path> = self.path.parent().unwrap_or(Path::new("")).into();
        let (doc, folder) = (cx.entity(), base.clone());
        let renderer = MarkdownRenderer::new(self.id("doc-markdown", cx), body.clone())
            .base(base)
            .on_link(move |href, _, cx| match link(&folder, href) {
                Link::External(url) => {
                    log::info!("docs: a link opens {url}");
                    cx.open_url(&url);
                }
                Link::Doc(path) if path.is_dir() => {
                    log::warn!("docs: a link to folder {} opens nothing", path.display())
                }
                Link::Doc(path) => {
                    log::info!("docs: a link opens {}", path.display());
                    doc.update(cx, |_, cx| cx.emit(DocEvent::Open(path)));
                }
                Link::Anchor(anchor) => log::debug!("docs: a link to #{anchor} stays"),
                Link::Bad => log::warn!("docs: a link to {href:?} leads nowhere"),
            });
        div()
            .id(self.id("doc-scroll", cx))
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .p_4()
            .child(renderer)
            .into_any_element()
    }

    /// Why the file does not show, and the system's app for it.
    fn notice(&self, icon: IconName, title: SharedString, cx: &Context<Self>) -> AnyElement {
        let mut state = EmptyState::new(self.id("doc-notice", cx), icon, title)
            .action(system_button(self.id("doc-system", cx), &self.path, cx));
        if let Some(name) = self.path.file_name() {
            state = state.body(name.to_string_lossy().to_string());
        }
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(state)
            .into_any_element()
    }

    /// HTML in a tab on macOS as its page, while no dialog asks and the document holds the keys, since a page draws over menus and dialogs; else none, and its source shows.
    #[cfg(target_os = "macos")]
    fn page(&self, window: &Window, cx: &Context<Self>) -> Option<AnyElement> {
        if !self.webby() || !self.web || !self.focus.contains_focused(window, cx) {
            return None;
        }
        let url = super::web::file_url(&self.path, self.stamp.as_ref()?)?;
        let page = WebView::new(self.id("doc-web", cx), WebSource::Url(url.into()));
        Some(div().flex_1().min_h_0().child(page).into_any_element())
    }

    #[cfg(not(target_os = "macos"))]
    fn page(&self, _: &Window, _: &Context<Self>) -> Option<AnyElement> {
        None
    }

    fn body(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        match &self.shown {
            Shown::Loading => div().size_full().into_any_element(),
            Shown::Text(body) => match (self.kind, &self.search) {
                (Kind::Json, Some(search)) => {
                    let viewer = JsonViewer::new(self.id("doc-json", cx), body.clone(), search);
                    div().p_2().child(viewer).into_any_element()
                }
                (Kind::Yaml, Some(search)) => {
                    let viewer = YamlViewer::new(self.id("doc-yaml", cx), body.clone(), search);
                    div().p_2().child(viewer).into_any_element()
                }
                (Kind::Xml, _) => {
                    let viewer = XmlViewer::new(self.id("doc-xml", cx), body.clone());
                    div().p_2().child(viewer).into_any_element()
                }
                _ => self.markdown(body, cx),
            },
            Shown::Code(editor) => {
                let editor = match self.page(window, cx) {
                    Some(page) => page,
                    None => div()
                        .flex_1()
                        .min_h_0()
                        .child(editor.clone())
                        .into_any_element(),
                };
                let mut column = div().size_full().flex().flex_col();
                if self.kind == Kind::Html {
                    let path = self.path.clone();
                    let browser = Button::new(
                        self.id("doc-browser", cx),
                        text(cx, "et.doc.open_browser", &[]),
                    )
                    .icon(IconName::Globe)
                    .variant(ButtonVariant::Ghost)
                    .on_click(move |_, _, cx| {
                        log::info!("docs: {} opens in the browser", path.display());
                        cx.open_with_system(&path)
                    });
                    column = column.child(div().flex().justify_end().p_1().child(browser));
                }
                column.child(editor).into_any_element()
            }
            Shown::Image(source) => div()
                .size_full()
                .child(ImageViewer::new(self.image_id(cx), source.clone()))
                .into_any_element(),
            Shown::Binary => {
                let title = text(cx, "et.doc.binary", &[]);
                self.notice(IconName::FileType, title, cx)
            }
            Shown::TooLarge(bytes) => {
                let title = text(cx, "et.doc.too_large", &[("size", &size(*bytes))]);
                self.notice(IconName::FileType, title, cx)
            }
            Shown::Failed(why) => {
                let title = text(cx, "et.doc.failed", &[("why", why)]);
                self.notice(IconName::TriangleAlert, title, cx)
            }
        }
    }
}

impl Render for Document {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let banner = self.missing.then(|| {
            let message = InlineMessage::new(Severity::Warning, text(cx, "et.doc.missing", &[]));
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .px_3()
                .py_1()
                .child(message)
        });
        div()
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .children(banner)
            .child(div().flex_1().min_h_0().child(self.body(window, cx)))
    }
}
