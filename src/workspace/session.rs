use std::path::PathBuf;

use gpui::{Context, Window};

use super::Workspace;
use crate::{
    projects::TabId,
    settings::{self, SettingsStore, StartIn},
    term::{
        config,
        shell::{ShellIntegration, Start, launch},
        spawn,
    },
};

impl Workspace {
    /// A tab of the set shell in the selected home, where Start in says.
    pub(super) fn new_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(start) = self.start_in(cx) else {
            return;
        };
        let shell = SettingsStore::get(cx).shell.clone();
        if let Err(error) = self.open(&shell, Vec::new(), start, None, window, cx) {
            log::error!("app: no new tab: {error:#}");
        }
    }

    /// A profile's session: `shell` with `args`, in `folder` or where Start in says.
    pub fn profile(
        &mut self,
        (shell, args): (String, Vec<String>),
        folder: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let start = match folder {
            Some(folder) => Start::In(folder),
            None => match self.start_in(cx) {
                Some(start) => start,
                None => return,
            },
        };
        log::info!("app: a profile opens {shell} with {} arguments", args.len());
        if let Err(error) = self.open(&shell, args, start, None, window, cx) {
            log::error!("app: no profile session: {error:#}");
        }
    }

    /// Where Start in puts a new session; none when its folder is gone.
    fn start_in(&self, cx: &Context<Self>) -> Option<Start> {
        let settings = SettingsStore::get(cx);
        match settings.start_in {
            StartIn::Home => Some(Start::Home),
            StartIn::Session => Some(
                self.current()
                    .map_or(Start::Home, |tab| Start::In(tab.session().folder.clone())),
            ),
            StartIn::Folder => match settings::start_folder(&settings.start_folder) {
                Ok(folder) => Some(Start::In(folder)),
                Err(error) => {
                    log::error!("app: no new session: {error:#}");
                    None
                }
            },
        }
    }

    /// Tab `id` again: the shell it runs, in the folder it last reported, right after it.
    pub fn duplicate(&mut self, id: TabId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.index(id).map(|index| self.tabs[index].session()) else {
            log::error!("app: there is no tab {} to duplicate", id.0);
            return;
        };
        let (shell, args, folder) = (
            session.shell.clone(),
            session.args.clone(),
            session.folder.clone(),
        );
        log::info!("app: tab {} duplicates into {}", id.0, folder.display());
        if let Err(error) = self.open(&shell, args, Start::In(folder), Some(id), window, cx) {
            log::error!("app: no duplicate tab: {error:#}");
        }
    }

    /// The shown tab's shell in `folder`, right after it; the set shell while no tab shows.
    pub fn here(
        &mut self,
        folder: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        let (shell, args) = match self.current() {
            Some(tab) => (tab.session().shell.clone(), tab.session().args.clone()),
            None => (SettingsStore::get(cx).shell.clone(), Vec::new()),
        };
        log::info!("app: tab here {}", folder.display());
        let after = self.current_id();
        self.open(&shell, args, Start::In(folder), after, window, cx)
    }

    /// Shows a `shell` tab with `args` from `start` in the selected home, after tab `after` or last.
    fn open(
        &mut self,
        shell: &str,
        args: Vec<String>,
        start: Start,
        after: Option<TabId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        let settings = SettingsStore::get(cx);
        let launch = launch(shell, &args, start, &cx.global::<ShellIntegration>().0)?;
        let (pty, events) = spawn(&launch, (80, 24), (8, 16), config(settings))?;
        let id = self.add(pty, events, launch, args, window, cx);
        if let Some(at) = after.and_then(|after| self.index(after)) {
            let tab = self.tabs.pop().expect("the tab just added");
            self.tabs.insert(at + 1, tab);
        }
        self.show(id, window, cx);
        Ok(())
    }
}
