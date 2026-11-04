//! The questions the workspace asks over its window.

use ely_gpui_component::{forms::TextInput, i18n::text, overlays::PromptDialog};
use gpui::{AppContext, Context, Entity, Focusable, Window};

use super::{TITLE, Workspace};
use crate::projects::TabId;

/// What the workspace is asking.
pub(super) enum Asking {
    RenameTab { id: TabId, field: Entity<TextInput> },
}

impl Workspace {
    /// Asks what tab `id` should be called, the name it has selected and its program's title beneath.
    pub fn rename(&mut self, id: TabId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.index(id) else {
            return;
        };
        let session = self.tabs[index].session();
        let (name, title) = (session.name.clone(), session.title.clone());
        let field = cx.new(|cx| {
            let mut input = TextInput::new(window, cx);
            input.set_text(name, cx);
            input.set_placeholder(
                if title.is_empty() {
                    TITLE.into()
                } else {
                    title
                },
                cx,
            );
            input
        });
        log::debug!("app: tab {} asks for a name", id.0);
        self.asking = Some(Asking::RenameTab { id, field });
        cx.notify();
    }

    /// Names tab `id` `name`, trimmed; empty gives the title back to its program.
    fn name(&mut self, id: TabId, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.index(id) else {
            log::debug!("app: the tab renamed is gone");
            return;
        };
        let session = self.tabs[index].session_mut();
        session.name = name.trim().to_string();
        if session.name.is_empty() {
            log::info!("app: tab {} takes its program's title again", id.0);
        } else {
            log::info!("app: tab {} is named {:?}", id.0, session.name);
        }
        self.retitle(window);
        cx.notify();
    }

    /// Gives the shown tab the keys again, else the workspace.
    fn refocus(&self, window: &mut Window, cx: &mut Context<Self>) {
        match self.current() {
            Some(tab) => window.focus(&tab.session().terminal.focus_handle(cx), cx),
            None => window.focus(&self.focus, cx),
        }
    }

    pub(super) fn dialog(&self, cx: &Context<Self>) -> Option<PromptDialog> {
        let Asking::RenameTab { id, field } = self.asking.as_ref()?;
        let id = *id;
        let (close, submit) = (cx.entity(), cx.entity());
        let dialog =
            PromptDialog::new("rename-tab", text(cx, "et.tab.rename_title", &[]), field, {
                move |window, cx| {
                    close.update(cx, |this, cx| {
                        this.asking = None;
                        this.refocus(window, cx);
                        cx.notify();
                    })
                }
            });
        Some(
            dialog
                .submit(text(cx, "et.tab.rename_submit", &[]))
                .on_submit(move |name, window, cx| {
                    submit.update(cx, |this, cx| {
                        this.name(id, name, window, cx);
                        this.refocus(window, cx);
                    })
                }),
        )
    }
}
