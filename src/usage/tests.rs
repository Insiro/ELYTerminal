use std::{
    env, fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use ruzstd::encoding::{CompressionLevel, compress_to_vec};

use super::{
    Window, claude,
    codex::{self, PACKED, Tokens},
    time::{self, Age, Clock, Reset},
};

/// A fresh folder under the temp dir.
pub(super) fn folder(name: &str) -> PathBuf {
    let folder = env::temp_dir().join(format!("elyterminal-usage-{name}-{}", std::process::id()));
    if folder.exists() {
        fs::remove_dir_all(&folder).expect("a stale test folder goes");
    }
    fs::create_dir_all(&folder).expect("a test folder");
    folder
}

const FULL: &str = r#"{"session_id":"x","model":{"id":"m"},"rate_limits":{
  "five_hour":{"used_percentage":23.5,"resets_at":1738425600},
  "seven_day":{"used_percentage":41.2,"resets_at":1738857600},
  "spend_limit":{"used_percentage":162.8,"resets_at":1740787200.0}}}"#;

#[test]
fn claude_reads_every_window_and_spend_past_100() {
    let limits = claude::parse(FULL).unwrap().expect("limits");
    let five = limits.five_hour.expect("five hours");
    assert_eq!(
        (five.percent, five.resets_at, five.minutes),
        (23.5, 1738425600, Some(300))
    );
    assert_eq!(limits.seven_day.unwrap().minutes, Some(10_080));
    let spend = limits.spend.unwrap();
    assert_eq!((spend.percent, spend.resets_at), (162.8, 1740787200));
}

#[test]
fn claude_keeps_the_windows_it_has() {
    let some = r#"{"rate_limits":{"seven_day":{"used_percentage":4,"resets_at":5},
        "five_hour":{"used_percentage":"x","resets_at":1}}}"#;
    let limits = claude::parse(some).unwrap().unwrap();
    assert_eq!(limits.five_hour, None);
    assert_eq!(limits.seven_day.unwrap().percent, 4.0);
    assert_eq!(claude::parse(r#"{"cwd":"/"}"#).unwrap(), None);
    assert_eq!(claude::parse(r#"{"rate_limits":null}"#).unwrap(), None);
    let empty = claude::parse(r#"{"rate_limits":{}}"#).unwrap().unwrap();
    assert_eq!(empty, Default::default());
}

#[test]
fn claude_refuses_junk() {
    let error = claude::parse("{\"rate_limits\": {").unwrap_err();
    assert!(format!("{error:#}").contains("not JSON"), "{error:#}");
}

/// A `token_count` line as Codex writes it.
fn counted(time: &str, percent: f64, rates: &str) -> String {
    format!(
        r#"{{"timestamp":"{time}","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":772006,"cached_input_tokens":701696,"output_tokens":7907,"reasoning_output_tokens":3501,"total_tokens":779913}},"last_token_usage":{{"input_tokens":67925,"output_tokens":196,"total_tokens":68121}},"model_context_window":258400}},"rate_limits":{rates}}}}}"#
    )
    .replace("PCT", &percent.to_string())
}

const RATES: &str = r#"{"limit_id":"codex","primary":{"used_percent":PCT,"window_minutes":300,"resets_at":1792759917},"secondary":{"used_percent":1.0,"window_minutes":10080,"resets_at":1793322125},"credits":{"has_credits":false},"plan_type":"plus","rate_limit_reached_type":null}"#;

/// A session file's lines: meta, a redacted message, then each given line.
fn session(lines: &[String]) -> String {
    let mut text = String::from(
        "{\"type\":\"session_meta\",\"payload\":{\"id\":\"redacted\"}}\n\
         {\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"content\":[{\"text\":\"redacted \\\"token_count\\\"\"}]}}\n",
    );
    for line in lines {
        text.push_str(line);
        text.push('\n');
    }
    text
}

fn packed(path: &Path, text: &str) {
    fs::write(
        path,
        compress_to_vec(text.as_bytes(), CompressionLevel::Fastest),
    )
    .unwrap();
}

#[test]
fn codex_keeps_the_last_limits_and_tokens() {
    let dir = folder("codex-last");
    let path = dir.join("rollout-a.jsonl.zst");
    let text = session(&[
        counted("2026-10-23T07:50:00.000Z", 5.0, RATES),
        counted("2026-10-23T07:55:30.401Z", 9.0, RATES),
        counted("2026-10-23T07:56:00.000Z", 50.0, "null"),
    ]);
    packed(&path, &text);
    let reading = codex::read(&path).unwrap();
    assert_eq!(
        reading.limits.five_hour,
        Some(Window {
            percent: 9.0,
            resets_at: 1792759917,
            minutes: Some(300)
        })
    );
    assert_eq!(reading.limits.seven_day.unwrap().minutes, Some(10_080));
    assert_eq!(reading.plan.as_deref(), Some("plus"));
    assert_eq!(
        reading.limits_at,
        time::parse_utc("2026-10-23T07:55:30.401Z")
    );
    let tokens = reading.tokens.unwrap();
    assert_eq!(
        tokens,
        Tokens {
            input: 772006,
            cached: 701696,
            output: 7907,
            total: 779913,
            last: 68121,
            window: Some(258400)
        }
    );
}

#[test]
fn codex_reads_relative_resets_and_null_info() {
    let dir = folder("codex-relative");
    let path = dir.join("rollout-b.jsonl.zst");
    let rates = r#"{"primary":{"used_percent":12.5,"window_minutes":300,"resets_in_seconds":600},"secondary":null}"#;
    let line = format!(
        r#"{{"timestamp":"1970-01-01T00:01:00Z","type":"event_msg","payload":{{"type":"token_count","info":null,"rate_limits":{rates}}}}}"#
    );
    packed(&path, &session(&[line]));
    let reading = codex::read(&path).unwrap();
    assert_eq!(reading.limits.five_hour.unwrap().resets_at, 660);
    assert_eq!(reading.limits.seven_day, None);
    assert_eq!(reading.tokens, None);
}

#[test]
fn codex_prefers_its_own_limit_id() {
    let dir = folder("codex-own");
    let path = dir.join("rollout-c.jsonl");
    let other = RATES.replace("\"codex\"", "\"other\"");
    let text = session(&[
        counted("2026-10-23T07:50:00Z", 7.0, RATES),
        counted("2026-10-23T07:51:00Z", 70.0, &other),
    ]);
    fs::write(&path, text).unwrap();
    assert_eq!(
        codex::read(&path)
            .unwrap()
            .limits
            .five_hour
            .unwrap()
            .percent,
        7.0
    );
}

#[test]
fn a_truncated_frame_keeps_the_lines_before_it() {
    let dir = folder("codex-cut");
    let path = dir.join("rollout-d.jsonl.zst");
    let mut lines = vec![counted("2026-10-23T07:50:00Z", 3.0, RATES)];
    lines.extend((0..20_000).map(|n| format!("{{\"type\":\"turn_context\",\"n\":{n}}}")));
    let whole = compress_to_vec(session(&lines).as_bytes(), CompressionLevel::Fastest);
    fs::write(&path, &whole[..whole.len() - 40]).unwrap();
    assert_eq!(
        codex::read(&path)
            .unwrap()
            .limits
            .five_hour
            .unwrap()
            .percent,
        3.0
    );
}

#[test]
fn a_plain_file_reads_its_tail() {
    let dir = folder("codex-tail");
    let path = dir.join("rollout-e.jsonl");
    let early = counted("2026-10-23T07:50:00Z", 1.0, RATES);
    let filler = "{\"type\":\"turn_context\",\"pad\":\"".to_string() + &"p".repeat(1 << 20) + "\"}";
    let late = counted("2026-10-23T08:00:00Z", 2.0, RATES);
    fs::write(&path, session(&[early, filler, late])).unwrap();
    assert_eq!(
        codex::read(&path)
            .unwrap()
            .limits
            .five_hour
            .unwrap()
            .percent,
        2.0
    );
}

#[test]
fn a_line_past_the_bound_is_skipped() {
    let dir = folder("codex-long");
    let path = dir.join("rollout-f.jsonl.zst");
    let long = counted("2026-10-23T07:59:00Z", 99.0, RATES).replace(
        "\"plan_type\"",
        &format!("\"pad\":\"{}\",\"plan_type\"", "q".repeat(5 << 20)),
    );
    let text = session(&[counted("2026-10-23T07:50:00Z", 4.0, RATES), long]);
    packed(&path, &text);
    assert_eq!(
        codex::read(&path)
            .unwrap()
            .limits
            .five_hour
            .unwrap()
            .percent,
        4.0
    );
}

#[test]
fn a_file_past_its_bound_is_an_error() {
    let dir = folder("codex-big");
    let path = dir.join("rollout-g.jsonl.zst");
    fs::File::create(&path)
        .unwrap()
        .set_len(PACKED + 1)
        .unwrap();
    let error = codex::read(&path).unwrap_err();
    assert!(format!("{error:#}").contains("past"), "{error:#}");
}

#[test]
fn the_newest_file_wins_by_time_not_name() {
    let sessions = folder("codex-newest");
    let at = |secs: u64| SystemTime::UNIX_EPOCH + Duration::from_secs(secs);
    let put = |day: &str, name: &str, secs: u64| {
        let dir = sessions.join(day);
        fs::create_dir_all(&dir).unwrap();
        let file = fs::File::create(dir.join(name)).unwrap();
        file.set_modified(at(secs)).unwrap();
    };
    put("2026/10/22", "rollout-late.jsonl.zst", 3_000);
    put("2026/10/23", "rollout-early.jsonl.zst", 2_000);
    put("2026/10/23", "notes.txt", 9_000);
    put("2026/10/20", "rollout-old.jsonl", 8_000);
    put("2026/09/30", "rollout-older.jsonl", 9_500);
    let found = codex::newest(&sessions).unwrap().unwrap();
    assert_eq!(found.file_name().unwrap(), "rollout-old.jsonl");
    assert_eq!(codex::newest(&sessions.join("none")).unwrap(), None);
}

#[test]
fn ages_and_resets_read_as_words_want() {
    assert_eq!(time::age(100, 70), Age::Now);
    assert_eq!(time::age(1_000, 700), Age::Minutes(5));
    assert!(time::age(10_000, 0).stale());
    assert_eq!(time::age(200_000, 0), Age::Days(2));
    let none = |_| None;
    assert_eq!(time::reset(10, 10, none), Reset::Passed);
    assert_eq!(
        time::reset(0, 2 * 3_600 + 13 * 60, none),
        Reset::In {
            days: 0,
            hours: 2,
            minutes: 13
        }
    );
    assert_eq!(
        time::reset(0, 90_000, none),
        Reset::In {
            days: 1,
            hours: 1,
            minutes: 0
        }
    );
    let fri = |_| {
        Some(Clock {
            weekday: 5,
            hour: 9,
            minute: 0,
        })
    };
    assert_eq!(
        time::reset(0, 90_000, fri),
        Reset::At {
            weekday: 5,
            hour: 9,
            minute: 0
        }
    );
    assert_eq!(
        time::reset(0, 600, fri),
        Reset::In {
            days: 0,
            hours: 0,
            minutes: 10
        }
    );
    assert_eq!(
        (time::span(300), time::span(10_080), time::span(90)),
        ("5h".into(), "7d".into(), "90m".into())
    );
}

#[test]
fn utc_times_parse() {
    assert_eq!(time::parse_utc("1970-01-01T00:00:00Z"), Some(0));
    assert_eq!(
        time::parse_utc("2026-10-23T07:55:30.401Z"),
        Some(1792742130)
    );
    assert_eq!(time::parse_utc("2026-10-23 07:55:30Z"), None);
    assert_eq!(time::parse_utc("2026-13-23T07:55:30Z"), None);
}
