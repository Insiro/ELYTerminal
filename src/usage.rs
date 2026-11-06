//! Agent usage: Claude's limits from its API and its statusLine, Codex's from its app server and its session files.

use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

use gpui::{App, AppContext, Context, Entity, Global, Task};

use crate::settings::SettingsStore;

pub mod api;
pub mod appserver;
pub mod claude;
pub mod codex;
pub mod install;
pub mod keychain;
pub mod live;
pub mod poll;
pub mod run;
pub mod summary;
#[cfg(test)]
mod tests;
pub mod time;
pub mod view;
pub mod web;

use live::{Live, Origin, Outcome};
use poll::{Changes, Paths, Poll};

/// How often the files are looked at while watched.
const TICK: Duration = Duration::from_secs(2);
/// How often a watched view draws again, for its countdowns.
const REDRAW: Duration = Duration::from_secs(60);

/// One limit's window: how much of it is used and when it starts again.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Window {
    /// 0 to 100; a spend limit may pass 100.
    pub percent: f64,
    /// Unix seconds.
    pub resets_at: i64,
    /// The window's length, where the source names it.
    pub minutes: Option<u32>,
}

/// The windows a source reports; each may be absent.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Limits {
    pub five_hour: Option<Window>,
    pub seven_day: Option<Window>,
    pub spend: Option<Window>,
}

/// A source's last good reading and its failure, if the last look failed.
#[derive(Clone, Debug, PartialEq)]
pub struct Source<T> {
    pub value: Option<T>,
    pub error: Option<String>,
    /// Whether it was looked at yet.
    pub checked: bool,
}

impl<T> Default for Source<T> {
    fn default() -> Self {
        Self {
            value: None,
            error: None,
            checked: false,
        }
    }
}

impl<T: PartialEq + std::fmt::Debug> Source<T> {
    /// Takes what a look found, logging each change; a failure keeps the last good value.
    pub fn set(&mut self, name: &str, found: Result<Option<T>, String>) -> bool {
        let first = !std::mem::replace(&mut self.checked, true);
        match found {
            Ok(value) => {
                let cleared = self.error.take().is_some();
                if cleared {
                    log::info!("usage: {name} reads again");
                }
                let changed = self.value != value;
                match (&self.value, &value) {
                    (None, Some(value)) => log::info!("usage: {name} found: {value:?}"),
                    (Some(_), None) => log::info!("usage: {name} lost"),
                    (Some(_), Some(value)) if changed => {
                        log::debug!("usage: {name} reads {value:?}")
                    }
                    (None, None) if first => log::info!("usage: {name} has no data"),
                    _ => {}
                }
                self.value = value;
                first || cleared || changed
            }
            Err(error) => {
                if self.error.as_ref() == Some(&error) {
                    return false;
                }
                log::warn!("{error}");
                self.error = Some(error);
                true
            }
        }
    }
}

/// The latest readings of every source, app-wide since limits are an account's.
pub struct Usage {
    pub claude: Source<poll::Claude>,
    pub codex: Source<codex::Reading>,
    pub claude_live: Live,
    pub codex_live: Live,
    /// The wrapper's status; none until first read.
    pub install: Option<install::Status>,
    paths: Paths,
    watchers: Rc<Cell<usize>>,
    drawn: Instant,
    _poll: Task<()>,
    _fetch: [Task<()>; 2],
}

/// Which service a fetch asks.
#[derive(Clone, Copy, Debug)]
enum Kind {
    Claude,
    Codex,
}

/// A source's newest reading, whichever way it came.
#[derive(Clone, Copy, Debug)]
pub struct Shown<'a> {
    pub limits: &'a Limits,
    pub at: i64,
    pub origin: Origin,
    pub plan: Option<&'a str>,
}

/// The newer of two readings; a tie goes to the first.
fn newest<'a>(a: Option<Shown<'a>>, b: Option<Shown<'a>>) -> Option<Shown<'a>> {
    match (a, b) {
        (Some(a), Some(b)) => Some(if b.at > a.at { b } else { a }),
        (a, b) => a.or(b),
    }
}

struct Shared(Entity<Usage>);

impl Global for Shared {}

/// Held while something shows usage; with none the files rest.
pub struct Watch(Rc<Cell<usize>>);

impl Drop for Watch {
    fn drop(&mut self) {
        let left = self.0.get() - 1;
        self.0.set(left);
        if left == 0 {
            log::debug!("usage: no one watches; reads rest");
        }
    }
}

/// The usage folder in the app's config folder.
pub fn folder() -> anyhow::Result<std::path::PathBuf> {
    Ok(crate::identity::config_dir()?.join("usage"))
}

/// Starts the app's usage, resting until watched.
pub fn init(cx: &mut App) -> anyhow::Result<()> {
    let settings = crate::projects::claude::config_dir().map(|dir| dir.join("settings.json"));
    let install = install::Install {
        settings,
        usage: folder()?,
    };
    let paths = Paths {
        install,
        codex: codex::home(),
    };
    log::info!(
        "usage: claude's data in {}, its settings {:?}, codex in {:?}",
        paths.install.usage.display(),
        paths.install.settings,
        paths.codex
    );
    let usage = cx.new(|cx| Usage::new(paths, cx));
    cx.set_global(Shared(usage));
    Ok(())
}

impl Usage {
    fn new(paths: Paths, cx: &mut Context<Self>) -> Self {
        let poll = cx.spawn(async move |this, cx| {
            let mut poll = Poll::default();
            loop {
                cx.background_executor().timer(TICK).await;
                let due = match this.update(cx, |this, cx| this.due(cx)) {
                    Ok(due) => due,
                    Err(_) => break,
                };
                let Some((paths, codex_on)) = due else {
                    continue;
                };
                let (back, changes) = cx
                    .background_executor()
                    .spawn(async move {
                        let changes = poll.tick(&paths, codex_on, Instant::now());
                        (poll, changes)
                    })
                    .await;
                poll = back;
                if this.update(cx, |this, cx| this.apply(changes, cx)).is_err() {
                    break;
                }
            }
        });
        Self {
            claude: Source::default(),
            codex: Source::default(),
            install: None,
            paths,
            watchers: Rc::new(Cell::new(0)),
            drawn: Instant::now(),
            _poll: poll,
            _fetch: [fetcher(Kind::Claude, cx), fetcher(Kind::Codex, cx)],
            claude_live: Live::default(),
            codex_live: Live::default(),
        }
    }

    pub fn entity(cx: &App) -> Entity<Self> {
        cx.global::<Shared>().0.clone()
    }

    /// A guard that keeps the files read while it lives.
    pub fn watch(cx: &App) -> Watch {
        let watchers = Self::entity(cx).read(cx).watchers.clone();
        if watchers.get() == 0 {
            log::debug!("usage: watched; reads start");
        }
        watchers.set(watchers.get() + 1);
        Watch(watchers)
    }

    pub fn paths(&self) -> &Paths {
        &self.paths
    }

    /// What the next tick reads, none while no one watches; draws again once a minute while watched.
    fn due(&mut self, cx: &mut Context<Self>) -> Option<(Paths, bool)> {
        if self.watchers.get() == 0 {
            return None;
        }
        let codex_on = SettingsStore::get(cx).codex_usage;
        if !codex_on && self.codex != Source::default() {
            log::info!("usage: codex reading off");
            self.codex = Source::default();
            self.redraw(cx);
        }
        if self.drawn.elapsed() >= REDRAW {
            self.redraw(cx);
        }
        Some((self.paths.clone(), codex_on))
    }

    fn apply(&mut self, changes: Changes, cx: &mut Context<Self>) {
        let changes = keep(changes, SettingsStore::get(cx).codex_usage);
        let mut changed = false;
        if let Some(found) = changes.claude {
            changed |= self.claude.set("claude", found);
        }
        if let Some(found) = changes.codex {
            changed |= self.codex.set("codex", found);
        }
        if let Some(status) = changes.install {
            changed |= self.set_install(status);
        }
        if changed {
            self.redraw(cx);
        }
    }

    /// Reads the wrapper's status now, as after the user changed it.
    pub fn recheck(&mut self, cx: &mut Context<Self>) {
        let status = self.paths.install.status();
        if self.set_install(status) {
            self.redraw(cx);
        }
    }

    fn set_install(&mut self, status: install::Status) -> bool {
        if self.install.as_ref() == Some(&status) {
            return false;
        }
        log::info!("usage: claude's wrapper is {status:?}");
        self.install = Some(status);
        true
    }

    /// Claude's newest reading: the API's or the statusLine's.
    pub fn claude_shown(&self) -> Option<Shown<'_>> {
        let file = self.claude.value.as_ref().map(|found| Shown {
            limits: &found.limits,
            at: found.at,
            origin: Origin::StatusLine,
            plan: None,
        });
        newest(self.claude_live.value.as_ref().map(shown), file)
    }

    /// Codex's newest reading: the app server's or the page's, else the session file's.
    pub fn codex_shown(&self, file_on: bool) -> Option<Shown<'_>> {
        let file = self
            .codex
            .value
            .as_ref()
            .filter(|_| file_on)
            .and_then(|reading| {
                Some(Shown {
                    limits: &reading.limits,
                    at: reading.limits_at?,
                    origin: Origin::SessionFile,
                    plan: reading.plan.as_deref(),
                })
            });
        newest(self.codex_live.value.as_ref().map(shown), file)
    }

    fn live(&mut self, kind: Kind) -> &mut Live {
        match kind {
            Kind::Claude => &mut self.claude_live,
            Kind::Codex => &mut self.codex_live,
        }
    }

    /// Whether `kind` is fetched now, clearing it while its setting is off.
    fn fetch_due(&mut self, kind: Kind, cx: &mut Context<Self>) -> bool {
        let settings = SettingsStore::get(cx);
        let on = match kind {
            Kind::Claude => settings.claude_api_usage,
            Kind::Codex => settings.codex_live_usage,
        };
        let watched = on && self.watchers.get() > 0;
        if !on && self.live(kind).clear() {
            log::info!("usage: {kind:?}'s live fetch off");
            self.redraw(cx);
        }
        self.live(kind).schedule.due(watched, Instant::now())
    }

    fn fetched(&mut self, kind: Kind, outcome: Outcome, cx: &mut Context<Self>) {
        let live = self.live(kind);
        live.schedule.done(&outcome, Instant::now());
        let on = match kind {
            Kind::Claude => SettingsStore::get(cx).claude_api_usage,
            Kind::Codex => SettingsStore::get(cx).codex_live_usage,
        };
        if !on {
            log::debug!("usage: a late {kind:?} fetch is dropped");
            return;
        }
        let name = match kind {
            Kind::Claude => "claude's API",
            Kind::Codex => "codex's live usage",
        };
        if self.live(kind).set(name, outcome, time::now()) {
            self.redraw(cx);
        }
    }

    fn redraw(&mut self, cx: &mut Context<Self>) {
        self.drawn = Instant::now();
        cx.notify();
    }
}

fn shown(fetched: &live::Fetched) -> Shown<'_> {
    Shown {
        limits: &fetched.limits,
        at: fetched.at,
        origin: fetched.origin,
        plan: fetched.plan.as_deref(),
    }
}

/// Fetches `kind` off the main thread whenever its schedule says.
fn fetcher(kind: Kind, cx: &mut Context<Usage>) -> Task<()> {
    cx.spawn(async move |this, cx| {
        loop {
            cx.background_executor().timer(TICK).await;
            let due = match this.update(cx, |this, cx| this.fetch_due(kind, cx)) {
                Ok(due) => due,
                Err(_) => break,
            };
            if !due {
                continue;
            }
            // A fetch blocks up to a minute, so on a thread of its own, not the executor's pool.
            let (sent, outcome) = futures::channel::oneshot::channel();
            std::thread::spawn(move || {
                let _ = sent.send(match kind {
                    Kind::Claude => api::fetch(),
                    Kind::Codex => appserver::fetch(),
                });
            });
            let Ok(outcome) = outcome.await else {
                log::error!("usage: {kind:?}'s fetch thread died");
                continue;
            };
            if this
                .update(cx, |this, cx| this.fetched(kind, outcome, cx))
                .is_err()
            {
                break;
            }
        }
    })
}

/// Drops Codex's reading when it was turned off while the read ran.
pub fn keep(mut changes: Changes, codex_on: bool) -> Changes {
    if !codex_on && changes.codex.take().is_some() {
        log::debug!("usage: a late codex read is dropped");
    }
    changes
}
