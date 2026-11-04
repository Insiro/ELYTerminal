//! Documents: a file shown as Markdown, a tree, an image or text, read again as it changes.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use ely_gpui_component::{editor::CodeEditor, forms::TextInput, i18n::text};
use gpui::{
    AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, ImageAssetLoader, Resource,
    ScrollHandle, SharedString, Task, Window,
};

pub mod kind;
pub mod links;
pub mod read;
#[cfg(test)]
mod tests;
mod view;

use kind::{Kind, language};
use read::{Found, Read, Stamp, look};

/// How often a shown document looks at its file.
const POLL: Duration = Duration::from_secs(1);

/// Where a document shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// The right column's preview.
    Panel,
    /// A tab of the main area.
    Tab,
}

/// What a document shows of its file.
pub enum Shown {
    Loading,
    /// Markdown, JSON, YAML or XML, drawn from the text.
    Text(SharedString),
    /// Any other text, and HTML's source, in a read-only editor.
    Code(Entity<CodeEditor>),
    Image(Resource),
    Binary,
    TooLarge(u64),
    Failed(String),
}

/// What a document asks of its owner.
pub enum DocEvent {
    /// A link leads to another file.
    Open(PathBuf),
}

/// A file shown as its kind shows, read off the main thread while visible, each second it changed.
pub struct Document {
    path: PathBuf,
    kind: Kind,
    place: Place,
    shown: Shown,
    /// The stamp last read; none while the file is missing.
    stamp: Option<Stamp>,
    /// The file went: the last shown stays, under a banner.
    missing: bool,
    focus: FocusHandle,
    /// Markdown's scroll, kept across reads.
    scroll: ScrollHandle,
    /// The tree's find field, for JSON and YAML.
    search: Option<Entity<TextInput>>,
    visible: bool,
    /// A look under way, so a poll waits for it.
    busy: bool,
    /// Counts the reads shown.
    version: u64,
    _polling: Task<()>,
    reading: Task<()>,
}

impl EventEmitter<DocEvent> for Document {}

impl Focusable for Document {
    /// The editor's, so its keys work; else the document's own.
    fn focus_handle(&self, cx: &gpui::App) -> FocusHandle {
        match &self.shown {
            Shown::Code(editor) => editor.focus_handle(cx),
            _ => self.focus.clone(),
        }
    }
}

impl Document {
    /// Opens `path` in `place`, read at once and each second it shows.
    pub fn new(path: PathBuf, place: Place, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let kind = Kind::of(&path);
        log::info!(
            "docs: {} opens as {kind:?} in the {place:?}",
            path.display()
        );
        let search = matches!(kind, Kind::Json | Kind::Yaml).then(|| {
            let placeholder = text(cx, "et.doc.search", &[]);
            cx.new(|cx| TextInput::new(window, cx).placeholder(placeholder))
        });
        let polling = cx.spawn_in(window, async move |doc, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                let ticked = doc.update_in(cx, |doc, window, cx| {
                    if doc.visible {
                        doc.check(window, cx);
                    }
                });
                if ticked.is_err() {
                    break;
                }
            }
        });
        let mut doc = Self {
            path,
            kind,
            place,
            shown: Shown::Loading,
            stamp: None,
            missing: false,
            focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            search,
            visible: true,
            busy: false,
            version: 0,
            _polling: polling,
            reading: Task::ready(()),
        };
        doc.check(window, cx);
        doc
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    pub fn place(&self) -> Place {
        self.place
    }

    /// Whether the file went since it was read.
    pub fn missing(&self) -> bool {
        self.missing
    }

    /// Polls only while shown; shown again, it looks at once.
    pub fn set_visible(&mut self, visible: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.visible == visible {
            return;
        }
        log::debug!(
            "docs: {} {}",
            self.path.display(),
            if visible { "shows" } else { "hides" }
        );
        self.visible = visible;
        if visible {
            self.check(window, cx);
        }
    }

    /// Looks at the file off the main thread, reading it when its stamp changed.
    fn check(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        let (path, kind, known) = (self.path.clone(), self.kind, self.stamp);
        let found = cx.background_spawn(async move { look(&path, kind, known) });
        self.reading = cx.spawn_in(window, async move |doc, cx| {
            let found = found.await;
            doc.update_in(cx, |doc, window, cx| {
                doc.busy = false;
                doc.take(found, window, cx);
            })
            .ok();
        });
    }

    /// Shows what a look found, drawing again only when it changed.
    fn take(&mut self, found: Found, window: &mut Window, cx: &mut Context<Self>) {
        let path = self.path.display();
        match found {
            Found::Same => {}
            Found::Gone => {
                self.stamp = None;
                if !self.missing {
                    log::warn!("docs: {path} is missing; its last read stays");
                    self.missing = true;
                    cx.notify();
                }
            }
            Found::Failed(why) => {
                if !matches!(&self.shown, Shown::Failed(was) if *was == why) {
                    log::error!("docs: {path} does not read: {why}");
                    self.shown = Shown::Failed(why);
                    cx.notify();
                }
            }
            Found::Changed(stamp, read) => {
                if std::mem::take(&mut self.missing) {
                    log::info!("docs: {path} is back");
                }
                if self.stamp.is_none() && self.version == 0 {
                    log::info!("docs: {path} read");
                } else {
                    log::debug!("docs: {path} changed; read again");
                }
                self.stamp = Some(stamp);
                self.version += 1;
                self.show(read, window, cx);
                cx.notify();
            }
        }
    }

    /// Wears a read: text into its view, an image decoded afresh, or why it cannot show.
    fn show(&mut self, read: Read, window: &mut Window, cx: &mut Context<Self>) {
        self.shown = match read {
            Read::Text(body) => match self.kind {
                Kind::Text | Kind::Html => match &self.shown {
                    Shown::Code(editor) => {
                        let editor = editor.clone();
                        editor.update(cx, |editor, cx| {
                            if editor.text() != body {
                                editor.set_text(body, cx);
                            }
                        });
                        Shown::Code(editor)
                    }
                    _ => {
                        let name = language(&self.path);
                        let editor = cx
                            .new(|cx| CodeEditor::new(body, window, cx).language(name).read_only());
                        // Keys given before the first read go on to the editor.
                        if self.focus.is_focused(window) {
                            window.focus(&editor.focus_handle(cx), cx);
                        }
                        Shown::Code(editor)
                    }
                },
                _ => match &self.shown {
                    Shown::Text(was) if **was == *body => return,
                    _ => Shown::Text(body.into()),
                },
            },
            Read::Image => {
                let source = Resource::Path(self.path.clone().into());
                if matches!(&self.shown, Shown::Image(_)) {
                    cx.remove_asset::<ImageAssetLoader>(&source);
                }
                Shown::Image(source)
            }
            Read::Binary => Shown::Binary,
            Read::TooLarge(size) => Shown::TooLarge(size),
            Read::Failed(why) => {
                log::error!("docs: {} does not read: {why}", self.path.display());
                Shown::Failed(why)
            }
            Read::Missing => unreachable!("a look turns a missing read into Gone"),
        };
    }
}
