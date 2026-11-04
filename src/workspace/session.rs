use std::path::PathBuf;

use gpui::{Context, Window};

use super::{Workspace, projects::notice};
use crate::{
    projects::{Home, TabId},
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
        let Some(start) = self.start_in(window, cx) else {
            return;
        };
        let shell = SettingsStore::get(cx).shell.clone();
        let place = (self.selected.clone(), None);
        if let Err(error) = self.open(&shell, Vec::new(), start, place, window, cx) {
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
            None => match self.start_in(window, cx) {
                Some(start) => start,
                None => return,
            },
        };
        log::info!("app: a profile opens {shell} with {} arguments", args.len());
        let place = (self.selected.clone(), None);
        if let Err(error) = self.open(&shell, args, start, place, window, cx) {
            log::error!("app: no profile session: {error:#}");
        }
    }

    /// Where a new session starts: a project's home in its folder, Others where Start in says; none, told, when that folder is gone.
    fn start_in(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<Start> {
        if let Home::Tree { path, .. } = &self.selected {
            if path.is_dir() {
                return Some(Start::In(path.clone()));
            }
            let error = anyhow::anyhow!("{} is not a folder", path.display());
            notice("et.projects.folder_missing", error, window, cx);
            return None;
        }
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
        let Some(tab) = self.index(id).map(|index| &self.tabs[index]) else {
            log::error!("app: there is no tab {} to duplicate", id.0);
            return;
        };
        let (session, home) = (tab.session(), tab.home.clone());
        let (shell, args, folder) = (
            session.shell.clone(),
            session.args.clone(),
            session.folder.clone(),
        );
        log::info!("app: tab {} duplicates into {}", id.0, folder.display());
        let place = (home, Some(id));
        if let Err(error) = self.open(&shell, args, Start::In(folder), place, window, cx) {
            log::error!("app: no duplicate tab: {error:#}");
        }
    }

    /// The shown tab's shell in `folder`, in the home that holds it, after the shown tab when it is that home's; the set shell while no tab shows.
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
        let home = self.home_for(&folder);
        log::info!("app: tab here {} in {home:?}", folder.display());
        let after = (home == self.selected).then(|| self.current_id()).flatten();
        self.open(&shell, args, Start::In(folder), (home, after), window, cx)
    }

    /// Shows a `shell` tab with `args` from `start` in `home`, after tab `after` or last; the home changes only once it started.
    fn open(
        &mut self,
        shell: &str,
        args: Vec<String>,
        start: Start,
        (home, after): (Home, Option<TabId>),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        let settings = SettingsStore::get(cx);
        let launch = launch(shell, &args, start, &cx.global::<ShellIntegration>().0)?;
        let (pty, events) = spawn(&launch, (80, 24), (8, 16), config(settings))?;
        let id = self.add(pty, events, launch, (args, home), window, cx);
        if let Some(at) = after.and_then(|after| self.index(after)) {
            let tab = self.tabs.pop().expect("the tab just added");
            self.tabs.insert(at + 1, tab);
        }
        self.show(id, window, cx);
        Ok(())
    }
}
