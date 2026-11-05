use super::super::{Limits, Window, summary::line, view::count};

#[test]
fn token_counts_read_short() {
    assert_eq!(
        (count(999), count(68_121), count(779_913)),
        ("999".into(), "68k".into(), "779k".into())
    );
    assert_eq!(count(1_250_000), "1.2M");
}

#[test]
fn the_summary_shows_live_windows_of_each_source() {
    let live = |percent, minutes| {
        Some(Window {
            percent,
            resets_at: 100,
            minutes: Some(minutes),
        })
    };
    let claude = Limits {
        five_hour: live(23.4, 300),
        seven_day: live(41.0, 10_080),
        spend: None,
    };
    let codex = Limits {
        five_hour: live(9.0, 300),
        seven_day: Some(Window {
            percent: 1.0,
            resets_at: 5,
            minutes: Some(10_080),
        }),
        spend: None,
    };
    let both = line(&[("Claude", Some(&claude)), ("Codex", Some(&codex))], 50);
    assert_eq!(
        both.as_deref(),
        Some("Claude 5h 23% · 7d 41% · Codex 5h 9%")
    );
    assert_eq!(
        line(&[("Claude", None), ("Codex", Some(&codex))], 500),
        None
    );
}
