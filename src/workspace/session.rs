use std::path::PathBuf;

use gpui::{AppContext, Context, Window};

use super::{Body, Tab, Workspace, claude::ready, projects::notice};
use crate::{
    projects::{
        Home, TabId,
        claude::{self, Mode},
    },
    sessions::Session,
    settings::{self, SettingsStore, StartIn},
    term::{
        Events, Launch, Pty, TerminalView, config,
        shell::{ShellIntegration, Start, launch},
        spawn,
    },
};

impl Workspace {
    /// A tab of the set shell in the selected home, where Start in says.
    pub(super) fn new_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(error) = self.new_tab(window, cx) {
            log::error!("app: no new tab: {error:#}");
        }
    }

    /// `new_session`, its failure given back; a folder the user was told is missing opens nothing.
    pub(super) fn new_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        let Some(start) = self.start_in(window, cx) else {
            return Ok(());
        };
        let shell = SettingsStore::get(cx).shell.clone();
        let place = (self.selected.clone(), None);
        self.open((&shell, Vec::new()), start, place, None, window, cx)
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
        if let Err(error) = self.open((&shell, args), start, place, None, window, cx) {
            log::error!("app: no profile session: {error:#}");
        }
    }

    /// Where a new session starts: a project's home in its folder, Others where Start in says; none, told, when that folder is gone.
    pub(super) fn start_in(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<Start> {
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
                    notice("et.session.start_folder", error, window, cx);
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
        // A Claude tab's duplicate starts a Claude session of its own.
        let claude = session
            .claude
            .as_ref()
            .map(|_| (claude::new_id(), Mode::New));
        log::info!("app: tab {} duplicates into {}", id.0, folder.display());
        let place = (home, Some(id));
        let (program, asked) = ((shell.as_str(), args), claude.is_some());
        match self.open(program, Start::In(folder), place, claude, window, cx) {
            Err(error) if asked => notice("et.claude.failed", error, window, cx),
            Err(error) => log::error!("app: no duplicate tab: {error:#}"),
            Ok(()) => {}
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
        let program = (shell.as_str(), args);
        self.open(program, Start::In(folder), (home, after), None, window, cx)
    }

    /// Shows a `shell` tab with `args` from `start` in `home`, after tab `after` or last, running `claude` if given; the home changes only once it started.
    pub(super) fn open(
        &mut self,
        program: (&str, Vec<String>),
        start: Start,
        place: (Home, Option<TabId>),
        claude: Option<(String, Mode)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        let id = self.begin(program, start, place, claude, window, cx)?;
        self.show(id, window, cx);
        Ok(())
    }

    /// A `shell` tab with `args` from `start` in `home`, after tab `after` or last, running `claude` if given, not shown.
    pub(super) fn begin(
        &mut self,
        (shell, args): (&str, Vec<String>),
        start: Start,
        (home, after): (Home, Option<TabId>),
        claude: Option<(String, Mode)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<TabId> {
        // A saved home wakes first, so its tabs come before this one.
        if self.dormant.contains_key(&home) {
            let _ = self.wake(&home, window, cx);
        }
        let settings = SettingsStore::get(cx);
        let started = match &start {
            Start::Home => None,
            Start::In(folder) => Some(folder.clone()),
        };
        let mut launch = launch(shell, &args, start, &cx.global::<ShellIntegration>().0)?;
        let hearing = match &claude {
            Some((id, mode)) => Some(ready(&mut launch, id, *mode, &settings.claude_prelude, cx)?),
            None => None,
        };
        let (pty, events) = spawn(&launch, (80, 24), (8, 16), config(settings))?;
        let id = self.add(pty, events, launch, (args, home, started), window, cx);
        if let (Some((claude, _)), Some(hearing)) = (claude, hearing) {
            self.watch_claude((id, claude), hearing, cx);
        }
        if let Some(at) = after.and_then(|after| self.index(after)) {
            let tab = self.tabs.pop().expect("the tab just added");
            self.tabs.insert(at + 1, tab);
        }
        self.dirty(cx);
        Ok(id)
    }

    /// A tab of `pty` in `home`, last in the list.
    pub(super) fn add(
        &mut self,
        pty: Pty,
        events: Events,
        launch: Launch,
        (args, home, start): (Vec<String>, Home, Option<PathBuf>),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> TabId {
        let terminal = cx.new(|cx| TerminalView::new(pty, events, window, cx));
        let id = terminal.entity_id();
        let events = cx.subscribe_in(&terminal, window, move |this, _, event, window, cx| {
            this.session_event(id, event, window, cx)
        });
        let shell = launch
            .program
            .as_ref()
            .map_or_else(String::new, |(program, _)| program.clone());
        let pid = terminal.read(cx).pid();
        let id = TabId(self.next_tab);
        self.next_tab += 1;
        log::info!(
            "app: tab {} opens {pid} in {} of {:?}",
            id.0,
            launch.cwd.display(),
            home
        );
        let session = Session {
            terminal,
            title: String::new(),
            name: String::new(),
            folder: launch.cwd,
            start,
            claude: None,
            watch: Default::default(),
            keyed: false,
            shell,
            args,
            reports: launch.reports,
            bell: false,
            waiting: false,
            look: None,
            _events: events,
        };
        self.tabs.push(Tab {
            id,
            home,
            body: Body::Term(session),
        });
        id
    }
}
