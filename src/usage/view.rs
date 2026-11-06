//! The Usage tab: each source's windows as meters, their resets and how old the reading is.

use ely_gpui_component::{
    buttons::Button,
    data_display::Meter,
    feedback::InlineMessage,
    i18n::text,
    primitives::Severity,
    theme::{ActiveTheme, ControlSize, TextSize},
    typography::Caption,
};
use gpui::{
    AnyElement, App, Context, Div, FontWeight, InteractiveElement, IntoElement, ParentElement,
    Render, SharedString, StatefulInteractiveElement, Styled, Subscription, Window, div,
};

use super::{
    Limits, Shown, Usage, Window as Span,
    codex::Tokens,
    install::Status,
    live::Trouble,
    time::{self, Age, Reset},
};
use crate::settings::SettingsStore;

/// The tab's body; it draws again only when usage changes.
pub struct UsageView {
    _usage: Subscription,
}

impl UsageView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let usage = Usage::entity(cx);
        Self {
            _usage: cx.observe(&usage, |_, _, cx| cx.notify()),
        }
    }
}

impl Render for UsageView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let usage = Usage::entity(cx).read(cx);
        let now = time::now();
        let word = |key: &str| text(cx, key, &[]);
        let settings = SettingsStore::get(cx);
        let mut claude = section(word("et.usage.claude_title"), cx);
        match (usage.claude_shown(), &usage.install) {
            (Some(shown), _) => {
                claude = claude
                    .children(meters("claude", shown.limits, now, cx))
                    .child(age(now, shown, cx));
            }
            (None, Some(Status::On)) => {
                claude = claude.child(Caption::new(word("et.usage.claude_waiting")))
            }
            (None, _) => {
                let open = Button::new("usage-open-settings", word("et.usage.open_settings"))
                    .size(ControlSize::Sm)
                    .on_click(|_, _, cx| crate::settings::window::usage::open_usage(cx));
                claude = claude
                    .child(Caption::new(word("et.usage.claude_off")))
                    .child(div().child(open));
            }
        }
        claude = claude
            .children(trouble(usage.claude_live.trouble.as_ref(), now, cx))
            .children(failure(&usage.claude.error));
        let mut codex = section(word("et.usage.codex_title"), cx);
        let file_on = settings.codex_usage;
        let tokens = usage
            .codex
            .value
            .as_ref()
            .filter(|_| file_on)
            .and_then(|reading| reading.tokens);
        codex = match usage.codex_shown(file_on) {
            _ if !file_on && !settings.codex_live_usage => {
                codex.child(Caption::new(word("et.usage.codex_off")))
            }
            None if usage.codex.checked || usage.codex_live.trouble.is_some() => {
                codex.child(Caption::new(word("et.usage.codex_none")))
            }
            None => codex,
            Some(shown) => codex
                .children(meters("codex", shown.limits, now, cx))
                .child(age(now, shown, cx)),
        };
        codex = codex
            .children(tokens.map(|tokens| Caption::new(tokens_line(tokens, cx))))
            .children(
                usage
                    .codex_live
                    .value
                    .as_ref()
                    .and_then(|live| live.credits.as_deref())
                    .map(|credits| Caption::new(text(cx, "et.usage.credits", &[("n", credits)]))),
            )
            .children(trouble(usage.codex_live.trouble.as_ref(), now, cx))
            .children(failure(&usage.codex.error));
        div()
            .id("usage-body")
            .size_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_4()
            .px_1()
            .child(claude)
            .child(codex)
    }
}

fn section(title: SharedString, cx: &App) -> Div {
    let theme = cx.theme();
    div().flex().flex_col().gap_2().child(
        div()
            .text_size(theme.text_size(TextSize::Sm))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme.colors.fg)
            .child(title),
    )
}

fn failure(error: &Option<String>) -> Option<InlineMessage> {
    error
        .clone()
        .map(|why| InlineMessage::new(Severity::Warning, why))
}

/// A meter for each window a source reports; one whose reset passed shows no percent.
pub fn meters(id: &str, limits: &Limits, now: i64, cx: &App) -> Vec<AnyElement> {
    let spend = text(cx, "et.usage.spend", &[]).to_string();
    [
        ("5h", limits.five_hour),
        ("7d", limits.seven_day),
        ("spend", limits.spend),
    ]
    .into_iter()
    .filter_map(|(key, window)| {
        let window = window?;
        let name = match (key, window.minutes) {
            ("spend", _) => spend.clone(),
            (_, Some(minutes)) => time::span(minutes),
            (key, None) => key.to_string(),
        };
        Some(meter(format!("usage-{id}-{key}"), name, window, now, cx))
    })
    .collect()
}

fn meter(id: String, name: String, window: Span, now: i64, cx: &App) -> AnyElement {
    match time::reset(now, window.resets_at, time::local) {
        Reset::Passed => {
            Caption::new(format!("{name} · {}", resets(Reset::Passed, cx))).into_any_element()
        }
        reset => {
            let percent = format!("{:.0}%", window.percent);
            let when = resets(reset, cx);
            let detail = text(
                cx,
                "et.usage.detail",
                &[("percent", &percent), ("when", &when)],
            );
            let share = (window.percent / 100.0).clamp(0.0, 1.0) as f32;
            Meter::new(SharedString::from(id), name, share)
                .detail(detail)
                .into_any_element()
        }
    }
}

/// Each weekday's key, Sunday first.
const DAYS: [&str; 7] = [
    "et.usage.sun",
    "et.usage.mon",
    "et.usage.tue",
    "et.usage.wed",
    "et.usage.thu",
    "et.usage.fri",
    "et.usage.sat",
];

/// When a window resets, in words.
pub fn resets(reset: Reset, cx: &App) -> String {
    match reset {
        Reset::Passed => text(cx, "et.usage.reset_passed", &[]).to_string(),
        Reset::In {
            days,
            hours,
            minutes,
        } => {
            let span = match (days, hours) {
                (0, 0) => format!("{minutes}m"),
                (0, _) => format!("{hours}h {minutes:02}m"),
                _ => format!("{days}d {hours}h"),
            };
            text(cx, "et.usage.resets_in", &[("time", &span)]).to_string()
        }
        Reset::At {
            weekday,
            hour,
            minute,
        } => {
            let day = text(cx, DAYS[usize::from(weekday) % 7], &[]);
            let clock = format!("{hour:02}:{minute:02}");
            text(cx, "et.usage.resets_at", &[("day", &day), ("time", &clock)]).to_string()
        }
    }
}

/// How old a reading is, muted past an hour, with its plan and where it came from.
fn age(now: i64, shown: Shown, cx: &App) -> Div {
    let age = time::age(now, shown.at);
    let words = match age {
        Age::Now => text(cx, "et.usage.age_now", &[]),
        Age::Minutes(n) => text(cx, "et.usage.age_minutes", &[("n", &n.to_string())]),
        Age::Hours(n) => text(cx, "et.usage.age_hours", &[("n", &n.to_string())]),
        Age::Days(n) => text(cx, "et.usage.age_days", &[("n", &n.to_string())]),
    };
    let words = match shown.plan {
        Some(plan) => format!("{plan} · {words}"),
        None => words.to_string(),
    };
    let theme = cx.theme();
    let color = if age.stale() {
        theme.colors.fg_muted
    } else {
        theme.colors.fg
    };
    div()
        .flex()
        .gap_2()
        .text_size(theme.text_size(TextSize::Sm))
        .child(div().text_color(color).child(words))
        .child(
            div()
                .text_color(theme.colors.fg_muted)
                .child(text(cx, shown.origin.key(), &[])),
        )
}

/// Why the last fetch gave nothing, on one line.
fn trouble(trouble: Option<&Trouble>, now: i64, cx: &App) -> Option<AnyElement> {
    let words = match trouble? {
        Trouble::Expired => text(cx, "et.usage.expired", &[]),
        Trouble::NoToken => text(cx, "et.usage.no_token", &[]),
        Trouble::NoCodex => text(cx, "et.usage.no_codex", &[]),
        Trouble::Limited(until) => {
            let minutes = ((until - now).max(0) + 59) / 60;
            text(cx, "et.usage.limited", &[("n", &minutes.to_string())])
        }
        Trouble::Failed(why) => {
            return Some(InlineMessage::new(Severity::Warning, why.clone()).into_any_element());
        }
    };
    Some(Caption::new(words).into_any_element())
}

fn tokens_line(tokens: Tokens, cx: &App) -> SharedString {
    let line = text(
        cx,
        "et.usage.tokens",
        &[
            ("total", &count(tokens.total)),
            ("cached", &count(tokens.cached)),
            ("last", &count(tokens.last)),
        ],
    );
    match tokens.window.filter(|&window| window > 0) {
        Some(window) => {
            let share = format!("{:.0}", tokens.last as f64 * 100.0 / window as f64);
            format!(
                "{line} · {}",
                text(cx, "et.usage.context", &[("percent", &share)])
            )
            .into()
        }
        None => line,
    }
}

/// A token count in thousands or millions.
pub fn count(n: u64) -> String {
    match n {
        0..1_000 => n.to_string(),
        1_000..1_000_000 => format!("{}k", n / 1_000),
        _ => format!("{:.1}M", n as f64 / 1_000_000.0),
    }
}
