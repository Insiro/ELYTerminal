//! The questions the workspace asks over its window.

use ely_gpui_component::{
    forms::TextInput,
    i18n::text,
    overlays::{ConfirmDialog, PromptDialog},
};
use gpui::{AnyElement, AppContext, Context, Entity, IntoElement, Window};

use super::{
    TITLE, Workspace,
    projects::{folder_name, tabs_of},
};
use crate::projects::TabId;

/// What the workspace is asking.
pub(super) enum Asking {
    RenameTab {
        id: TabId,
        field: Entity<TextInput>,
    },
    RenameProject {
        id: u64,
        field: Entity<TextInput>,
    },
    RemoveProject {
        id: u64,
    },
    /// Closing a tab Claude still runs in.
    CloseClaude {
        id: TabId,
    },
    RemoveWorktree(super::worktrees::Removal),
}

impl Workspace {
    /// Asks what tab `id` should be called, the name it has selected and its program's title beneath.
    pub fn rename(&mut self, id: TabId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.index(id) else {
            return;
        };
        let Some(session) = self.tabs[index].term() else {
            return;
        };
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
        let Some(session) = self.tabs[index].term_mut() else {
            return;
        };
        session.name = name.trim().to_string();
        if session.name.is_empty() {
            log::info!("app: tab {} takes its program's title again", id.0);
        } else {
            log::info!("app: tab {} is named {:?}", id.0, session.name);
        }
        self.dirty(cx);
        self.retitle(window, cx);
        cx.notify();
    }

    /// Asks what project `id` should be called, its folder's name beneath.
    pub fn ask_rename_project(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.project(id) else {
            return;
        };
        let (name, folder) = (
            project.name.clone().unwrap_or_default(),
            folder_name(&project.folder),
        );
        let field = cx.new(|cx| {
            let mut input = TextInput::new(window, cx);
            input.set_text(name, cx);
            input.set_placeholder(folder, cx);
            input
        });
        log::debug!("app: project {id} asks for a name");
        self.asking = Some(Asking::RenameProject { id, field });
        cx.notify();
    }

    /// Gives the shown tab the keys again, else the workspace.
    fn refocus(&self, window: &mut Window, cx: &mut Context<Self>) {
        match self.current() {
            Some(tab) => tab.focus(window, cx),
            None => window.focus(&self.focus, cx),
        }
    }

    pub(super) fn dialog(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let close = cx.entity();
        let closed = move |window: &mut Window, cx: &mut gpui::App| {
            close.update(cx, |this, cx| {
                this.asking = None;
                this.refocus(window, cx);
                cx.notify();
            })
        };
        let submit = cx.entity();
        Some(match self.asking.as_ref()? {
            Asking::RemoveWorktree(removal) => self.removal_dialog(removal, closed, cx)?,
            Asking::RenameTab { id, field } => {
                let id = *id;
                PromptDialog::new(
                    "rename-tab",
                    text(cx, "et.tab.rename_title", &[]),
                    field,
                    closed,
                )
                .submit(text(cx, "et.tab.rename_submit", &[]))
                .on_submit(move |name, window, cx| {
                    submit.update(cx, |this, cx| {
                        this.name(id, name, window, cx);
                        this.refocus(window, cx);
                    })
                })
                .into_any_element()
            }
            Asking::RenameProject { id, field } => {
                let id = *id;
                let title = text(cx, "et.projects.rename_title", &[]);
                PromptDialog::new("rename-project", title, field, closed)
                    .submit(text(cx, "et.tab.rename_submit", &[]))
                    .on_submit(move |name, window, cx| {
                        submit.update(cx, |this, cx| {
                            this.name_project(id, name, window, cx);
                            this.refocus(window, cx);
                        })
                    })
                    .into_any_element()
            }
            Asking::RemoveProject { id } => {
                let id = *id;
                let project = self.project(id)?;
                let open = tabs_of(
                    self.tabs.iter().map(|tab| (tab.id, &tab.home)),
                    &project.folder,
                );
                let label = project.label();
                let title = text(cx, "et.projects.remove_title", &[("name", &label)]);
                let message = match open.len() {
                    0 => text(cx, "et.projects.remove_body", &[]),
                    count => text(
                        cx,
                        "et.projects.remove_running",
                        &[("count", &count.to_string())],
                    ),
                };
                let claude = open
                    .iter()
                    .filter_map(|id| self.index(*id))
                    .filter(|at| self.tabs[*at].term().is_some_and(|s| s.claude.is_some()))
                    .count();
                let message = match claude {
                    0 => message,
                    count => {
                        let count = count.to_string();
                        let more = text(cx, "et.projects.remove_claude", &[("count", &count)]);
                        format!("{message} {more}").into()
                    }
                };
                ConfirmDialog::new("remove-project", title, message, closed)
                    .confirm(text(cx, "et.projects.remove_confirm", &[]))
                    .destructive()
                    .on_confirm(move |window, cx| {
                        submit.update(cx, |this, cx| this.remove_project(id, window, cx))
                    })
                    .into_any_element()
            }
            Asking::CloseClaude { id } => {
                let id = *id;
                let title = text(cx, "et.claude.close_title", &[]);
                let message = text(cx, "et.claude.close_body", &[]);
                ConfirmDialog::new("close-claude", title, message, closed)
                    .confirm(text(cx, "et.claude.close_confirm", &[]))
                    .destructive()
                    .on_confirm(move |window, cx| {
                        submit.update(cx, |this, cx| {
                            this.close(id, window, cx);
                            this.refocus(window, cx);
                        })
                    })
                    .into_any_element()
            }
        })
    }
}
