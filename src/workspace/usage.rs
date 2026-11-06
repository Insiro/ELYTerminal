//! What a window shows of usage, and the watches that keep it read while shown.

use gpui::{App, AppContext, Context, Entity, Subscription};

use super::Workspace;
use crate::{
    settings::PanelTab,
    settings::SettingsStore,
    usage::{Usage, Watch, view::UsageView},
};

/// The window's watches: the projects column's summary and the Usage tab.
pub(super) struct Shown {
    pub(super) tab: Entity<UsageView>,
    summary: Option<Watch>,
    shown: Option<Watch>,
    _usage: Subscription,
}

impl Shown {
    pub(super) fn new(cx: &mut Context<Workspace>) -> Self {
        let usage = Usage::entity(cx);
        // Only the summary draws in the workspace itself.
        let observed = cx.observe(&usage, |this, _, cx| {
            if this.usage.summary.is_some() {
                cx.notify();
            }
        });
        Self {
            tab: cx.new(UsageView::new),
            summary: None,
            shown: None,
            _usage: observed,
        }
    }

    /// Holds a watch for each part the settings show.
    pub(super) fn follow(&mut self, tab: PanelTab, cx: &App) {
        let settings = SettingsStore::get(cx);
        let summary = settings.show_sessions && settings.usage_summary;
        if summary != self.summary.is_some() {
            self.summary = summary.then(|| Usage::watch(cx));
        }
        let shown = settings.show_panel && tab == PanelTab::Usage;
        if shown != self.shown.is_some() {
            self.shown = shown.then(|| Usage::watch(cx));
        }
    }
}
