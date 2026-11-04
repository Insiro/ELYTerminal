//! What a tab shows: a terminal, or a document.

use gpui::{App, Entity, Focusable, Window};

use super::Tab;
use crate::{docs::Document, sessions::Session};

pub enum Body {
    Term(Box<Session>),
    Doc(Entity<Document>),
}

/// A document's title: its file's name, else its path.
pub fn doc_title(doc: &Document) -> String {
    let path = doc.path();
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().to_string(),
    )
}

impl Tab {
    /// The tab's terminal session, when it shows one.
    pub fn term(&self) -> Option<&Session> {
        match &self.body {
            Body::Term(session) => Some(session),
            Body::Doc(_) => None,
        }
    }

    pub fn term_mut(&mut self) -> Option<&mut Session> {
        match &mut self.body {
            Body::Term(session) => Some(session),
            Body::Doc(_) => None,
        }
    }

    /// The tab's document, when it shows one.
    pub fn doc(&self) -> Option<&Entity<Document>> {
        match &self.body {
            Body::Doc(doc) => Some(doc),
            Body::Term(_) => None,
        }
    }

    /// What the strip and the window call it.
    pub fn title(&self, cx: &App) -> String {
        match &self.body {
            Body::Term(session) => session.named().to_string(),
            Body::Doc(doc) => doc_title(doc.read(cx)),
        }
    }

    /// Gives the tab the keys.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        let handle = match &self.body {
            Body::Term(session) => session.terminal.focus_handle(cx),
            Body::Doc(doc) => doc.focus_handle(cx),
        };
        window.focus(&handle, cx);
    }

    /// Steps the tab out of view: a terminal ends its gestures, a document stops polling.
    pub fn hide(&self, window: &mut Window, cx: &mut App) {
        match &self.body {
            Body::Term(session) => session.terminal.update(cx, |view, cx| {
                view.let_go(cx);
                view.set_front(false, window);
            }),
            Body::Doc(doc) => doc.update(cx, |doc, cx| doc.set_visible(false, window, cx)),
        }
    }
}
