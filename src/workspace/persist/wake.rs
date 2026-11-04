//! Each home's saved tabs started as it is selected.

use std::path::Path;

use gpui::{Context, Window};

use super::{Dormant, Workspace};
use crate::{
    projects::{Home, TabId, claude::Mode, store::SavedTab},
    term::shell::Start,
    workspace::{projects::notice, tabs::Tab},
};

impl Workspace {
    /// Starts `home`'s saved tabs in their order, unless its folder is gone; the tab it showed, to show. Failures are told at once.
    pub(in crate::workspace) fn wake(
        &mut self,
        home: &Home,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<TabId> {
        let asleep = self.dormant.remove(home)?;
        if let Home::Tree { path, .. } = home
            && !path.is_dir()
        {
            log::warn!("workspace: {} is missing; its tabs wait", path.display());
            self.dormant.insert(home.clone(), asleep);
            return None;
        }
        log::info!("workspace: {} tabs of {home:?} wake", asleep.tabs.len());
        let (mut woken, mut failed, mut kept) = (Vec::new(), Vec::new(), Vec::new());
        for (at, tab) in asleep.tabs.into_iter().enumerate() {
            let (name, shell, args, folder, claude) = match tab.clone() {
                SavedTab::Terminal {
                    name,
                    shell,
                    args,
                    folder,
                    claude,
                } => (name, shell, args, folder, claude),
                // A document opens though its file is gone, which its banner says.
                SavedTab::Doc { path } => {
                    woken.push((at, self.add_doc(path, home.clone(), window, cx)));
                    continue;
                }
            };
            let start = folder.clone().map_or(Start::Home, Start::In);
            let place = (home.clone(), None);
            // A WSL tab's transcripts lie inside its distribution, out of sight.
            let wsl = Path::new(&shell)
                .file_stem()
                .is_some_and(|stem| stem.eq_ignore_ascii_case("wsl"));
            let claude = claude.map(|id| {
                let mode = if wsl {
                    Mode::Resume
                } else {
                    self.modes.of(&id)
                };
                (id, mode)
            });
            let shown = folder
                .as_deref()
                .map_or_else(|| "~".into(), |at| at.display().to_string());
            match self.begin((&shell, args), start, place, claude, window, cx) {
                Ok(id) => {
                    let tab = self.index(id).map(|at| &mut self.tabs[at]);
                    if let Some(session) = tab.and_then(Tab::term_mut) {
                        session.name = name;
                        session
                            .terminal
                            .update(cx, |view, _| view.set_front(false, window));
                    }
                    woken.push((at, id));
                }
                Err(error) => {
                    log::error!("workspace: a tab in {shown} did not start: {error:#}");
                    failed.push(format!("{shown}: {error:#}"));
                    kept.push(tab);
                }
            }
        }
        // A tab that did not start waits until its home is selected again or forgotten.
        if !kept.is_empty() {
            let waiting = Dormant {
                shown: 0,
                tabs: kept,
                failed: true,
            };
            self.dormant.insert(home.clone(), waiting);
        }
        if !failed.is_empty() {
            notice(
                "et.store.wake_failed",
                anyhow::anyhow!(failed.join("\n")),
                window,
                cx,
            );
        }
        self.dirty(cx);
        let shown = woken.iter().find(|(at, _)| *at == asleep.shown);
        shown.or(woken.first()).map(|(_, id)| *id)
    }
}
