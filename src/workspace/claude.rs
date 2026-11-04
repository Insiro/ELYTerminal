//! Claude tabs: a shell whose integration starts `claude` at its first prompt, by id.

use std::path::Path;

use ely_gpui_component::i18n::text;
use gpui::{App, ClipboardItem, Context, Window};

use super::{Workspace, dialogs::Asking, projects::notice};
use crate::{
    projects::{
        Home, TabId,
        claude::{self, Heard, Mode},
    },
    settings::SettingsStore,
    term::{Launch, shell},
};

/// Hands `launch`'s shell Claude's id and the prelude; refuses a shell whose integration cannot start it.
pub(super) fn ready(
    launch: &mut Launch,
    id: &str,
    mode: Mode,
    prelude: &str,
    cx: &App,
) -> anyhow::Result<()> {
    anyhow::ensure!(claude::valid(id), "claude: {id:?} is no session id");
    if !launch.reports {
        let program = match &launch.program {
            Some((program, _)) => program.clone(),
            None => shell::default_shell()?,
        };
        let name = Path::new(&program)
            .file_name()
            .map_or(program.clone(), |name| name.to_string_lossy().into_owned());
        let why = text(cx, "et.claude.unsupported", &[("shell", &name)]);
        anyhow::bail!("{why}");
    }
    let mut names = vec!["ELY_CLAUDE"];
    launch
        .env
        .push(("ELY_CLAUDE".into(), claude::env(id, mode)));
    if !prelude.is_empty() {
        names.push("ELY_CLAUDE_PRELUDE");
        launch
            .env
            .push(("ELY_CLAUDE_PRELUDE".into(), prelude.to_string()));
    }
    // WSL hands a Windows variable to the distribution only as WSLENV names it.
    if let Some((_, shared)) = launch.env.iter_mut().find(|(name, _)| name == "WSLENV") {
        for name in names {
            shared.push(':');
            shared.push_str(name);
        }
    }
    // cmd has no prompt hook: it runs Claude as its own command, and no mark tells its end.
    #[cfg(windows)]
    if launch.quote == crate::term::Quote::Cmd {
        let word = match mode {
            Mode::New => "--session-id",
            Mode::Resume => "--resume",
        };
        let command = format!("claude {word} {id}");
        let command = match prelude {
            "" => command,
            prelude => format!("{prelude} & {command}"),
        };
        if let Some((_, args)) = launch.program.as_mut() {
            *args = vec!["/k".into(), command];
        }
    }
    log::info!("claude: a tab starts {}", claude::env(id, mode));
    Ok(())
}

impl Workspace {
    /// A Claude tab of the set shell in `home`, else the selected one, where a new tab starts; a failure is told.
    pub fn new_claude(&mut self, home: Option<Home>, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(home) = home {
            self.select_home(home, window, cx);
        }
        let Some(start) = self.start_in(window, cx) else {
            return;
        };
        let shell = SettingsStore::get(cx).shell.clone();
        let place = (self.selected.clone(), None);
        let claude = Some((claude::new_id(), Mode::New));
        if let Err(error) = self.open((&shell, Vec::new()), start, place, claude, window, cx) {
            notice("et.claude.failed", error, window, cx);
        }
    }

    /// Closes tab `id`, asking first while Claude runs in it.
    pub fn ask_close(&mut self, id: TabId, window: &mut Window, cx: &mut Context<Self>) {
        let running = self
            .index(id)
            .is_some_and(|at| self.tabs[at].session().claude.is_some());
        if running {
            log::debug!("app: tab {} runs Claude, so closing asks", id.0);
            self.asking = Some(Asking::CloseClaude { id });
            cx.notify();
        } else {
            self.close(id, window, cx);
        }
    }

    /// Copies tab `id`'s Claude session id.
    pub fn copy_claude(&self, id: TabId, cx: &mut App) {
        let claude = self
            .index(id)
            .and_then(|at| self.tabs[at].session().claude.clone());
        match claude {
            Some(claude) => {
                log::info!("app: copied Claude session {claude}");
                cx.write_to_clipboard(ClipboardItem::new_string(claude));
            }
            None => log::error!("app: tab {} runs no Claude to copy", id.0),
        }
    }

    /// Tab `index`'s terminal heard `mark`; Claude's end drops its id, so the tab is a shell again.
    pub(super) fn claude_marked(&mut self, index: usize, mark: Heard, cx: &mut Context<Self>) {
        let tab = self.tabs[index].id;
        let session = self.tabs[index].session_mut();
        if session.claude.is_none() || !session.watch.hear(mark) {
            return;
        }
        if let Some(id) = session.claude.take() {
            log::info!("app: Claude {id} in tab {} ended, {mark:?}", tab.0);
        }
        self.dirty(cx);
        cx.notify();
    }
}
