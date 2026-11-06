//! Each home's right column, kept as the window leaves the home and shown again on its return.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use gpui::{App, Context, Entity, Window};

use super::{Workspace, toggles::set_tab};
use crate::{
    docs::Document,
    projects::{Home, store::SavedPanel},
    settings::{PanelTab, SettingsStore},
};

/// A home's preview: open, or as `workspace.json` kept it.
pub(super) enum Shown {
    Doc(Entity<Document>),
    Saved(PathBuf),
}

/// A home's right column as it was left.
#[derive(Debug, PartialEq)]
pub(super) struct Panel<D> {
    pub tab: PanelTab,
    pub preview: Option<D>,
    pub share: f32,
}

/// Keeps `now` for `from` and hands back what `to` showed, if it was ever left.
pub(super) fn swap<D>(
    kept: &mut HashMap<Home, Panel<D>>,
    from: Home,
    now: Panel<D>,
    to: &Home,
) -> Option<Panel<D>> {
    kept.insert(from, now);
    kept.remove(to)
}

/// The folder a home's own tree shows: its worktree, or the home folder for Others.
pub(super) fn tree_root(home: &Home) -> Option<PathBuf> {
    match home {
        Home::Tree { path, .. } => Some(path.clone()),
        Home::Others => dirs::home_dir(),
    }
}

/// The order panels save in: projects' by path, Others last.
fn order(home: &Home) -> (bool, Option<(&Path, &Path)>) {
    match home {
        Home::Tree { project, path } => (false, Some((project.as_path(), path.as_path()))),
        Home::Others => (true, None),
    }
}

/// A path `workspace.json` can keep, which holds text.
fn text(path: &Path) -> bool {
    path.to_str().is_some()
}

impl Workspace {
    /// The window turns to `home`: the right column of the home left is kept, and `home`'s comes back.
    pub(super) fn enter(&mut self, home: &Home, window: &mut Window, cx: &mut Context<Self>) {
        if *home == self.selected {
            return;
        }
        let preview = self.preview.take();
        if let Some(doc) = &preview {
            doc.update(cx, |doc, cx| doc.set_visible(false, window, cx));
        }
        let left = Panel {
            tab: SettingsStore::get(cx).panel,
            preview: preview.map(Shown::Doc),
            share: self.preview_share,
        };
        let from = self.selected.clone();
        log::debug!("app: the right column of {from:?} kept");
        if let Some(back) = swap(&mut self.panels, from, left, home) {
            self.apply(back, window, cx);
        }
    }

    /// Shows a kept right column; Files follows the home's folder after.
    pub(super) fn apply(
        &mut self,
        panel: Panel<Shown>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        log::debug!("app: the right column shows {:?} again", panel.tab);
        set_tab(panel.tab, cx);
        self.preview_share = panel.share;
        match panel.preview {
            Some(Shown::Doc(doc)) => self.preview = Some(doc),
            Some(Shown::Saved(path)) if path.is_file() => self.preview(path, window, cx),
            Some(Shown::Saved(path)) => {
                log::info!("app: the kept preview {} is gone", path.display())
            }
            None => {}
        }
        self.sync_preview(window, cx);
    }

    /// Each home's right column as `workspace.json` keeps it.
    pub(super) fn saved_panels(&self, cx: &App) -> Vec<SavedPanel> {
        let path = |shown: &Shown| match shown {
            Shown::Doc(doc) => doc.read(cx).path().to_path_buf(),
            Shown::Saved(path) => path.clone(),
        };
        let current = (
            &self.selected,
            SettingsStore::get(cx).panel,
            self.preview
                .as_ref()
                .map(|doc| doc.read(cx).path().to_path_buf()),
        );
        let kept = (self.panels.iter())
            .map(|(home, panel)| (home, panel.tab, panel.preview.as_ref().map(path)));
        let projects = |home: &Home| match home {
            Home::Tree { project, .. } => self.projects.iter().any(|p| p.folder == *project),
            Home::Others => true,
        };
        let mut saved: Vec<SavedPanel> = std::iter::once(current)
            .chain(kept)
            .filter(|(home, ..)| projects(home))
            .map(|(home, tab, preview)| {
                let open = tree_root(home).map(|root| self.files.read(cx).open_in(&root));
                SavedPanel {
                    home: home.clone(),
                    tab: Some(tab),
                    preview: preview.filter(|path| text(path)),
                    open: open
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|p| text(p))
                        .collect(),
                }
            })
            .collect();
        saved.sort_by(|left, right| order(&left.home).cmp(&order(&right.home)));
        saved
    }

    /// Takes in the right columns `workspace.json` kept, before a home is selected.
    pub(super) fn seed_panels(&mut self, saved: Vec<SavedPanel>, cx: &mut Context<Self>) {
        let tab = SettingsStore::get(cx).panel;
        for panel in saved {
            if let Some(root) = tree_root(&panel.home) {
                let chosen = panel.preview.as_deref();
                let open = panel.open.clone();
                self.files
                    .update(cx, |files, cx| files.seed(root, open, chosen, cx));
            }
            let kept = Panel {
                tab: panel.tab.unwrap_or(tab),
                preview: panel.preview.map(Shown::Saved),
                share: super::preview::SHARE,
            };
            self.panels.insert(panel.home, kept);
        }
        log::info!("workspace: {} right columns restored", self.panels.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home(path: &str) -> Home {
        Home::Tree {
            project: "/p".into(),
            path: path.into(),
        }
    }

    fn panel(tab: PanelTab, preview: Option<&'static str>) -> Panel<&'static str> {
        Panel {
            tab,
            preview,
            share: 0.45,
        }
    }

    #[test]
    fn a_home_left_comes_back_as_it_was() {
        let mut kept = HashMap::new();
        let (a, b) = (home("/p"), home("/p-feat"));
        let a_files = panel(PanelTab::Files, Some("/p/README.md"));
        assert_eq!(swap(&mut kept, a.clone(), a_files, &b), None);
        let b_git = panel(PanelTab::Git, None);
        let back = swap(&mut kept, b.clone(), b_git, &a);
        assert_eq!(back, Some(panel(PanelTab::Files, Some("/p/README.md"))));
        let back = swap(&mut kept, a, panel(PanelTab::Usage, None), &b);
        assert_eq!(back, Some(panel(PanelTab::Git, None)));
    }

    #[test]
    fn a_home_never_left_gives_nothing_back() {
        let mut kept = HashMap::new();
        swap(
            &mut kept,
            Home::Others,
            panel(PanelTab::Git, None),
            &home("/p"),
        );
        assert_eq!(kept.len(), 1);
        assert!(kept.contains_key(&Home::Others));
    }

    #[test]
    fn others_saves_last_and_its_tree_is_the_home_folder() {
        let (a, b) = (home("/p"), home("/p-feat"));
        let mut homes = [Home::Others, b.clone(), a.clone()];
        homes.sort_by(|l, r| order(l).cmp(&order(r)));
        assert_eq!(homes, [a, b, Home::Others]);
        assert_eq!(tree_root(&home("/p-feat")), Some("/p-feat".into()));
        assert_eq!(tree_root(&Home::Others), dirs::home_dir());
    }
}
