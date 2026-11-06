//! Settings › Usage: Claude's wrapper, put in only on Apply in a dialog that shows the change, and Codex's switch.

use ely_gpui_component::{
    buttons::Button,
    chat::CodeBlock,
    feedback::InlineMessage,
    i18n::text,
    overlays::{Close, Dialog},
    primitives::Severity,
    settings::{SettingsRow, SettingsSection},
    theme::{ActiveTheme, ControlSize},
    typography::Caption,
};
use gpui::{
    AnyElement, App, Context, Div, IntoElement, ParentElement, SharedString, Styled, Subscription,
    Window, div,
};

use super::{SettingsWindow, open_window, switch};
use crate::usage::{
    Usage, Watch,
    install::{Plan, Status, pretty},
    time,
};

/// What the page holds while it shows: its watch, the open dialog, the last failure.
pub(super) struct Page {
    watch: Option<Watch>,
    asking: Option<Asking>,
    failed: Option<SharedString>,
    _usage: Subscription,
}

/// The dialog open: the change Apply makes, or the steps to make it by hand.
struct Asking {
    plan: Plan,
    manual: bool,
}

impl Page {
    pub(super) fn new(cx: &mut Context<SettingsWindow>) -> Self {
        let usage = Usage::entity(cx);
        let observed = cx.observe(&usage, |this, _, cx| {
            if this.usage.watch.is_some() {
                cx.notify();
            }
        });
        Self {
            watch: None,
            asking: None,
            failed: None,
            _usage: observed,
        }
    }
}

/// Opens the settings at Usage.
pub fn open_usage(cx: &mut App) {
    open_window(cx);
    let open = cx
        .windows()
        .into_iter()
        .find_map(|window| window.downcast::<SettingsWindow>());
    if let Some(window) = open {
        let shown = window.update(cx, |this, _, cx| {
            this.section = "usage".into();
            cx.notify();
        });
        if let Err(error) = shown {
            log::error!("settings: usage did not open: {error:#}");
        }
    }
}

impl SettingsWindow {
    /// Watches usage while its page shows, and reads the wrapper's status as it opens.
    pub(super) fn follow_usage(&mut self, cx: &mut Context<Self>) {
        let shown = self.section == "usage";
        if shown == self.usage.watch.is_some() {
            return;
        }
        self.usage.watch = shown.then(|| Usage::watch(cx));
        if shown {
            Usage::entity(cx).update(cx, |usage, cx| usage.recheck(cx));
        }
    }

    pub(super) fn usage_page(&self, cx: &Context<Self>) -> Div {
        let settings = crate::settings::SettingsStore::get(cx);
        let this = cx.entity();
        let word = |key: &str| text(cx, key, &[]);
        let status = Usage::entity(cx).read(cx).install.clone();
        let (state, button) = match &status {
            None => (None, None),
            Some(Status::Off) => (None, Some(("turn-on", "et.usage.turn_on"))),
            Some(Status::On) => (
                Some(word("et.usage.state_on")),
                Some(("turn-off", "et.usage.turn_off")),
            ),
            Some(Status::Elsewhere) => (
                Some(word("et.usage.elsewhere")),
                Some(("forget", "et.usage.forget")),
            ),
            Some(Status::Missing) => (Some(word("et.usage.missing")), None),
            Some(Status::Error(why)) => (Some(text(cx, "et.usage.error", &[("why", why)])), None),
        };
        let control = button.map(|(id, key)| {
            let this = this.clone();
            let button = Button::new(id, word(key)).size(ControlSize::Sm);
            let button = if id == "turn-on" {
                button.primary()
            } else {
                button
            };
            button.on_click(move |_, _, cx| this.update(cx, |this, cx| this.press(id, cx)))
        });
        let claude = SettingsRow::new(word("et.usage.claude_limits"))
            .description(word("et.usage.claude_hint"))
            .control(div().flex().items_center().gap_2().children(control));
        let codex = switch(&this, "codex_usage", settings.codex_usage, |s, on| {
            s.codex_usage = on
        });
        let claude_api = switch(
            &this,
            "claude_api_usage",
            settings.claude_api_usage,
            |s, on| s.claude_api_usage = on,
        );
        let codex_live = switch(
            &this,
            "codex_live_usage",
            settings.codex_live_usage,
            |s, on| s.codex_live_usage = on,
        );
        let summary = switch(&this, "usage_summary", settings.usage_summary, |s, on| {
            s.usage_summary = on
        });
        let section = SettingsSection::new(word("et.settings.usage"))
            .row(
                SettingsRow::new(word("et.usage.claude_api"))
                    .description(word("et.usage.claude_api_hint"))
                    .control(claude_api),
            )
            .row(claude)
            .row(
                SettingsRow::new(word("et.usage.codex_live"))
                    .description(word("et.usage.codex_live_hint"))
                    .control(codex_live),
            )
            .row(
                SettingsRow::new(word("et.usage.codex"))
                    .description(word("et.usage.codex_hint"))
                    .control(codex),
            )
            .row(
                SettingsRow::new(word("et.usage.summary"))
                    .description(word("et.usage.summary_hint"))
                    .control(summary),
            );
        let tone = |why: SharedString| InlineMessage::new(Severity::Danger, why);
        div()
            .flex()
            .flex_col()
            .gap_4()
            .children(self.usage.failed.clone().map(tone))
            .children(state.map(|state| match status {
                Some(Status::On) => InlineMessage::new(Severity::Success, state),
                _ => InlineMessage::new(Severity::Warning, state),
            }))
            .child(section)
            .child(Caption::new(word("et.usage.project_note")))
    }

    /// Runs a row's button: the dialog for Turn on, else at once.
    fn press(&mut self, id: &str, cx: &mut Context<Self>) {
        let usage = Usage::entity(cx);
        let install = usage.read(cx).paths().install.clone();
        self.usage.failed = None;
        let done = match id {
            "turn-on" => install.plan().map(|plan| {
                log::info!(
                    "settings: usage asks before it wraps {}",
                    plan.path.display()
                );
                self.usage.asking = Some(Asking {
                    plan,
                    manual: false,
                });
            }),
            "turn-off" => install.revert(time::now()).map(|status| {
                log::info!("settings: usage turned off: {status:?}");
            }),
            "forget" => {
                install.forget();
                Ok(())
            }
            other => unreachable!("settings: no usage button {other}"),
        };
        if let Err(error) = done {
            log::error!("settings: usage {id} failed: {error:#}");
            self.usage.failed = Some(format!("{error:#}").into());
        }
        usage.update(cx, |usage, cx| usage.recheck(cx));
        cx.notify();
    }

    /// Closes the dialog, writing what it asked for when `write` is set.
    fn answer(&mut self, write: bool, cx: &mut Context<Self>) {
        let Some(asking) = self.usage.asking.take() else {
            return;
        };
        let usage = Usage::entity(cx);
        let install = usage.read(cx).paths().install.clone();
        let written = match (write, asking.manual) {
            (false, _) => {
                log::info!("settings: usage's change is cancelled");
                Ok(())
            }
            (true, false) => install.apply(&asking.plan, time::now()),
            (true, true) => install.prepare(&asking.plan, time::now()).map(|()| {
                log::info!("settings: usage's files are ready for a statusLine set by hand");
            }),
        };
        if let Err(error) = written {
            log::error!("settings: usage's change failed: {error:#}");
            self.usage.failed = Some(format!("{error:#}").into());
        }
        usage.update(cx, |usage, cx| usage.recheck(cx));
        cx.notify();
    }

    pub(super) fn usage_dialog(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let asking = self.usage.asking.as_ref()?;
        let word = |key: &str| text(cx, key, &[]);
        let this = cx.entity();
        let on_close = {
            let this = this.clone();
            move |_: &mut Window, cx: &mut App| this.update(cx, |this, cx| this.answer(false, cx))
        };
        let code = |id: &'static str, value: String| CodeBlock::new(id, value).language("json");
        let path = asking.plan.path.display().to_string();
        let shown = |title: &str, block: CodeBlock| {
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(Caption::new(word(title)))
                .child(block)
        };
        let body = if asking.manual {
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(Caption::new(text(
                    cx,
                    "et.usage.manual_note",
                    &[("path", &path)],
                )))
                .child(code(
                    "usage-manual",
                    format!("\"statusLine\": {}", pretty(&asking.plan.after)),
                ))
        } else {
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(shown(
                    "et.usage.before",
                    code("usage-before", Plan::shown(asking.plan.before.as_ref())),
                ))
                .child(shown(
                    "et.usage.after",
                    code("usage-after", pretty(&asking.plan.after)),
                ))
                .child(Caption::new(word("et.usage.dialog_note")))
        };
        let manual = asking.manual;
        let (cancel, steps, apply) = (
            word("et.usage.cancel"),
            word(if manual {
                "et.usage.back"
            } else {
                "et.usage.manual"
            }),
            word(if manual {
                "et.usage.done"
            } else {
                "et.usage.apply"
            }),
        );
        let cancel = move |close: Close| {
            Button::new("usage-cancel", cancel)
                .size(ControlSize::Sm)
                .on_click(move |_, window, cx| close(window, cx))
        };
        let flip = this.clone();
        let steps = move |_: Close| {
            Button::new("usage-manual", steps)
                .size(ControlSize::Sm)
                .on_click(move |_, _, cx| {
                    flip.update(cx, |this, cx| {
                        if let Some(asking) = this.usage.asking.as_mut() {
                            asking.manual = !asking.manual;
                        }
                        cx.notify();
                    })
                })
        };
        let apply = move |close: Close| {
            Button::new("usage-apply", apply)
                .primary()
                .size(ControlSize::Sm)
                .on_click(move |_, window, cx| {
                    this.update(cx, |this, cx| this.answer(true, cx));
                    close(window, cx);
                })
        };
        let width = cx.theme().prose_width();
        Some(
            Dialog::new("usage-consent", word("et.usage.dialog_title"), on_close)
                .detail(path)
                .child(div().w(width).child(body))
                .action(cancel)
                .action(steps)
                .action(apply)
                .into_any_element(),
        )
    }
}
