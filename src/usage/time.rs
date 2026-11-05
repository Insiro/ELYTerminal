//! A reading's age and a window's reset, in words' parts.

/// How old a reading is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Age {
    /// Under a minute.
    Now,
    Minutes(i64),
    Hours(i64),
    Days(i64),
}

impl Age {
    /// Over an hour: shown muted.
    pub fn stale(self) -> bool {
        matches!(self, Self::Hours(_) | Self::Days(_))
    }
}

pub fn age(now: i64, at: i64) -> Age {
    let secs = (now - at).max(0);
    match secs {
        0..60 => Age::Now,
        60..3_600 => Age::Minutes(secs / 60),
        3_600..86_400 => Age::Hours(secs / 3_600),
        _ => Age::Days(secs / 86_400),
    }
}

/// When a window starts again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reset {
    /// Its time passed, and no new reading came.
    Passed,
    /// Within a day, or where no local clock reads.
    In { days: i64, hours: i64, minutes: i64 },
    /// A day or more away: weekday 0 for Sunday, and the local time.
    At { weekday: u8, hour: u8, minute: u8 },
}

pub fn reset(now: i64, at: i64, local: impl Fn(i64) -> Option<Clock>) -> Reset {
    let secs = at - now;
    if secs <= 0 {
        return Reset::Passed;
    }
    if secs >= 86_400
        && let Some(clock) = local(at)
    {
        return Reset::At {
            weekday: clock.weekday,
            hour: clock.hour,
            minute: clock.minute,
        };
    }
    let minutes = (secs + 59) / 60;
    Reset::In {
        days: minutes / 1_440,
        hours: minutes % 1_440 / 60,
        minutes: minutes % 60,
    }
}

/// A moment on the local clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clock {
    pub weekday: u8,
    pub hour: u8,
    pub minute: u8,
}

/// `secs` on this machine's clock; none where it does not read.
#[cfg(unix)]
pub fn local(secs: i64) -> Option<Clock> {
    let time = libc::time_t::try_from(secs).ok()?;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: both pointers are valid for the call; localtime_r keeps neither.
    let done = unsafe { libc::localtime_r(&time, &mut tm) };
    (!done.is_null()).then_some(Clock {
        weekday: tm.tm_wday as u8,
        hour: tm.tm_hour as u8,
        minute: tm.tm_min as u8,
    })
}

#[cfg(not(unix))]
pub fn local(_: i64) -> Option<Clock> {
    None
}

/// Unix seconds of an RFC 3339 time in UTC, `2026-10-23T07:55:30.401Z`.
pub fn parse_utc(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    if bytes.len() < 20 || !text.ends_with('Z') {
        return None;
    }
    let number = |from: usize, to: usize| -> Option<i64> {
        let part = text.get(from..to)?;
        part.bytes()
            .all(|b| b.is_ascii_digit())
            .then(|| part.parse().ok())?
    };
    let marks = [(4, b'-'), (7, b'-'), (10, b'T'), (13, b':'), (16, b':')];
    if marks.iter().any(|&(at, mark)| bytes[at] != mark) {
        return None;
    }
    let (year, month, day) = (number(0, 4)?, number(5, 7)?, number(8, 10)?);
    let (hour, minute, second) = (number(11, 13)?, number(14, 16)?, number(17, 19)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 {
        return None;
    }
    Some(days(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second)
}

/// Days from 1970-01-01 to a civil date, Howard Hinnant's way.
fn days(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let of_era = year - era * 400;
    let of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let of_cycle = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
    era * 146_097 + of_cycle - 719_468
}

/// A window's length as the bars name it: `5h`, `7d`, `90m`.
pub fn span(minutes: u32) -> String {
    match minutes {
        m if m % 1_440 == 0 => format!("{}d", m / 1_440),
        m if m % 60 == 0 => format!("{}h", m / 60),
        m => format!("{m}m"),
    }
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() as i64)
}
