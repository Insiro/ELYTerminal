use std::time::{Duration, Instant};

use serde_json::json;

use super::super::{
    api, appserver, keychain,
    live::{EVERY, LIMITED, Origin, Outcome, Schedule, Trouble},
    run::{self, Answer, Secret},
    web,
};

const OAUTH: &str = r#"{"five_hour":{"utilization":37.0,"resets_at":"2026-11-06T15:59:59.661633+00:00"},
  "seven_day":{"utilization":12.5,"resets_at":"2026-11-10T03:00:00Z"},
  "seven_day_opus":null,"extra_usage":{"is_enabled":false}}"#;

#[test]
fn claude_s_api_reads_both_windows_with_any_time_form() {
    let limits = api::parse(OAUTH.as_bytes()).expect("reads");
    let five = limits.five_hour.expect("five hours");
    assert_eq!((five.percent, five.minutes), (37.0, Some(300)));
    assert_eq!(five.resets_at, 1_793_980_799);
    let seven = limits.seven_day.expect("a week");
    assert_eq!((seven.percent, seven.resets_at), (12.5, 1_794_279_600));
    let other = api::parse(br#"{"five_hour":{"used_percentage":140,"resets_at":1793980799000}}"#)
        .expect("reads");
    let five = other.five_hour.expect("five hours");
    assert_eq!(
        (five.percent, five.resets_at),
        (100.0, 1_793_980_799),
        "clamped, ms"
    );
    assert_eq!(other.seven_day, None);
    assert!(api::parse(b"[]").is_err(), "no object");
    assert_eq!(
        api::resets(&json!("2026-11-06T17:00:00+02:00")),
        Some(1_793_977_200)
    );
}

#[test]
fn claude_s_api_status_means_what_it_says() {
    let answer = |status, retry_after, body: &str| Answer {
        status,
        retry_after,
        body: body.as_bytes().to_vec(),
    };
    let read = api::outcome(answer(200, None, OAUTH), Some("max".into()), 7);
    let Outcome::Read(fetched) = read else {
        panic!("a reading")
    };
    assert_eq!((fetched.origin, fetched.at), (Origin::Api, 7));
    assert_eq!(fetched.plan.as_deref(), Some("max"));
    for status in [401, 403] {
        assert_eq!(
            api::outcome(answer(status, None, "{}"), None, 0),
            Outcome::Trouble(Trouble::Expired)
        );
    }
    assert_eq!(
        api::outcome(answer(429, Some(90), ""), None, 0),
        Outcome::Limited(Some(90))
    );
    assert!(matches!(
        api::outcome(answer(500, None, ""), None, 0),
        Outcome::Trouble(Trouble::Failed(_))
    ));
}

#[test]
fn curl_s_status_splits_from_the_body() {
    let out = b"{\"a\":1}\n\x1eelyterminal-http:429 120".to_vec();
    let answer = run::answer(out).expect("splits");
    assert_eq!(answer.body, b"{\"a\":1}");
    assert_eq!((answer.status, answer.retry_after), (429, Some(120)));
    let plain = run::answer(b"ok\n\x1eelyterminal-http:200 ".to_vec()).expect("splits");
    assert_eq!((plain.status, plain.retry_after), (200, None));
    assert!(run::answer(b"no mark".to_vec()).is_err());
}

#[test]
fn credentials_read_without_showing_the_token() {
    let text = r#"{"claudeAiOauth":{"accessToken":"sk-ant-oat01-x","refreshToken":"r",
      "expiresAt":1793980799000,"scopes":["user:inference"],"subscriptionType":"max"}}"#;
    let found = keychain::parse(text).expect("reads").expect("a token");
    assert_eq!(found.token, Secret("sk-ant-oat01-x".into()));
    assert_eq!(found.plan.as_deref(), Some("max"));
    assert!(!found.expired(1_793_980_798_999));
    assert!(found.expired(1_793_980_799_000));
    assert!(!format!("{found:?}").contains("sk-ant"), "Debug hides it");
    assert_eq!(keychain::parse(r#"{"other":1}"#).expect("reads"), None);
    assert_eq!(
        keychain::parse(r#"{"claudeAiOauth":{"accessToken":""}}"#).expect("reads"),
        None
    );
    assert!(keychain::parse("not json").is_err());
    let file = keychain::file(std::path::Path::new("/c"));
    assert_eq!(file, std::path::Path::new("/c/.credentials.json"));
}

#[cfg(target_os = "macos")]
#[test]
fn a_config_folder_suffixes_the_keychain_item() {
    assert_eq!(
        keychain::suffixed("/tmp/claude-x"),
        "Claude Code-credentials-5d5a6afc"
    );
    assert_eq!(keychain::services(None), ["Claude Code-credentials"]);
    assert_eq!(keychain::account(Some("insiro")), "insiro");
    assert_eq!(keychain::account(Some("a b")), "claude-code-user");
    assert_eq!(keychain::account(None), "claude-code-user");
}

const RATES: &str = r#"{"rateLimits":{"limitId":"codex","primary":{"usedPercent":12,"windowDurationMins":300,"resetsAt":1793948675},
  "secondary":{"usedPercent":40,"windowDurationMins":10080,"resetsAt":1794535475},
  "credits":{"hasCredits":true,"unlimited":false,"balance":"25"},"planType":"plus"}}"#;

#[test]
fn codex_s_app_server_result_reads() {
    let result: serde_json::Value = serde_json::from_str(RATES).expect("json");
    let fetched = appserver::parse(&result, 9).expect("reads");
    assert_eq!(fetched.origin, Origin::AppServer);
    assert_eq!(fetched.plan.as_deref(), Some("plus"));
    assert_eq!(fetched.credits.as_deref(), Some("25"));
    let five = fetched.limits.five_hour.expect("five hours");
    assert_eq!((five.percent, five.resets_at), (12.0, 1_793_948_675));
    assert_eq!(fetched.limits.seven_day.expect("a week").percent, 40.0);
    assert!(appserver::parse(&json!({}), 0).is_err());
}

#[test]
fn windows_go_by_their_length_then_their_place() {
    let swapped = appserver::limits(Some((5.0, Some(10_080), 2)), Some((6.0, Some(300), 1)));
    assert_eq!(swapped.five_hour.expect("five").percent, 6.0);
    assert_eq!(swapped.seven_day.expect("week").percent, 5.0);
    let unknown = appserver::limits(Some((1.0, Some(60), 3)), None);
    let five = unknown.five_hour.expect("primary");
    assert_eq!((five.percent, five.minutes), (1.0, Some(60)));
    assert_eq!(unknown.seven_day, None);
}

#[test]
fn chatgpt_s_page_and_codex_s_auth_read() {
    let page = br#"{"plan_type":"pro","rate_limit":{"primary_window":{"used_percent":22,"limit_window_seconds":18000,"reset_at":1793948675},
      "secondary_window":{"used_percent":3,"limit_window_seconds":604800,"reset_at":1794535475}}}"#;
    let fetched = web::parse(page, 4).expect("reads");
    assert_eq!(
        (fetched.origin, fetched.plan.as_deref()),
        (Origin::Web, Some("pro"))
    );
    assert_eq!(fetched.limits.five_hour.expect("five").percent, 22.0);
    assert_eq!(
        fetched.limits.seven_day.expect("week").minutes,
        Some(10_080)
    );
    assert!(web::parse(br#"{"rate_limit":{}}"#, 0).is_err(), "no plan");
    let auth = r#"{"auth_mode":"chatgpt","tokens":{"access_token":"eyJ.x","account_id":"acc-1"}}"#;
    let auth = web::auth(auth).expect("reads").expect("a token");
    assert_eq!(auth.account.as_deref(), Some("acc-1"));
    assert!(!format!("{auth:?}").contains("eyJ"));
    assert_eq!(web::auth(r#"{"tokens":{}}"#).expect("reads"), None);
}

#[test]
fn a_watch_fetches_at_once_then_every_five_minutes() {
    let start = Instant::now();
    let mut schedule = Schedule::default();
    assert!(!schedule.due(false, start), "unwatched");
    assert!(schedule.due(true, start), "a watch starts");
    assert!(!schedule.due(true, start), "one at a time");
    let read = Outcome::Trouble(Trouble::NoToken);
    schedule.done(&read, start);
    assert!(!schedule.due(true, start + Duration::from_secs(60)));
    assert!(schedule.due(true, start + EVERY));
    schedule.done(&read, start + EVERY);
    assert!(!schedule.due(false, start + EVERY));
    assert!(
        schedule.due(true, start + EVERY),
        "a new watch fetches at once"
    );
}

#[test]
fn a_rate_limit_holds_even_a_new_watch() {
    let start = Instant::now();
    let mut schedule = Schedule::default();
    assert!(schedule.due(true, start));
    schedule.done(&Outcome::Limited(Some(10)), start);
    assert!(!schedule.due(false, start));
    assert!(
        !schedule.due(true, start + EVERY - Duration::from_secs(1)),
        "at least 5 min"
    );
    assert!(schedule.due(true, start + EVERY));
    schedule.done(&Outcome::Limited(None), start + EVERY);
    assert!(!schedule.due(true, start + EVERY + LIMITED - Duration::from_secs(1)));
    assert!(schedule.due(true, start + EVERY + LIMITED));
    assert_eq!(Schedule::limited(Some(3_600), 100), Trouble::Limited(3_700));
    assert_eq!(Schedule::limited(None, 0), Trouble::Limited(900));
}

#[cfg(unix)]
mod server {
    use std::{os::unix::fs::PermissionsExt, path::PathBuf, time::Duration};

    use super::super::folder;
    use super::{RATES, appserver};

    fn script(name: &str, body: &str) -> PathBuf {
        let path = folder(name).join("codex");
        std::fs::write(&path, format!("#!/bin/sh\n{body}")).expect("a script");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("mode");
        path
    }

    #[test]
    fn the_app_server_answers_after_initialize_and_is_killed() {
        let rates = RATES.replace('\n', "");
        let body = format!(
            "read a\nprintf '%s\\n' 'banner' '{{\"method\":\"note\"}}' '{{\"id\":1,\"result\":{{}}}}'\nread b\nread c\n\
             case \"$b$c\" in *initialized*rateLimits/read*) printf '%s\\n' '{{\"id\":2,\"result\":{rates}}}';; esac\nsleep 30\n"
        );
        let codex = script("appserver-ok", &body);
        let started = std::time::Instant::now();
        let result = appserver::ask(&codex, &[], Duration::from_secs(5)).expect("answers");
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "killed, not waited for"
        );
        let fetched = appserver::parse(&result, 0).expect("reads");
        assert_eq!(fetched.limits.five_hour.expect("five").percent, 12.0);
    }

    #[test]
    fn a_silent_or_refusing_server_fails() {
        let silent = script("appserver-silent", "sleep 30\n");
        let error = appserver::ask(&silent, &[], Duration::from_millis(300)).unwrap_err();
        assert!(format!("{error:#}").contains("no answer to 1"), "{error:#}");
        let refusing = script(
            "appserver-refuses",
            "read a\nprintf '%s\\n' '{\"id\":1,\"error\":{\"message\":\"not signed in\"}}'\nsleep 30\n",
        );
        let error = appserver::ask(&refusing, &[], Duration::from_secs(5)).unwrap_err();
        assert!(format!("{error:#}").contains("not signed in"), "{error:#}");
        let gone = script("appserver-exits", "echo broken >&2\nexit 3\n");
        let error = appserver::ask(&gone, &[], Duration::from_secs(5)).unwrap_err();
        assert!(format!("{error:#}").contains("exited"), "{error:#}");
    }
}

/// One real, read-only fetch of each service; prints percents, resets, plans and troubles, never a token.
#[test]
#[ignore = "network and the user's own sign-ins"]
fn live_fetches() {
    for (name, outcome) in [("claude", api::fetch()), ("codex", appserver::fetch())] {
        match outcome {
            Outcome::Read(fetched) => println!(
                "{name}: {:?} 5h {:?} 7d {:?} plan {:?} credits {:?}",
                fetched.origin,
                fetched.limits.five_hour.map(|w| (w.percent, w.resets_at)),
                fetched.limits.seven_day.map(|w| (w.percent, w.resets_at)),
                fetched.plan,
                fetched.credits,
            ),
            other => println!("{name}: {other:?}"),
        }
    }
}
