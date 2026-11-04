use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::Instant,
};

use ely_gpui_component::{
    buttons::{ButtonVariant, IconButton},
    i18n::text,
    primitives::{FocusScope, IconName},
    shell::{ColumnShell, ResizeBorder, ShellColumn},
    theme::ControlSize,
    typography::Caption,
};
use gpui::{
    Action, App, AppContext, Context, Entity, FocusHandle, InteractiveElement, IntoElement,
    KeyBinding, ParentElement, Render, StyleRefinement, Styled, Subscription, Task, Window,
    actions, div, prelude::FluentBuilder,
};

use crate::{
    blur::Blur,
    docs::Document,
    files::{Files, Here},
    git::panel::{Acted, GitPanel},
    menus,
    projects::{Home, TabId, claude::Modes},
    sessions::orb,
    settings::{PanelTab, SettingsStore, window::OpenSettings},
};

mod claude;
mod dialogs;
mod doctabs;
mod events;
mod folder;
mod panel;
mod persist;
mod preview;
mod projects;
mod refresh;
mod session;
mod sidebar;
mod strip;
mod tabs;
mod toggles;
mod widths;

use dialogs::Asking;
pub use persist::Boot;
use persist::{Dormant, Persist};
use projects::Project;
use tabs::{Body, Tab};

use toggles::column_toggle;
use widths::{Dragged, Side};

actions!(
    elyterminal,
    [
        ToggleSessions,
        TogglePanel,
        NewSession,
        NewClaude,
        CloseSession,
        NextSession,
        PreviousSession,
        LastSession
    ]
);

/// Shows the selected home's tab at this place, from 0; past the last one, the key goes on to the program.
#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = elyterminal, no_json)]
pub struct SelectSession(pub usize);

/// The key context of the main window's content.
pub const CONTEXT: &str = "Workspace";

/// The window's title while the program names none.
pub const TITLE: &str = "ELYTerminal";

/// Ghostty's keys: Cmd on macOS; off it, Ctrl-Shift for keys a terminal takes, Alt for digits.
pub fn bind_keys(cx: &mut App) {
    let (sessions, panel, new, close, digit) = if cfg!(target_os = "macos") {
        ("cmd-b", "cmd-alt-b", "cmd-t", "cmd-w", "cmd")
    } else {
        (
            "ctrl-shift-b",
            "ctrl-shift-alt-b",
            "ctrl-shift-t",
            "ctrl-shift-w",
            "alt",
        )
    };
    let context = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new(sessions, ToggleSessions, context),
        KeyBinding::new(panel, TogglePanel, context),
        KeyBinding::new(new, NewSession, context),
        KeyBinding::new(close, CloseSession, context),
        KeyBinding::new("ctrl-tab", NextSession, context),
        KeyBinding::new("ctrl-shift-tab", PreviousSession, context),
        KeyBinding::new(&format!("{digit}-9"), LastSession, context),
    ]);
    cx.bind_keys(
        (1..=8).map(|n| KeyBinding::new(&format!("{digit}-{n}"), SelectSession(n - 1), context)),
    );
}

/// The main window's content: homes, the tabs over the terminal, and Git or Files, in Ely's columns.
pub struct Workspace {
    focus: FocusHandle,
    tabs: Vec<Tab>,
    /// The home whose tabs the strip shows.
    selected: Home,
    /// The tab each home shows.
    shown: HashMap<Home, TabId>,
    next_tab: u64,
    /// The projects the left column lists, in order.
    projects: Vec<Project>,
    next_project: u64,
    /// When an activation last listed every project's worktrees.
    refreshed: Option<Instant>,
    /// The saved tabs of each home not woken yet.
    dormant: HashMap<Home, Dormant>,
    /// How each restored Claude tab starts.
    modes: Modes,
    /// Saves `workspace.json`, once restored.
    persist: Option<Persist>,
    asking: Option<Asking>,
    files: Entity<Files>,
    /// The file Files previews under its tree, and the tree's share.
    preview: Option<Entity<Document>>,
    preview_share: f32,
    git: Entity<GitPanel>,
    /// What the window shows of the desktop, and the blur behind it.
    background: Blur,
    /// Column widths a drag shows before the settings hold them.
    dragged: Dragged,
    /// Draws the window at each step of the status marks while one shown moves.
    ticking: Option<Task<()>>,
    /// The shells this system holds, found as the window opened.
    found: Vec<crate::profiles::Found>,
    choosing_folder: bool,
    folder_actions: bool,
    _settings: Subscription,
    _acted: Subscription,
    _here: Subscription,
    _activation: Subscription,
}

impl Workspace {
    /// The window's content with no tab yet, its panels in `folder` until `restore` runs.
    pub fn new(folder: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let git = cx.new(|cx| GitPanel::new(folder.clone(), window, cx));
        // A change the Git panel made shows in Files' marks too.
        let acted = cx.subscribe(&git, |this, _, _: &Acted, cx| {
            if let Home::Tree { project, .. } = this.selected.clone() {
                this.refresh_folder(&project, cx);
            }
            let Some(folder) = this
                .current()
                .and_then(|tab| Some(tab.term()?.folder.clone()))
            else {
                return;
            };
            this.files.update(cx, |files, cx| files.show(&folder, cx));
        });
        let files = cx.new(|cx| {
            let mut files = Files::new(folder);
            files.poll(cx);
            files
        });
        let here = cx.subscribe_in(&files, window, |this, _, Here(folder), window, cx| {
            if let Err(error) = this.here(folder.clone(), window, cx) {
                folder::failed(error, window, cx);
            }
        });
        Self::hook_files(&files, window, cx);
        Self {
            focus: cx.focus_handle(),
            tabs: Vec::new(),
            selected: Home::Others,
            shown: HashMap::new(),
            next_tab: 0,
            projects: Vec::new(),
            next_project: 0,
            refreshed: None,
            dormant: HashMap::new(),
            modes: Modes::default(),
            persist: None,
            asking: None,
            files,
            preview: None,
            preview_share: preview::SHARE,
            git,
            background: Blur::new(window, SettingsStore::get(cx)),
            dragged: Dragged::default(),
            ticking: None,
            found: crate::profiles::detected(),
            choosing_folder: false,
            folder_actions: false,
            _settings: cx.observe_global_in::<SettingsStore>(window, |this, window, cx| {
                this.background.follow(window, SettingsStore::get(cx));
                this.sync_preview(window, cx);
                let settings = SettingsStore::get(cx);
                if !settings.files_menu || !settings.show_panel || settings.panel != PanelTab::Files
                {
                    this.folder_actions = false;
                }
                cx.notify();
            }),
            _acted: acted,
            _here: here,
            _activation: cx.observe_window_activation(window, |this, window, cx| {
                if window.is_window_active() {
                    this.refresh_all(cx);
                }
            }),
        }
    }

    /// Notes each session's mark `now`, and draws the window at each step while the drawn one moves.
    fn tick(&mut self, now: Instant, cx: &mut Context<Self>) {
        for session in self.tabs.iter_mut().filter_map(Tab::term_mut) {
            session.note(now, cx);
        }
        let moving = SettingsStore::get(cx).show_sessions && self.marks_move(now);
        if moving == self.ticking.is_some() {
            return;
        }
        log::debug!("app: status marks {}", if moving { "move" } else { "rest" });
        self.ticking = moving.then(|| {
            cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(orb::wait(Instant::now()))
                        .await;
                    if this.update(cx, |_, cx| cx.notify()).is_err() {
                        break;
                    }
                }
            })
        });
    }

    /// Files and Git show `folder`.
    fn follow(&mut self, folder: &Path, cx: &mut Context<Self>) {
        self.files.update(cx, |files, cx| files.show(folder, cx));
        self.git.update(cx, |git, cx| git.show(folder, cx));
        self.keep_preview(folder, cx);
    }

    /// What the shown tab is called, else the selected home.
    fn title(&self, cx: &App) -> String {
        match self.current() {
            Some(tab) => tab.title(cx),
            None => self
                .home_label(&self.selected)
                .unwrap_or_else(|| TITLE.into()),
        }
    }

    fn retitle(&self, window: &mut Window, cx: &App) {
        let title = self.title(cx);
        if window.window_title() != title {
            log::debug!("app: the title is {title:?}");
            window.set_window_title(&title);
        }
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.background.fit(window);
        let now = Instant::now();
        self.tick(now, cx);
        // A page lies over every dialog, so it waits while one asks.
        if let Some(doc) = self.current().and_then(Tab::doc) {
            let allowed = self.asking.is_none();
            doc.update(cx, |doc, cx| doc.allow_web(allowed, cx));
        }
        let settings = SettingsStore::get(cx);
        let workspace = cx.entity();
        // It leads the first column or ends the last.
        let left = column_toggle(
            &workspace,
            ("toggle-sessions", IconName::PanelLeft),
            (
                settings.show_sessions,
                ["et.tip.hide_sessions", "et.tip.show_sessions"],
            ),
            |settings| settings.show_sessions = !settings.show_sessions,
            cx,
        );
        let right = column_toggle(
            &workspace,
            ("toggle-panel", IconName::PanelRight),
            (
                settings.show_panel,
                ["et.tip.hide_panel", "et.tip.show_panel"],
            ),
            |settings| settings.show_panel = !settings.show_panel,
            cx,
        );
        // No menu bar off macOS: a gear opens Settings.
        let gear = (!cfg!(target_os = "macos")).then(|| {
            IconButton::new("open-settings", IconName::Settings)
                .variant(ButtonVariant::Ghost)
                .size(ControlSize::Sm)
                .tooltip(text(cx, "et.settings.title", &[]))
                .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenSettings), cx))
        });
        let end = div()
            .flex()
            .items_center()
            .gap_2()
            .children(gear)
            .child(right);
        let mut shell = ColumnShell::new("workspace").glass();
        let body = match self.current().map(|tab| &tab.body) {
            Some(Body::Term(session)) => session.terminal.clone().into_any_element(),
            // A document draws again only when it changed, not at each terminal frame.
            Some(Body::Doc(doc)) => doc
                .clone()
                .cached(StyleRefinement::default().size_full())
                .into_any_element(),
            None if self.dormant.contains_key(&self.selected) => self.asleep(cx),
            None => self.empty(cx).into_any_element(),
        };
        let mut terminal = ShellColumn::new("terminal")
            .title(self.title(cx))
            .child(self.tab_strip(cx))
            .child(div().flex_1().min_h_0().child(body));
        if settings.show_sessions {
            let added = workspace.clone();
            let add = IconButton::new("add-project", IconName::FolderPlus)
                .variant(ButtonVariant::Ghost)
                .size(ControlSize::Sm)
                .tooltip(text(cx, "et.projects.add", &[]))
                .on_click(move |_, window, cx| {
                    added.update(cx, |this, cx| this.choose_project(window, cx))
                });
            let new = IconButton::new("new-session", IconName::Plus)
                .variant(ButtonVariant::Ghost)
                .size(ControlSize::Sm)
                .tooltip(text(cx, "et.session.new", &[]))
                .on_click(move |_, window, cx| {
                    workspace.update(cx, |this, cx| this.new_session(window, cx))
                });
            let column = self
                .side_column(Side::Sessions, cx)
                .leading(left)
                .title(text(cx, "et.projects.title", &[]))
                .action(div().flex().items_center().gap_1().child(add).child(new))
                .child(self.homes(now, cx));
            shell = shell.column(column);
        } else {
            terminal = terminal.leading(left);
        }
        if settings.show_panel {
            let tab = settings.panel;
            let switch = self.panel_tabs(cx);
            let quiet = self
                .current()
                .and_then(|tab| tab.term())
                .is_some_and(|s| !s.reports);
            let caption = || {
                Caption::new(text(cx, "et.panel.no_folder", &[]))
                    .px_3()
                    .pt_2()
            };
            // Hidden, not dropped, so each keeps its state.
            let body = |on: bool| {
                div()
                    .flex_1()
                    .min_h_0()
                    .p_2()
                    .when(!on, |body| body.hidden())
            };
            // Git's list draws every row, so terminal output must not redraw it.
            let fill = || StyleRefinement::default().size_full();
            shell = shell.column(terminal).column(
                self.side_column(Side::Panel, cx)
                    .action(end)
                    // Below the title bar, clear of Windows' caption buttons.
                    .child(div().flex().justify_center().px_2().pt_2().child(switch))
                    .child(div().flex_none().child(self.folder_actions(cx)))
                    .children(quiet.then(caption))
                    .child(body(tab == PanelTab::Git).child(self.git.clone().cached(fill())))
                    .child(body(tab == PanelTab::Files).child(self.files_body(cx))),
            );
        } else {
            shell = shell.column(terminal.action(end));
        }
        // The context sits above the focus root, so its keys work wherever focus is.
        crate::interface::dress(div(), cx)
            .size_full()
            .key_context(CONTEXT)
            .map(menus::window_actions)
            .on_action(cx.listener(|this, _: &ToggleSessions, _, cx| {
                this.toggle(
                    |settings| settings.show_sessions = !settings.show_sessions,
                    cx,
                )
            }))
            .on_action(cx.listener(|this, _: &TogglePanel, _, cx| {
                this.toggle(|settings| settings.show_panel = !settings.show_panel, cx)
            }))
            .on_action(cx.listener(|this, _: &NewSession, window, cx| {
                if this.asking.is_none() {
                    this.new_session(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &NewClaude, window, cx| {
                if this.asking.is_none() {
                    this.new_claude(None, window, cx);
                }
            }))
            // Only a shown tab closes, so the menu item waits for one.
            .when_some(self.current_id(), |root, id| {
                root.on_action(cx.listener(move |this, _: &CloseSession, window, cx| {
                    if this.asking.is_none() {
                        this.ask_close(id, window, cx);
                    }
                }))
            })
            .on_action(cx.listener(|this, _: &NextSession, window, cx| {
                if this.asking.is_none() {
                    this.step(true, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &PreviousSession, window, cx| {
                if this.asking.is_none() {
                    this.step(false, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &LastSession, window, cx| {
                if this.asking.is_none()
                    && let Some(&last) = this.strip().last()
                {
                    this.show(last, window, cx);
                }
            }))
            .on_action(cx.listener(|this, select: &SelectSession, window, cx| {
                if this.asking.is_some() {
                    return;
                }
                match this.strip().get(select.0) {
                    Some(&id) => this.show(id, window, cx),
                    None => cx.propagate(),
                }
            }))
            .child(
                ResizeBorder::new()
                    .when(self.background.see_through(), ResizeBorder::glass)
                    .child(FocusScope::new(&self.focus).root().size_full().child(shell)),
            )
            .children(self.dialog(cx))
    }
}
