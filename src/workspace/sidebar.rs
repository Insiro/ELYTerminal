//! The left column's rows: each project, its linked worktrees, then Others.

use std::{path::Path, time::Instant};

use ely_gpui_component::{
    i18n::text,
    layout::Collapsible,
    lists::ListItem,
    menus::ContextMenu,
    motion::Reorder,
    primitives::{Disclosure, Icon, IconName},
    theme::ActiveTheme,
};
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window, div,
};

use super::{
    Tab, Workspace,
    projects::{Project, Row, Trees, children},
};
use crate::{
    projects::Home,
    sessions::{self, Need, Session, orb::Look},
    settings::SettingsStore,
};

mod menu;

impl Workspace {
    /// Selects `home`, waking its saved tabs, showing the tab it showed last, else nothing.
    pub(super) fn select_home(&mut self, home: Home, window: &mut Window, cx: &mut Context<Self>) {
        let woken = self.wake(&home, window, cx);
        match woken.or_else(|| self.shown.get(&home).copied()) {
            Some(id) => self.show(id, window, cx),
            None => {
                if let Some(tab) = self.current() {
                    let terminal = tab.session().terminal.clone();
                    terminal.update(cx, |view, cx| {
                        view.let_go(cx);
                        view.set_front(false, window);
                    });
                }
                log::info!("app: home {home:?} selected");
                self.selected = home;
                self.dirty(cx);
                self.show_none(window, cx);
            }
        }
    }

    /// The tab of `row` that asks the most of the user, when one asks anything.
    fn loudest(&self, row: &Row) -> Option<&Session> {
        self.tabs
            .iter()
            .filter(|tab| row.covers(&tab.home))
            .map(Tab::session)
            .filter(|session| session.need() > Need::Calm || session.look.is_some())
            .max_by_key(|session| session.need())
    }

    /// Others shows while it holds tabs, is selected, or no project is listed.
    fn others_shown(&self) -> bool {
        self.projects.is_empty()
            || self.selected == Home::Others
            || self.tabs.iter().any(|tab| tab.home == Home::Others)
            || self.dormant.contains_key(&Home::Others)
    }

    /// Some tab's orb moves `now`, so a row may draw it.
    pub(super) fn marks_move(&self, now: Instant) -> bool {
        self.tabs
            .iter()
            .map(Tab::session)
            .any(|session| !rung(session) && session.moves(now))
    }

    /// The mark of `row`'s most pressing tab: a bell or a hidden wait draws a bell.
    fn mark(&self, row: &Row, now: Instant, cx: &Context<Self>) -> Option<AnyElement> {
        match self.loudest(row)? {
            session if rung(session) => Some(
                Icon::new(IconName::Bell)
                    .color(cx.theme().colors.warning)
                    .into_any_element(),
            ),
            session => sessions::mark(session, now, cx),
        }
    }

    /// The homes' rows, each with the mark of its most pressing tab.
    pub(super) fn homes(&self, now: Instant, cx: &Context<Self>) -> impl IntoElement {
        let workspace = cx.entity();
        let home_dir = dirs::home_dir();
        let blocks = self.projects.iter().map(|project| {
            let block = self.project_block(project, now, home_dir.as_deref(), cx);
            (project.id.to_string(), block)
        });
        let projects = if SettingsStore::get(cx).drag_sessions {
            let dragged = workspace.clone();
            blocks
                .fold(Reorder::new("project-rows"), |list, (key, block)| {
                    list.row(key, block)
                })
                .on_reorder(move |from, to, _, cx| {
                    dragged.update(cx, |this, cx| this.reorder_projects(from, to, cx))
                })
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .children(blocks.map(|(_, block)| block))
                .into_any_element()
        };
        let others = self.others_shown().then(|| {
            let mut row = ListItem::new("home-others", text(cx, "et.projects.others", &[]))
                .leading(Icon::new(IconName::Terminal))
                .selected(self.selected == Home::Others)
                .on_click(move |_, window, cx| {
                    workspace.update(cx, |this, cx| this.select_home(Home::Others, window, cx))
                });
            if let Some(mark) = self.mark(&Row::Others, now, cx) {
                row = row.trailing(mark);
            }
            row
        });
        div()
            .id("homes")
            .size_full()
            .overflow_y_scroll()
            .p_2()
            .flex()
            .flex_col()
            .child(projects)
            .children(others)
    }

    /// A project's row, its menu, and its linked worktrees beneath while open.
    fn project_block(
        &self,
        project: &Project,
        now: Instant,
        home_dir: Option<&Path>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let workspace = cx.entity();
        let (id, home) = (project.id, project.home());
        let homes = self.tabs.iter().map(|tab| &tab.home);
        let trees = children(project, homes.chain(self.dormant.keys()));
        let toggle = workspace.clone();
        let disclosure = (!trees.is_empty()).then(|| {
            div()
                .id(("project-disclose", id as usize))
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    toggle.update(cx, |this, cx| this.toggle_project(id, cx))
                })
                .child(Disclosure::new(("project-open", id as usize), project.open))
        });
        let leading = div()
            .flex()
            .items_center()
            .gap_1()
            .children(disclosure)
            .child(match project.listing.error {
                Some(_) => Icon::new(IconName::TriangleAlert).color(cx.theme().colors.warning),
                None => Icon::new(IconName::Folder),
            });
        let description = match (&project.listing.error, &project.trees) {
            (Some(_), _) => Some(text(cx, "et.projects.failed", &[]).to_string()),
            (None, Trees::Git(_)) => project.branch().map(str::to_string),
            (None, Trees::Plain) => Some(sessions::shown(&project.folder, home_dir)),
            (None, Trees::Pending) => None,
        };
        let row = Row::Project {
            folder: project.folder.clone(),
            open: project.open,
        };
        let select = workspace.clone();
        let chosen = home.clone();
        let mut item = ListItem::new(("project", id as usize), project.label())
            .leading(leading)
            .selected(self.selected == home || (!project.open && row.covers(&self.selected)))
            .on_click(move |_, window, cx| {
                select.update(cx, |this, cx| this.select_home(chosen.clone(), window, cx))
            });
        if let Some(description) = description {
            item = item.description(description);
        }
        if let Some(mark) = self.mark(&row, now, cx) {
            item = item.trailing(mark);
        }
        let menu = menu::project(&workspace, project, cx);
        let head = ContextMenu::new(("project-menu", id as usize), menu).child(item);
        let rows = trees.into_iter().map(|tree| {
            let home = Home::Tree {
                project: project.folder.clone(),
                path: tree.path.clone(),
            };
            let key = format!("{id}:{}", tree.path.display());
            let select = workspace.clone();
            let chosen = home.clone();
            let mut item = ListItem::new(gpui::ElementId::Name(key.clone().into()), tree.label)
                .leading(Icon::new(IconName::GitBranch))
                .selected(self.selected == home)
                .on_click(move |_, window, cx| {
                    select.update(cx, |this, cx| this.select_home(chosen.clone(), window, cx))
                });
            item = item.description(if tree.missing {
                text(cx, "et.projects.missing", &[]).to_string()
            } else {
                sessions::shown(&tree.path, home_dir)
            });
            if let Some(mark) = self.mark(&Row::Tree(home.clone()), now, cx) {
                item = item.trailing(mark);
            }
            let menu = menu::worktree(&workspace, home, tree.path, cx);
            ContextMenu::new(gpui::ElementId::Name(format!("menu:{key}").into()), menu)
                .pl_4()
                .child(item)
        });
        div()
            .flex()
            .flex_col()
            .child(head)
            .child(Collapsible::new(("project-trees", id as usize), project.open).children(rows))
            .into_any_element()
    }
}

/// A bell, or a wait heard while hidden that its orb does not show.
fn rung(session: &Session) -> bool {
    let need = session.need();
    need == Need::Bell
        || (need == Need::Waiting && !matches!(session.look, Some((Look::Waiting, _))))
}
