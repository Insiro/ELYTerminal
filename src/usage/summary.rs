//! The projects column's last line: each source's live windows, as percents.

use ely_gpui_component::{
    i18n::text,
    primitives::Tooltip,
    theme::{ActiveTheme, TextSize},
};
use gpui::{
    App, InteractiveElement, IntoElement, ParentElement, SharedString, StatefulInteractiveElement,
    Styled, div,
};

use super::{Limits, Usage, time};
use crate::settings::{PanelTab, SettingsStore};

/// `Claude 5h 23% · 7d 41%`, a part for each source with a window whose reset is ahead.
pub fn line(sources: &[(&str, Option<&Limits>)], now: i64) -> Option<String> {
    let parts: Vec<String> = sources
        .iter()
        .filter_map(|(name, limits)| {
            let limits = (*limits)?;
            let windows: Vec<String> = [limits.five_hour, limits.seven_day]
                .into_iter()
                .flatten()
                .filter(|window| window.resets_at > now)
                .map(|window| {
                    let span = window.minutes.map_or_else(String::new, time::span);
                    format!("{span} {:.0}%", window.percent)
                })
                .collect();
            (!windows.is_empty()).then(|| format!("{name} {}", windows.join(" · ")))
        })
        .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// The line, pressed to show the Usage tab; none while no source has a live window.
pub fn element(cx: &App) -> Option<impl IntoElement> {
    let usage = Usage::entity(cx).read(cx);
    let file_on = SettingsStore::get(cx).codex_usage;
    let sources = [
        ("Claude", usage.claude_shown().map(|shown| shown.limits)),
        (
            "Codex",
            usage.codex_shown(file_on).map(|shown| shown.limits),
        ),
    ];
    let line = line(&sources, time::now())?;
    let theme = cx.theme();
    let tip: SharedString = text(cx, "et.usage.summary_tip", &[]);
    Some(
        div()
            .id("usage-summary")
            .flex_none()
            .px_3()
            .py_2()
            .border_t_1()
            .border_color(theme.colors.border)
            .text_size(theme.text_size(TextSize::Sm))
            .text_color(theme.colors.fg_muted)
            .cursor_pointer()
            .hover(|style| style.text_color(theme.colors.fg))
            .tooltip(Tooltip::text(tip))
            .child(line)
            .on_click(|_, _, cx| {
                let shown = SettingsStore::update(cx, |settings| {
                    settings.show_panel = true;
                    settings.panel = PanelTab::Usage;
                });
                match shown {
                    Ok(()) => log::info!("usage: the summary shows the Usage tab"),
                    Err(error) => log::error!("usage: the Usage tab stays hidden: {error:#}"),
                }
            }),
    )
}
