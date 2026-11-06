//! Saved tabs as the left column lists them, what a right press on one offers, and their dialogs.

use ely_gpui_component::{
    i18n::text,
    lists::ListItem,
    menus::{ContextMenu, Menu, MenuItem},
    overlays::{ConfirmDialog, Dialog},
    primitives::{Icon, IconName},
    theme::{ActiveTheme, Radius},
};
use gpui::{
    AnyElement, App, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window, div,
};

use super::super::{Asking, Workspace};
use crate::{
    projects::{Home, store::Parked},
    term::MONO,
    usage::time::{self, Age},
};

/// How long ago `saved` was, in words.
fn ago(saved: u64, cx: &App) -> String {
    let n = |n: i64| n.to_string();
    match time::age(time::now(), saved as i64) {
        Age::Now => text(cx, "et.parked.age_now", &[]),
        Age::Minutes(m) => text(cx, "et.parked.age_minutes", &[("n", &n(m))]),
        Age::Hours(h) => text(cx, "et.parked.age_hours", &[("n", &n(h))]),
        Age::Days(d) => text(cx, "et.parked.age_days", &[("n", &n(d))]),
    }
    .to_string()
}

impl Workspace {
    /// The rows of `home`'s saved tabs, under its own row.
    pub(in crate::workspace) fn parked_rows(
        &self,
        home: &Home,
        cx: &Context<Self>,
    ) -> Vec<AnyElement> {
        let workspace = cx.entity();
        self.parked
            .iter()
            .enumerate()
            .filter(|(_, parked)| parked.home == *home)
            .map(|(at, parked)| {
                let icon = match parked.claude {
                    Some(_) => IconName::Bot,
                    None => IconName::Terminal,
                };
                let (resume, kept) = (workspace.clone(), parked.clone());
                let row = ListItem::new(("parked", at), parked.name.clone())
                    .leading(Icon::new(icon).color(cx.theme().colors.fg_muted))
                    .description(ago(parked.saved, cx))
                    .on_click(move |_, window, cx| {
                        resume.update(cx, |this, cx| this.unpark(&kept, window, cx))
                    });
                let menu = menu(&workspace, parked, cx);
                ContextMenu::new(("parked-menu", at), menu)
                    .pl_4()
                    .child(row)
                    .into_any_element()
            })
            .collect()
    }

    /// The dialogs saved tabs ask: a busy tab's save, a delete, and a last screen.
    pub(in crate::workspace) fn parked_dialog(
        &self,
        asking: &Asking,
        closed: impl Fn(&mut Window, &mut App) + 'static,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let submit = cx.entity();
        Some(match asking {
            Asking::Park { id } => {
                let id = *id;
                let title = text(cx, "et.parked.busy_title", &[]);
                let message = text(cx, "et.parked.busy_body", &[]);
                ConfirmDialog::new("park-busy", title, message, closed)
                    .confirm(text(cx, "et.parked.save", &[]))
                    .destructive()
                    .on_confirm(move |window, cx| {
                        submit.update(cx, |this, cx| this.park(id, window, cx))
                    })
                    .into_any_element()
            }
            Asking::DeleteParked(parked) => {
                let kept = parked.clone();
                let title = text(cx, "et.parked.delete_title", &[("name", &parked.name)]);
                let message = text(cx, "et.parked.delete_body", &[]);
                ConfirmDialog::new("delete-parked", title, message, closed)
                    .confirm(text(cx, "et.parked.delete_confirm", &[]))
                    .destructive()
                    .on_confirm(move |_, cx| {
                        submit.update(cx, |this, cx| this.forget_parked(&kept, cx))
                    })
                    .into_any_element()
            }
            Asking::ParkedScreen(parked) => {
                let screen = if parked.screen.is_empty() {
                    text(cx, "et.parked.no_screen", &[]).to_string()
                } else {
                    parked.screen.clone()
                };
                let body = div()
                    .id("parked-screen")
                    .size_full()
                    .overflow_scroll()
                    .p_3()
                    .rounded(cx.theme().radius(Radius::Md))
                    .bg(cx.theme().colors.sunken)
                    .font_family(MONO)
                    .text_sm()
                    .whitespace_nowrap()
                    .child(screen);
                Dialog::new("parked-screen-dialog", parked.name.clone(), closed)
                    .detail(ago(parked.saved, cx))
                    .fullscreen()
                    .child(body)
                    .into_any_element()
            }
            _ => return None,
        })
    }
}

/// What a right press on saved tab `parked` offers.
fn menu(workspace: &gpui::Entity<Workspace>, parked: &Parked, cx: &App) -> Menu {
    let (resume, shown, deleted) = (workspace.clone(), workspace.clone(), workspace.clone());
    let (one, two, three) = (parked.clone(), parked.clone(), parked.clone());
    Menu::new()
        .item(
            MenuItem::new(text(cx, "et.parked.resume", &[]))
                .icon(IconName::Play)
                .on_click(move |window, cx| {
                    resume.update(cx, |this, cx| this.unpark(&one, window, cx))
                }),
        )
        .item(
            MenuItem::new(text(cx, "et.parked.show_screen", &[]))
                .icon(IconName::FileText)
                .on_click(move |_, cx| {
                    shown.update(cx, |this, cx| {
                        this.asking = Some(Asking::ParkedScreen(two.clone()));
                        cx.notify();
                    })
                }),
        )
        .separator()
        .item(
            MenuItem::new(text(cx, "et.parked.delete", &[]))
                .icon(IconName::Trash2)
                .on_click(move |_, cx| {
                    deleted.update(cx, |this, cx| {
                        this.asking = Some(Asking::DeleteParked(three.clone()));
                        cx.notify();
                    })
                }),
        )
}
