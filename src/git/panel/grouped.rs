use std::path::{Path, PathBuf};

use ely_gpui_component::{
    buttons::{ButtonVariant, IconButton},
    git::{ChangeAction, ChangeSection, GitStatusBadge},
    i18n::text,
    lists::GitStatus,
    primitives::{Disclosure, FocusRing, Icon, IconName},
    theme::{ActiveTheme, ControlSize, IconSize, Radius, TextSize},
    typography::{Ellipsis, tabular},
};
use gpui::{
    AnyElement, Context, Div, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Role, SharedString, StatefulInteractiveElement, Styled, div,
    prelude::FluentBuilder, rems,
};

use super::{
    Change, Discarding, GitPanel, Repo, listed,
    tree::{Item, Row, rows},
    view::{LISTED, compared, shown},
};

/// What a row's button acts on: a file by its shown path, or every change under a folder.
#[derive(Clone)]
enum Target {
    File(SharedString),
    Folder(PathBuf),
}

fn id_of(name: String) -> ElementId {
    (ElementId::from("git-tree"), name).into()
}

/// The actions a section's rows offer, Ely's list's own.
fn actions(section: ChangeSection) -> Vec<(ChangeAction, IconName, &'static str)> {
    let changed = matches!(section, ChangeSection::Unstaged | ChangeSection::Untracked);
    let mut out = Vec::new();
    if changed {
        out.push((
            ChangeAction::Discard,
            IconName::RotateCcw,
            "git.changes.discard",
        ));
    }
    if section == ChangeSection::Staged {
        out.push((
            ChangeAction::Unstage,
            IconName::Minus,
            "git.changes.unstage",
        ));
    }
    if changed {
        out.push((ChangeAction::Stage, IconName::Plus, "git.changes.stage"));
    }
    out
}

impl GitPanel {
    /// Opens or shuts the folder `key` in `section`.
    fn fold(&mut self, section: ChangeSection, key: SharedString, cx: &mut Context<Self>) {
        let shut = (compared(section), key);
        if !self.shut.remove(&shut) {
            log::debug!("git panel: {} shut in {section:?}", shut.1);
            self.shut.insert(shut);
        }
        cx.notify();
    }

    /// What a folder's row asked for every change under `folder` in `section`.
    fn folder_action(
        &mut self,
        folder: PathBuf,
        section: ChangeSection,
        action: ChangeAction,
        cx: &mut Context<Self>,
    ) {
        let shown = shown(&folder);
        let Some(repo) = self.current() else {
            return self.waits(&format!("{action:?} {shown}/"));
        };
        let changes: Vec<Change> = listed(repo, section)
            .iter()
            .filter(|change| change.path.starts_with(&folder))
            .cloned()
            .collect();
        if changes.is_empty() {
            log::warn!("git panel: {shown}/ holds nothing in {section:?}");
            return;
        }
        let top = repo.top.clone();
        let files = changes.len();
        log::debug!("git panel: {action:?} {shown}/ in {section:?}, {files} files");
        match action {
            ChangeAction::Stage => {
                let work = move |top: &Path| crate::git::stage(top, &changes);
                self.act("et.git.stage_failed", top, work, |_, _| {}, cx)
            }
            ChangeAction::Unstage => {
                let work = move |top: &Path| crate::git::unstage(top, &changes);
                self.act("et.git.unstage_failed", top, work, |_, _| {}, cx)
            }
            ChangeAction::Discard => {
                let cost = match section {
                    ChangeSection::Untracked => "et.git.loss_folder",
                    _ => "et.git.loss_under",
                };
                let name = format!("{shown}/").into();
                self.discarding = Some(Discarding {
                    changes,
                    section,
                    name,
                    cost,
                });
                cx.notify();
            }
            ChangeAction::Open => log::warn!("git panel: a folder opens no diff"),
        }
    }

    /// One action's button on a row.
    fn button(
        &self,
        id: String,
        (action, icon, words): (ChangeAction, IconName, &'static str),
        target: Target,
        section: ChangeSection,
        cx: &Context<Self>,
    ) -> IconButton {
        let panel = cx.entity();
        IconButton::new(id_of(id), icon)
            .variant(ButtonVariant::Ghost)
            .size(ControlSize::Sm)
            .tooltip(text(cx, words, &[]))
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                panel.update(cx, |panel, cx| match &target {
                    Target::File(row) => panel.row_action(row, section, action, cx),
                    Target::Folder(folder) => {
                        panel.folder_action(folder.clone(), section, action, cx)
                    }
                })
            })
    }

    /// A row of the tree, in the look of Ely's changes list.
    fn tree_row(
        &self,
        row: Row,
        changes: &[Change],
        section: ChangeSection,
        cx: &Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let colors = theme.colors.clone();
        let base = |id: String| {
            div()
                .id(id_of(id))
                .role(Role::ListItem)
                .tab_index(0)
                .border_1()
                .border_color(gpui::transparent_black())
                .focus_ring(cx)
                .flex()
                .items_center()
                .gap_2()
                .pl(rems(0.5 + 0.75 * row.depth as f32))
                .pr_2()
                .py_1()
                .rounded(theme.radius(Radius::Sm))
                .cursor_pointer()
                .hover(|row| row.bg(colors.hover))
        };
        let buttons = |target: Target, cx: &Context<Self>| {
            let at = |action: ChangeAction| match &target {
                Target::File(path) => format!("{action:?}-{section:?}-file-{path}"),
                Target::Folder(path) => format!("{action:?}-{section:?}-dir-{}", shown(path)),
            };
            let open = (ChangeAction::Open, IconName::FileText, "git.changes.open");
            let open = matches!(target, Target::File(_)).then_some(open);
            let made = open
                .into_iter()
                .chain(actions(section))
                .map(|act| self.button(at(act.0), act, target.clone(), section, cx));
            div().flex_none().flex().children(made.collect::<Vec<_>>())
        };
        match row.item {
            Item::Folder {
                path,
                key,
                name,
                open,
                count,
            } => {
                let fold = cx.entity();
                let shut = key.clone();
                base(format!("dir-{section:?}-{key}"))
                    .aria_label(name.clone())
                    .aria_expanded(open)
                    .on_click(move |_, _, cx| {
                        fold.update(cx, |panel, cx| panel.fold(section, shut.clone(), cx))
                    })
                    .child(Disclosure::new(
                        id_of(format!("fold-{section:?}-{key}")),
                        open,
                    ))
                    .child(
                        Icon::new(if open {
                            IconName::FolderOpen
                        } else {
                            IconName::Folder
                        })
                        .size(IconSize::Sm)
                        .color(colors.fg_muted),
                    )
                    .child(div().flex_1().min_w_0().child(Ellipsis::new(name)))
                    .child(
                        tabular(div().flex_none().text_color(colors.fg_subtle))
                            .child(count.to_string()),
                    )
                    .child(buttons(Target::Folder(path), cx))
                    .into_any_element()
            }
            Item::File { at, name } => {
                let change = &changes[at];
                let path = shown(&change.path);
                let picked = (self.diff.as_ref()).is_some_and(|diff| {
                    diff.section == section && shown(&diff.change.path) == path
                });
                let (open, row) = (cx.entity(), path.clone());
                base(format!("file-{section:?}-{path}"))
                    .aria_label(path.clone())
                    .aria_selected(picked)
                    .when(picked, |row| row.bg(colors.selection))
                    .on_click(move |_, _, cx| {
                        open.update(cx, |panel, cx| {
                            panel.row_action(&row, section, ChangeAction::Open, cx)
                        })
                    })
                    .child(GitStatusBadge::new(
                        id_of(format!("status-{section:?}-{path}")),
                        change.status,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_color(colors.fg)
                            .when(change.status == GitStatus::Deleted, |name| {
                                name.line_through()
                            })
                            .child(Ellipsis::new(name)),
                    )
                    .child(buttons(Target::File(path), cx))
                    .into_any_element()
            }
        }
    }

    /// A section's heading, its count and its all-at-once button, then its tree.
    fn tree_section(
        &self,
        section: ChangeSection,
        changes: &[Change],
        cx: &Context<Self>,
    ) -> Option<Div> {
        let changes = &changes[..changes.len().min(LISTED)];
        if changes.is_empty() {
            return None;
        }
        let theme = cx.theme();
        let all = match section {
            ChangeSection::Staged => Some((IconName::Minus, "git.changes.unstage_all", false)),
            ChangeSection::Unstaged => Some((IconName::Plus, "git.changes.stage_all", true)),
            _ => None,
        };
        let button = all.map(|(icon, words, stage)| {
            let panel = cx.entity();
            IconButton::new(id_of(format!("all-{section:?}")), icon)
                .variant(ButtonVariant::Ghost)
                .size(ControlSize::Sm)
                .tooltip(text(cx, words, &[]))
                .on_click(move |_, _, cx| panel.update(cx, |panel, cx| panel.all(stage, cx)))
        });
        let heading = div()
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .text_size(theme.text_size(TextSize::Xs))
            .text_color(theme.colors.fg_muted)
            .child(div().flex_1().font_weight(FontWeight::MEDIUM).child(text(
                cx,
                compared(section),
                &[],
            )))
            .child(tabular(div()).child(changes.len().to_string()))
            .children(button);
        let shut = |key: &SharedString| self.shut.contains(&(compared(section), key.clone()));
        let rows = rows(changes, shut).into_iter();
        let rows: Vec<AnyElement> = rows
            .map(|row| self.tree_row(row, changes, section, cx))
            .collect();
        Some(div().flex().flex_col().child(heading).children(rows))
    }

    /// The changes grouped by folder in each section.
    pub(super) fn grouped(&self, repo: &Repo, cx: &Context<Self>) -> Div {
        let theme = cx.theme();
        let sections = [
            (ChangeSection::Conflicted, &repo.conflicted),
            (ChangeSection::Staged, &repo.staged),
            (ChangeSection::Unstaged, &repo.unstaged),
            (ChangeSection::Untracked, &repo.untracked),
        ];
        let drawn = sections
            .into_iter()
            .filter_map(|(section, changes)| self.tree_section(section, changes, cx));
        div()
            .flex()
            .flex_col()
            .gap_2()
            .text_size(theme.text_size(TextSize::Sm))
            .children(drawn.collect::<Vec<_>>())
    }
}
