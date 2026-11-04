//! The strip of the selected home's tabs over the terminal, and what shows with none.

use ely_gpui_component::{
    buttons::{Button, ButtonVariant, IconButton},
    feedback::EmptyState,
    i18n::text,
    menus::OverflowMenu,
    navigation::{EditorTab, EditorTabs},
    primitives::IconName,
    theme::ControlSize,
};
use gpui::{Context, IntoElement, ParentElement, SharedString, Styled, div};

use super::Workspace;
use crate::{
    projects::TabId,
    sessions::{Need, menu},
    settings::SettingsStore,
};

/// A tab's icon: the shape of its need, as the strip draws every icon in one color.
fn icon(need: Need) -> IconName {
    match need {
        Need::Error => IconName::CircleX,
        Need::Waiting => IconName::Hourglass,
        Need::Bell => IconName::Bell,
        Need::Working => IconName::LoaderCircle,
        Need::Done => IconName::CircleCheck,
        Need::Calm => IconName::Terminal,
    }
}

fn key(id: TabId) -> SharedString {
    id.0.to_string().into()
}

/// The tab a strip key names.
fn tab_of(key: &SharedString) -> Option<TabId> {
    match key.parse() {
        Ok(id) => Some(TabId(id)),
        Err(error) => {
            log::error!("app: the strip names no tab {key:?}: {error}");
            None
        }
    }
}

impl Workspace {
    pub(super) fn tab_strip(&self, cx: &Context<Self>) -> impl IntoElement {
        let settings = SettingsStore::get(cx);
        let workspace = cx.entity();
        let tabs = self.strip().into_iter().filter_map(|id| {
            let session = self.tabs[self.index(id)?].session();
            Some(EditorTab::new(key(id), session.named().to_string()).icon(icon(session.need())))
        });
        let (select, close) = (workspace.clone(), workspace.clone());
        let mut strip = EditorTabs::new("tabs", tabs)
            .on_select(move |key, window, cx| {
                if let Some(id) = tab_of(key) {
                    select.update(cx, |this, cx| this.show(id, window, cx))
                }
            })
            .on_close(move |key, window, cx| {
                if let Some(id) = tab_of(key) {
                    close.update(cx, |this, cx| this.close(id, window, cx))
                }
            });
        if let Some(id) = self.current_id() {
            strip = strip.selected(key(id));
        }
        if settings.drag_sessions {
            let dragged = workspace.clone();
            strip = strip.on_reorder(move |from, to, _, cx| {
                dragged.update(cx, |this, cx| this.reorder(from, to, cx))
            });
        }
        let new = workspace.clone();
        let plus = IconButton::new("new-tab", IconName::Plus)
            .variant(ButtonVariant::Ghost)
            .size(ControlSize::Sm)
            .tooltip(text(cx, "et.session.new", &[]))
            .on_click(move |_, window, cx| new.update(cx, |this, cx| this.new_session(window, cx)));
        let more = self.current_id().map(|id| {
            let menu = menu::tab(&workspace, id, settings.session_menu, cx);
            OverflowMenu::new("tab-menu", menu).tooltip(text(cx, "et.tab.more", &[]))
        });
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap_1()
            .pr_2()
            .child(div().flex_1().min_w_0().child(strip))
            .child(plus)
            .children(crate::profiles::button(&workspace, &self.found, cx))
            .children(more)
    }

    /// What the terminal's place shows while the home holds no tab.
    pub(super) fn empty(&self, cx: &Context<Self>) -> impl IntoElement {
        let workspace = cx.entity();
        let new = Button::new("empty-new-tab", text(cx, "et.session.new", &[]))
            .icon(IconName::Plus)
            .on_click(move |_, window, cx| {
                workspace.update(cx, |this, cx| this.new_session(window, cx))
            });
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                EmptyState::new("no-tab", IconName::Terminal, text(cx, "et.tab.empty", &[]))
                    .body(text(cx, "et.tab.empty_hint", &[]))
                    .action(new),
            )
    }
}
