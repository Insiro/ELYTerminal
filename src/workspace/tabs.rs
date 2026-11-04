//! The tabs each home holds, and which one each home shows.

use gpui::{Context, EntityId, Focusable, Window};

use super::{Asking, Workspace};
use crate::{
    projects::{Home, TabId},
    sessions::Session,
};

/// A tab of the strip: what it shows, and the home it lives in.
pub struct Tab {
    pub id: TabId,
    pub home: Home,
    pub body: Body,
}

pub use body::Body;

/// The places, in order, of the tabs that live in `home`.
pub fn strip<'a>(homes: impl IntoIterator<Item = &'a Home>, home: &Home) -> Vec<usize> {
    homes
        .into_iter()
        .enumerate()
        .filter_map(|(index, of)| (of == home).then_some(index))
        .collect()
}

/// The tab a strip shows once `closed` leaves it: the next, else the one before.
pub fn neighbor(strip: &[TabId], closed: TabId) -> Option<TabId> {
    let at = strip.iter().position(|&id| id == closed)?;
    strip
        .get(at + 1)
        .or_else(|| at.checked_sub(1).and_then(|before| strip.get(before)))
        .copied()
}

/// The place after `at`, or before it, round the ends of `count`.
pub fn step(at: usize, count: usize, forward: bool) -> usize {
    if forward {
        (at + 1) % count
    } else {
        (at + count - 1) % count
    }
}

/// The places in the whole list of a move from strip place `from` to `to`.
pub fn moved(strip: &[usize], from: usize, to: usize) -> Option<(usize, usize)> {
    Some((*strip.get(from)?, *strip.get(to)?))
}

impl Workspace {
    /// Every session, in the tabs' order.
    pub fn sessions(&self) -> impl Iterator<Item = &Session> {
        self.tabs.iter().filter_map(Tab::term)
    }

    pub(super) fn index(&self, id: TabId) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.id == id)
    }

    /// The tab whose terminal is `terminal`.
    pub(super) fn holding(&self, terminal: EntityId) -> Option<usize> {
        self.tabs.iter().position(|tab| {
            tab.term()
                .is_some_and(|s| s.terminal.entity_id() == terminal)
        })
    }

    /// The tab the selected home shows.
    pub(super) fn current(&self) -> Option<&Tab> {
        let id = *self.shown.get(&self.selected)?;
        self.tabs.iter().find(|tab| tab.id == id)
    }

    pub(super) fn current_id(&self) -> Option<TabId> {
        self.current().map(|tab| tab.id)
    }

    /// The selected home's tabs, in order.
    pub(super) fn strip(&self) -> Vec<TabId> {
        strip(self.tabs.iter().map(|tab| &tab.home), &self.selected)
            .into_iter()
            .map(|index| self.tabs[index].id)
            .collect()
    }

    /// Shows tab `id` in its home, selects that home, clears its marks and gives its terminal the keys.
    pub fn show(&mut self, id: TabId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.index(id) else {
            return;
        };
        if self.current_id() == Some(id) {
            if let Some(session) = self.tabs[index].term() {
                window.focus(&session.terminal.focus_handle(cx), cx);
            }
            return;
        }
        let hidden = self.current().filter(|tab| tab.id != id);
        if let Some(hidden) = hidden.and_then(Tab::term) {
            hidden.terminal.update(cx, |view, cx| {
                view.let_go(cx);
                view.set_front(false, window);
            });
        }
        let tab = &mut self.tabs[index];
        self.selected = tab.home.clone();
        self.shown.insert(tab.home.clone(), id);
        if let Some(session) = tab.term_mut() {
            session.bell = false;
            session.waiting = false;
            session
                .terminal
                .update(cx, |view, _| view.set_front(true, window));
            window.focus(&session.terminal.focus_handle(cx), cx);
            let folder = session.folder.clone();
            self.follow(&folder, cx);
        }
        log::debug!("app: tab {} shown", id.0);
        self.dirty(cx);
        self.retitle(window);
        cx.notify();
    }

    /// Shows no tab: the workspace takes the keys, and the panels show the home's folder.
    pub(super) fn show_none(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        // A gone worktree's panels show its project, else the home folder.
        let folder = match &self.selected {
            Home::Tree { path, .. } if path.is_dir() => Some(path.clone()),
            Home::Tree { project, .. } if project.is_dir() => Some(project.clone()),
            _ => dirs::home_dir(),
        };
        match folder {
            Some(folder) => self.follow(&folder, cx),
            None => log::error!("app: no home folder for the panels"),
        }
        log::debug!("app: home {:?} shows no tab", self.selected);
        self.retitle(window);
        cx.notify();
    }

    /// Closes tab `id`; its program hangs up, and its home shows a neighbor or nothing.
    pub fn close(&mut self, id: TabId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.index(id) else {
            return;
        };
        let home = self.tabs[index].home.clone();
        let strip: Vec<TabId> = strip(self.tabs.iter().map(|tab| &tab.home), &home)
            .into_iter()
            .map(|at| self.tabs[at].id)
            .collect();
        let current = self.current_id() == Some(id);
        if matches!(&self.asking, Some(Asking::RenameTab { id: asked, .. } | Asking::CloseClaude { id: asked }) if *asked == id)
        {
            self.asking = None;
        }
        let tab = self.tabs.remove(index);
        match tab.term() {
            Some(session) => {
                log::info!(
                    "app: tab {} of {} closes",
                    id.0,
                    session.terminal.read(cx).pid()
                );
                // Its textures leave the window's atlas with it.
                session
                    .terminal
                    .update(cx, |view, _| view.set_front(false, window));
            }
            None => log::info!("app: tab {} closes", id.0),
        }
        self.dirty(cx);
        if self.shown.get(&home) != Some(&id) {
            cx.notify();
            return;
        }
        match neighbor(&strip, id) {
            Some(next) if current => self.show(next, window, cx),
            Some(next) => {
                self.shown.insert(home, next);
                cx.notify();
            }
            None => {
                log::info!("app: no tab left in {home:?}");
                self.shown.remove(&home);
                if current {
                    self.show_none(window, cx);
                }
            }
        }
    }

    /// Shows the selected home's next tab, or the one before, round the ends.
    pub(super) fn step(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let strip = self.strip();
        let Some(at) = self
            .current_id()
            .and_then(|id| strip.iter().position(|&tab| tab == id))
        else {
            return;
        };
        self.show(strip[step(at, strip.len(), forward)], window, cx);
    }

    /// Moves the strip's tab `from` to place `to`, as a drag asks.
    pub fn reorder(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        let places = strip(self.tabs.iter().map(|tab| &tab.home), &self.selected);
        let Some((at, into)) = moved(&places, from, to).filter(|(at, into)| at != into) else {
            return;
        };
        let tab = self.tabs.remove(at);
        log::info!(
            "app: tab {} moves from strip place {from} to {to}",
            tab.id.0
        );
        self.tabs.insert(into, tab);
        self.dirty(cx);
        cx.notify();
    }
}

mod body;
#[cfg(test)]
mod tests;
