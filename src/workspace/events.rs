use gpui::{Context, EntityId, Window};

use super::Workspace;
use crate::term::{State, TerminalEvent};

impl Workspace {
    /// What the terminal `id` reported.
    pub(super) fn session_event(
        &mut self,
        id: EntityId,
        event: &TerminalEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.holding(id) else {
            return;
        };
        if let TerminalEvent::Marked(mark) = event {
            self.claude_marked(index, *mark, window, cx);
            return;
        }
        let tab = self.tabs[index].id;
        let shown = self.current_id() == Some(tab);
        let Some(session) = self.tabs[index].term_mut() else {
            return;
        };
        match event {
            TerminalEvent::Title(title) => {
                log::debug!("app: tab {}'s program names it {title:?}", tab.0);
                session.title.clone_from(title);
            }
            TerminalEvent::Folder(folder) => {
                session.folder.clone_from(folder);
                session.reports = true;
                if shown {
                    self.files.update(cx, |files, cx| files.show(folder, cx));
                    self.git.update(cx, |git, cx| git.show(folder, cx));
                    self.keep_preview(folder, cx);
                }
            }
            TerminalEvent::Bell if !shown => {
                if !session.bell {
                    log::debug!("app: a bell in hidden tab {}", tab.0);
                }
                session.bell = true;
            }
            TerminalEvent::Bell => {}
            // A hidden tab that starts waiting stays marked until it is shown.
            TerminalEvent::Shows(Some(State::Blocked)) if !shown => {
                log::debug!("app: hidden tab {} waits", tab.0);
                session.waiting = true;
            }
            TerminalEvent::Shows(_) | TerminalEvent::Ended(_) | TerminalEvent::Marked(_) => {}
            TerminalEvent::Dragged(key) => {
                if !shown {
                    return;
                }
                match self.files.read(cx).path(key) {
                    Some(path) => {
                        log::info!("app: {} dragged onto the terminal", path.display());
                        session
                            .terminal
                            .update(cx, |view, cx| view.type_paths(&[path], cx));
                    }
                    None => log::warn!("app: a tree drag of {key} types nothing"),
                }
            }
            TerminalEvent::Exited(status) => {
                let pid = session.terminal.read(cx).pid();
                log::info!("app: tab {}'s program {pid} ended, {status}", tab.0);
                self.close(tab, window, cx);
                return;
            }
        }
        if shown {
            self.retitle(window, cx);
        }
        cx.notify();
    }

    /// A press on notification `tag`: its tab hears of it and shows in its home, unless it asked not to; false when no tab here posted it.
    pub fn pressed(&mut self, tag: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let found = self.tabs.iter().find_map(|tab| {
            let terminal = &tab.term()?.terminal;
            let focus = terminal.update(cx, |view, _| view.pressed(tag))?;
            Some((tab.id, focus))
        });
        let Some((id, focus)) = found else {
            return false;
        };
        if focus {
            cx.activate(true);
            window.activate_window();
            self.show(id, window, cx);
        }
        true
    }
}
