//! A project dragged in the left column: the pointer's height picks its place, a release anywhere takes it.

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use ely_gpui_component::theme::{ActiveTheme, Elevation, Radius};
use gpui::{
    AnyElement, App, AppContext as _, Bounds, Context, Div, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, ParentElement, Pixels, Render, SharedString, Stateful,
    StatefulInteractiveElement, Styled, Window, canvas, div, prelude::FluentBuilder,
};

use super::Workspace;

/// A project's row on its way, by id.
struct ProjectDrag(u64);

/// What follows the pointer: the project's name.
struct Ghost(SharedString);

impl Render for Ghost {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .px_2()
            .py_1()
            .bg(theme.colors.overlay)
            .text_color(theme.colors.fg)
            .border_1()
            .border_color(theme.colors.border)
            .rounded(theme.radius(Radius::Md))
            .shadow(theme.elevation(Elevation::Floating))
            .child(self.0.clone())
    }
}

/// The project held, and the place it would take: before block `target`, or last at the count.
#[derive(Clone, Copy)]
struct Held {
    id: u64,
    target: usize,
}

/// The held project, and each block's bounds from the last paint.
#[derive(Default)]
pub(in crate::workspace) struct Hold {
    held: Option<Held>,
    bounds: Rc<RefCell<HashMap<u64, Bounds<Pixels>>>>,
}

/// The place a pointer at `y` picks: before the first block whose middle lies below it, else last.
fn place(blocks: &[(Pixels, Pixels)], y: Pixels) -> usize {
    blocks
        .iter()
        .position(|(top, bottom)| *top + (*bottom - *top) / 2.0 > y)
        .unwrap_or(blocks.len())
}

/// Where the block at `from` goes for place `target`, or none when it stays.
fn moved(from: usize, target: usize) -> Option<usize> {
    let to = if target > from { target - 1 } else { target };
    (to != from).then_some(to)
}

impl Workspace {
    /// The place a drag would move a project to, while one would move.
    pub(super) fn drop_place(&self, cx: &App) -> Option<usize> {
        let held = self.hold.held.filter(|_| cx.has_active_drag())?;
        let from = self.projects.iter().position(|p| p.id == held.id)?;
        moved(from, held.target).map(|_| held.target)
    }

    fn lift(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(from) = self.projects.iter().position(|p| p.id == id) else {
            return;
        };
        log::info!("app: project {from} lifted");
        self.hold.held = Some(Held { id, target: from });
        cx.notify();
    }

    fn aim(&mut self, y: Pixels, cx: &mut Context<Self>) {
        let Some(held) = self.hold.held else { return };
        let bounds = self.hold.bounds.borrow();
        let blocks: Option<Vec<_>> = self
            .projects
            .iter()
            .map(|p| bounds.get(&p.id).map(|b| (b.top(), b.bottom())))
            .collect();
        let Some(blocks) = blocks else { return };
        let target = place(&blocks, y);
        drop(bounds);
        if target != held.target {
            log::debug!("app: project drag at {y:?} aims at place {target}");
            self.hold.held = Some(Held { target, ..held });
            cx.notify();
        }
    }

    /// A release anywhere moves the held project to its shown place.
    fn land(&mut self, cx: &mut Context<Self>) {
        let Some(held) = self.hold.held.take() else {
            return;
        };
        cx.notify();
        if !cx.has_active_drag() {
            log::debug!("app: a project drag ended before its release");
            return;
        }
        let Some(from) = self.projects.iter().position(|p| p.id == held.id) else {
            log::info!("app: the project dragged is gone");
            return;
        };
        match moved(from, held.target) {
            Some(to) => self.reorder_projects(from, to, cx),
            None => log::info!("app: project {from} dropped in its place"),
        }
    }

    /// Escape drops the drag under way, moving nothing.
    pub(in crate::workspace) fn escape_drag(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.hold.held.is_none() || event.keystroke.key != "escape" {
            return;
        }
        log::info!("app: project drag cancelled");
        self.hold.held = None;
        cx.stop_active_drag(window);
        cx.stop_propagation();
        cx.notify();
    }

    /// `head` lifts project `id` once the pointer moves past gpui's threshold.
    pub(super) fn draggable(
        &self,
        id: u64,
        label: SharedString,
        head: impl IntoElement,
        cx: &Context<Self>,
    ) -> AnyElement {
        let workspace = cx.entity();
        div()
            .id(("project-drag", id as usize))
            .child(head)
            .on_drag(ProjectDrag(id), move |drag, _, _, cx| {
                workspace.update(cx, |this, cx| this.lift(drag.0, cx));
                cx.new(|_| Ghost(label.clone()))
            })
            .into_any_element()
    }

    /// Block `ix` of `count`, its bounds kept, the place line drawn on it while a drag picks it.
    pub(super) fn slot(
        &self,
        ix: usize,
        id: u64,
        block: AnyElement,
        count: usize,
        cx: &Context<Self>,
    ) -> AnyElement {
        let place = self.drop_place(cx);
        let held = self.hold.held.is_some_and(|h| h.id == id) && cx.has_active_drag();
        let bounds = self.hold.bounds.clone();
        let theme = cx.theme();
        let line = || {
            div()
                .absolute()
                .left_0()
                .right_0()
                .h(theme.tab_indicator())
                .rounded_full()
                .bg(theme.colors.accent)
        };
        div()
            .relative()
            .child(div().when(held, |block| block.opacity(0.5)).child(block))
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
            .when(place == Some(ix), |slot| slot.child(line().top_0()))
            .when(place == Some(count) && ix + 1 == count, |slot| {
                slot.child(line().bottom_0())
            })
            .into_any_element()
    }

    /// The pointer's height aims the held project wherever it goes, and any release lands it.
    pub(super) fn track(&self, list: Stateful<Div>, cx: &Context<Self>) -> Stateful<Div> {
        let (aim, up, out) = (cx.entity(), cx.entity(), cx.entity());
        list.on_drag_move::<ProjectDrag>(move |event, _, cx| {
            aim.update(cx, |this, cx| this.aim(event.event.position.y, cx))
        })
        .capture_any_mouse_up(move |event, _, cx| {
            if event.button == MouseButton::Left {
                up.update(cx, |this, cx| this.land(cx))
            }
        })
        .on_mouse_up_out(MouseButton::Left, move |_, _, cx| {
            out.update(cx, |this, cx| this.land(cx))
        })
    }
}

#[cfg(test)]
mod tests {
    use gpui::px;

    use super::{moved, place};

    #[test]
    fn the_pointer_picks_the_place_by_middles() {
        // A short row, a tall block of a project and its worktrees, a short row.
        let blocks = [
            (px(0.0), px(30.0)),
            (px(30.0), px(130.0)),
            (px(130.0), px(160.0)),
        ];
        assert_eq!(place(&blocks, px(-50.0)), 0);
        assert_eq!(place(&blocks, px(14.0)), 0);
        assert_eq!(place(&blocks, px(16.0)), 1);
        assert_eq!(place(&blocks, px(79.0)), 1);
        assert_eq!(place(&blocks, px(81.0)), 2);
        assert_eq!(place(&blocks, px(150.0)), 3);
        assert_eq!(place(&blocks, px(900.0)), 3);
        assert_eq!(place(&[], px(10.0)), 0);
    }

    #[test]
    fn a_place_beside_the_block_moves_nothing() {
        assert_eq!(moved(1, 1), None);
        assert_eq!(moved(1, 2), None);
        assert_eq!(moved(1, 0), Some(0));
        assert_eq!(moved(0, 3), Some(2));
        assert_eq!(moved(2, 0), Some(0));
    }
}
