//! Agent usage: Claude's limits through its statusLine, Codex's from its session files.

pub mod claude;
pub mod codex;
#[cfg(test)]
mod tests;
pub mod time;

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
