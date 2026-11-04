//! Claude tabs: a shell whose integration starts `claude` at its first prompt, by id.

use std::{path::Path, time::Duration};

use ely_gpui_component::i18n::text;
use gpui::{App, ClipboardItem, Context, Window};

use super::{Workspace, dialogs::Asking, projects::notice};
use crate::{
    projects::{
        Home, TabId,
        claude::{self, End, Heard, Mode, Watch},
    },
    settings::SettingsStore,
    term::{Launch, shell},
};

/// How long a Claude tab waits for Claude's command to start.
const START: Duration = Duration::from_secs(20);

/// Hands `launch`'s shell Claude's id and the prelude; refuses a shell whose integration cannot start it. False when no mark tells Claude's end.
pub(super) fn ready(
    launch: &mut Launch,
    id: &str,
    mode: Mode,
    prelude: &str,
    cx: &App,
) -> anyhow::Result<bool> {
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
    log::info!("claude: a tab starts {}", claude::env(id, mode));
    // cmd has no prompt hook: it runs Claude as its own command, and no mark tells its end.
    #[cfg(windows)]
    if launch.quote == crate::term::Quote::Cmd {
        let command = claude::cmd_line(id, mode, prelude)?;
        if let Some((_, args)) = launch.program.as_mut() {
            *args = vec!["/k".into(), command];
        }
        return Ok(false);
    }
    Ok(true)
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

    /// Tab `index`'s terminal heard `mark`; Claude's end, or word that it does not start, drops its id, so the tab is a shell again.
    pub(super) fn claude_marked(&mut self, index: usize, mark: Heard, cx: &mut Context<Self>) {
        let tab = self.tabs[index].id;
        let session = self.tabs[index].session_mut();
        if session.claude.is_none() {
            return;
        }
        match session.watch.hear(mark) {
            Some(End::Exited) => log::info!("app: Claude in tab {} ended, {mark:?}", tab.0),
            Some(End::Skipped) => log::warn!("app: Claude in tab {} did not start", tab.0),
            None => return,
        }
        self.drop_claude(index, cx);
    }

    /// Tab `index` runs Claude no more.
    fn drop_claude(&mut self, index: usize, cx: &mut Context<Self>) {
        let tab = self.tabs[index].id;
        let session = self.tabs[index].session_mut();
        if let Some(id) = session.claude.take() {
            log::info!("app: tab {} forgets Claude {id}", tab.0);
        }
        session.watch = Watch::Off;
        let terminal = session.terminal.clone();
        terminal.update(cx, |view, _| view.watch_marks(false));
        self.dirty(cx);
        cx.notify();
    }

    /// Tab `id` runs Claude `claude`, its end heard while `watched`; a command that has not started in 20 s is told and forgotten.
    pub(super) fn watch_claude(
        &mut self,
        (id, claude): (TabId, String),
        watched: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.index(id) else {
            return;
        };
        let session = self.tabs[index].session_mut();
        session.claude = Some(claude.clone());
        session.watch = if watched {
            Watch::Starting
        } else {
            Watch::Blind
        };
        let terminal = session.terminal.clone();
        terminal.update(cx, |view, _| view.watch_marks(watched));
        if !watched {
            return;
        }
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(START).await;
            let _ = this.update_in(cx, |this, window, cx| {
                let Some(index) = this.index(id) else {
                    return;
                };
                let session = this.tabs[index].session();
                if session.claude.as_ref() != Some(&claude) || !session.watch.waiting() {
                    return;
                }
                log::warn!(
                    "app: Claude {claude} in tab {} did not start in {START:?}",
                    id.0
                );
                this.drop_claude(index, cx);
                let why = text(cx, "et.claude.not_started_body", &[]);
                notice(
                    "et.claude.not_started",
                    anyhow::anyhow!("{why}"),
                    window,
                    cx,
                );
            });
        })
        .detach();
    }
}
