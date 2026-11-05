//! The tab strip's row in Ely's `EditorTabs` look, drawn here so a dragged tab lands wherever the pointer lets go.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};

use ely_gpui_component::{
    buttons::{ButtonVariant, IconButton},
    forms::Choice,
    i18n::text,
    navigation::TabOverflowMenu,
    primitives::{DragGhost, Icon, IconName},
    theme::{ActiveTheme, ControlSize, IconSize, TextSize},
};
use gpui::{
    AnyElement, App, Bounds, Context, Div, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Pixels, Role, ScrollHandle, SharedString, Stateful, StatefulInteractiveElement,
    Styled, canvas, div, prelude::FluentBuilder,
};

use super::{Workspace, key, tab_of};
use crate::{
    projects::TabId,
    workspace::sidebar::{moved, place},
};

/// A tab on its way, by id.
struct TabDrag(TabId);

/// A tab the row shows.
pub(super) struct Shown {
    pub id: TabId,
    pub title: SharedString,
    pub icon: IconName,
}

/// The tab held, and the place it would take: before tab `target`, or last at the count.
#[derive(Clone, Copy)]
struct Held {
    id: TabId,
    target: usize,
}

/// The held tab, each tab's bounds from the last paint, and the row's scroll.
#[derive(Default)]
pub(in crate::workspace) struct TabHold {
    held: Option<Held>,
    bounds: Rc<RefCell<HashMap<TabId, Bounds<Pixels>>>>,
    scroll: ScrollHandle,
    /// The selected tab the row last scrolled to.
    followed: Rc<Cell<Option<Option<TabId>>>>,
}

impl TabHold {
    /// Drops the tab held, saying whether one was.
    pub(in crate::workspace) fn cancel(&mut self) -> bool {
        self.held.take().is_some()
    }
}

impl Workspace {
    /// The place a drag would move a tab to, while one would move.
    fn tab_place(&self, strip: &[TabId], cx: &App) -> Option<usize> {
        let held = self.tab_hold.held.filter(|_| cx.has_active_drag())?;
        let from = strip.iter().position(|&id| id == held.id)?;
        moved(from, held.target).map(|_| held.target)
    }

    fn lift_tab(&mut self, id: TabId, cx: &mut Context<Self>) {
        let Some(from) = self.strip().iter().position(|&tab| tab == id) else {
            return;
        };
        log::info!("app: tab {} lifted from strip place {from}", id.0);
        self.tab_hold.held = Some(Held { id, target: from });
        cx.notify();
    }

    fn aim_tab(&mut self, x: Pixels, cx: &mut Context<Self>) {
        let Some(held) = self.tab_hold.held else {
            return;
        };
        let bounds = self.tab_hold.bounds.borrow();
        let spans: Option<Vec<_>> = self
            .strip()
            .iter()
            .map(|id| bounds.get(id).map(|b| (b.left(), b.right())))
            .collect();
        let Some(spans) = spans else { return };
        let target = place(&spans, x);
        drop(bounds);
        if target != held.target {
            log::debug!("app: tab drag at {x:?} aims at strip place {target}");
            self.tab_hold.held = Some(Held { target, ..held });
            cx.notify();
        }
    }

    /// A release anywhere moves the held tab to its shown place.
    fn land_tab(&mut self, cx: &mut Context<Self>) {
        let Some(held) = self.tab_hold.held.take() else {
            return;
        };
        cx.notify();
        if !cx.has_active_drag() {
            log::debug!("app: a tab drag ended before its release");
            return;
        }
        let Some(from) = self.strip().iter().position(|&id| id == held.id) else {
            log::info!("app: the tab dragged is gone");
            return;
        };
        match moved(from, held.target) {
            Some(to) => self.reorder(from, to, cx),
            None => log::info!("app: tab {} dropped in its place", held.id.0),
        }
    }

    /// The row of `tabs`, `selected` marked; with `drags` a tab moves by a drag.
    pub(super) fn tab_row(
        &self,
        tabs: Vec<Shown>,
        selected: Option<TabId>,
        drags: bool,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let ids: Vec<TabId> = tabs.iter().map(|tab| tab.id).collect();
        let place = self.tab_place(&ids, cx);
        let count = tabs.len();
        let theme = cx.theme();
        let colors = &theme.colors;
        let line = || {
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .w(theme.tab_indicator())
                .bg(colors.accent)
        };
        let listed: Vec<Choice> = tabs
            .iter()
            .map(|tab| Choice::new(key(tab.id), tab.title.clone()).icon(tab.icon))
            .collect();
        let row: Vec<AnyElement> = tabs
            .into_iter()
            .enumerate()
            .map(|(ix, tab)| {
                let Shown { id, title, icon } = tab;
                let on = selected == Some(id);
                let held = drags && self.tab_hold.held.is_some_and(|h| h.id == id);
                let held = held && cx.has_active_drag();
                let group = SharedString::from(format!("strip-tab-{}", id.0));
                let fg = if on { colors.fg } else { colors.fg_muted };
                let (select, close, middle) = (cx.entity(), cx.entity(), cx.entity());
                let close_button = IconButton::new(("close-tab", id.0 as usize), IconName::X)
                    .size(ControlSize::Sm)
                    .variant(ButtonVariant::Ghost)
                    .tooltip(text(cx, "dialog.close", &[]))
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        close.update(cx, |this, cx| this.ask_close(id, window, cx))
                    });
                let slot = div()
                    .relative()
                    .flex_none()
                    .size(theme.control_height(ControlSize::Sm))
                    .child(
                        div()
                            .absolute()
                            .size_full()
                            // Clear, not hidden: it still takes a press.
                            .when(!on, |cover| {
                                cover
                                    .opacity(0.)
                                    .group_hover(group.clone(), |style| style.opacity(1.))
                            })
                            .child(close_button),
                    );
                let bounds = self.tab_hold.bounds.clone();
                let workspace = cx.entity();
                let ghost = title.clone();
                div()
                    .id(("strip-tab", id.0 as usize))
                    .role(Role::Tab)
                    .aria_selected(on)
                    .aria_label(title.clone())
                    .group(group)
                    .relative()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap_1p5()
                    .max_w(theme.tab_width().1)
                    .h(theme.control_height(ControlSize::Lg))
                    .pl_3()
                    .pr_1()
                    .border_r_1()
                    .border_color(colors.border)
                    .text_size(theme.text_size(TextSize::Sm))
                    .text_color(fg)
                    .map(|el| {
                        if on {
                            el.bg(colors.bg).child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .right_0()
                                    .h(theme.tab_indicator())
                                    .bg(colors.accent),
                            )
                        } else {
                            el.hover(|style| style.text_color(colors.fg))
                        }
                    })
                    .when(held, |el| el.opacity(0.5))
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                    .on_mouse_down(MouseButton::Middle, move |_, window, cx| {
                        middle.update(cx, |this, cx| this.ask_close(id, window, cx))
                    })
                    .on_click(move |_, window, cx| {
                        select.update(cx, |this, cx| this.show(id, window, cx))
                    })
                    .when(drags, |el| {
                        el.on_drag(TabDrag(id), move |drag, _, _, cx| {
                            workspace.update(cx, |this, cx| this.lift_tab(drag.0, cx));
                            DragGhost::new(ghost.clone(), Some(icon), cx)
                        })
                    })
                    .child(Icon::new(icon).size(IconSize::Sm).color(fg))
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(title),
                    )
                    .child(slot)
                    .child(
                        canvas(
                            move |b, _, _| {
                                bounds.borrow_mut().insert(id, b);
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                    )
                    .when(place == Some(ix), |el| el.child(line().left_0()))
                    .when(place == Some(count) && ix + 1 == count, |el| {
                        el.child(line().right_0())
                    })
                    .into_any_element()
            })
            .collect();
        let followed = self.tab_hold.followed.clone();
        let follow = (followed.get() != Some(selected)).then(|| {
            let scroll = self.tab_hold.scroll.clone();
            let chosen = selected.and_then(|id| ids.iter().position(|&tab| tab == id));
            canvas(
                move |_, window, _| {
                    if scroll.bounds().size.width <= Pixels::ZERO {
                        return;
                    }
                    followed.set(Some(selected));
                    if let Some(ix) = chosen {
                        scroll.scroll_to_item(ix);
                        window.request_animation_frame();
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
        });
        let overflow = (!listed.is_empty()).then(|| {
            let pick = cx.entity();
            let menu =
                TabOverflowMenu::new("tabs-overflow", listed).on_select(move |key, window, cx| {
                    if let Some(id) = tab_of(key) {
                        pick.update(cx, |this, cx| this.show(id, window, cx))
                    }
                });
            let menu = match selected {
                Some(id) => menu.selected(key(id)),
                None => menu,
            };
            div().flex_none().px_1().child(menu)
        });
        let strip = div()
            .id("tabs")
            .flex()
            .items_center()
            .bg(colors.sunken)
            .border_b_1()
            .border_color(colors.border)
            .child(
                div()
                    .id("strip")
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .overflow_x_scroll()
                    .track_scroll(&self.tab_hold.scroll)
                    .children(row)
                    .children(follow),
            )
            .children(overflow);
        if drags {
            self.track_tabs(strip, cx)
        } else {
            strip
        }
    }

    /// The pointer's place across aims the held tab wherever it goes, and any release lands it.
    fn track_tabs(&self, strip: Stateful<Div>, cx: &Context<Self>) -> Stateful<Div> {
        let (aim, up, out) = (cx.entity(), cx.entity(), cx.entity());
        strip
            .on_drag_move::<TabDrag>(move |event, _, cx| {
                aim.update(cx, |this, cx| this.aim_tab(event.event.position.x, cx))
            })
            .capture_any_mouse_up(move |event, _, cx| {
                if event.button == MouseButton::Left {
                    up.update(cx, |this, cx| this.land_tab(cx))
                }
            })
            .on_mouse_up_out(MouseButton::Left, move |_, _, cx| {
                out.update(cx, |this, cx| this.land_tab(cx))
            })
    }
}

#[cfg(test)]
mod tests {
    use gpui::px;

    use crate::workspace::sidebar::{moved, place};

    #[test]
    fn the_pointer_across_picks_a_tab_place() {
        // Three tabs of 100, 160 and 80 after a scroll left them at 40.
        let tabs = [
            (px(40.0), px(140.0)),
            (px(140.0), px(300.0)),
            (px(300.0), px(380.0)),
        ];
        assert_eq!(place(&tabs, px(0.0)), 0);
        assert_eq!(place(&tabs, px(89.0)), 0);
        assert_eq!(place(&tabs, px(91.0)), 1);
        assert_eq!(place(&tabs, px(219.0)), 1);
        assert_eq!(place(&tabs, px(221.0)), 2);
        assert_eq!(place(&tabs, px(345.0)), 3);
        // Past the last tab, on the strip's blank end or beyond it.
        assert_eq!(place(&tabs, px(900.0)), 3);
        assert_eq!(moved(0, place(&tabs, px(900.0))), Some(2));
        assert_eq!(moved(2, place(&tabs, px(345.0))), None);
    }
}
