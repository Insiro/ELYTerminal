//! The left column's homes: Others alone until projects come.

use std::time::Instant;

use ely_gpui_component::{
    i18n::text,
    lists::ListItem,
    primitives::{Icon, IconName},
    theme::ActiveTheme,
};
use gpui::{Context, IntoElement, ParentElement, Styled, Window, div};

use super::{Tab, Workspace};
use crate::{
    projects::Home,
    sessions::{self, Need, Session, orb::Look},
};

impl Workspace {
    /// Selects `home`, showing the tab it showed last, else nothing.
    pub(super) fn select_home(&mut self, home: Home, window: &mut Window, cx: &mut Context<Self>) {
        match self.shown.get(&home) {
            Some(&id) => self.show(id, window, cx),
            None => {
                if let Some(tab) = self.current() {
                    let terminal = tab.session().terminal.clone();
                    terminal.update(cx, |view, cx| {
                        view.let_go(cx);
                        view.set_front(false, window);
                    });
                }
                self.selected = home;
                self.show_none(window, cx);
            }
        }
    }

    /// The Others tab that asks the most of the user, when one asks anything.
    fn loudest(&self) -> Option<&Session> {
        self.tabs
            .iter()
            .filter(|tab| tab.home == Home::Others)
            .map(Tab::session)
            .filter(|session| session.need() > Need::Calm || session.look.is_some())
            .max_by_key(|session| session.need())
    }

    /// The session whose orb the homes draw: a bell or a hidden wait draws a bell instead.
    pub(super) fn drawn(&self) -> Option<&Session> {
        self.loudest().filter(|session| !rung(session))
    }

    /// The homes' rows, each with the mark of its most pressing tab.
    pub(super) fn homes(&self, now: Instant, cx: &Context<Self>) -> impl IntoElement {
        let workspace = cx.entity();
        let mut row = ListItem::new("home-others", text(cx, "et.projects.others", &[]))
            .selected(self.selected == Home::Others)
            .on_click(move |_, window, cx| {
                workspace.update(cx, |this, cx| this.select_home(Home::Others, window, cx))
            });
        let mark = match self.loudest() {
            Some(session) if rung(session) => Some(
                Icon::new(IconName::Bell)
                    .color(cx.theme().colors.warning)
                    .into_any_element(),
            ),
            Some(session) => sessions::mark(session, now, cx),
            None => None,
        };
        if let Some(mark) = mark {
            row = row.leading(mark);
        }
        div().p_2().child(row)
    }
}

/// A bell, or a wait heard while hidden that its orb does not show.
fn rung(session: &Session) -> bool {
    let need = session.need();
    need == Need::Bell
        || (need == Need::Waiting && !matches!(session.look, Some((Look::Waiting, _))))
}
