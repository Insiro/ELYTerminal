//! A terminal's last screen as plain text, and whether a command runs, for a tab saved for later.

use alacritty_terminal::{
    grid::Dimensions,
    index::{Column, Line, Point},
    term::Term,
};

use super::view::TerminalView;

/// The history lines kept above the screen.
const HISTORY: usize = 200;
/// The most bytes kept, the last ones.
pub const CAP: usize = 64 * 1024;

/// The screen and up to `history` lines above it, blank ends trimmed, cut to the last `cap` bytes.
fn text<T>(term: &Term<T>, history: usize, cap: usize) -> String {
    let above = history.min(term.grid().history_size()) as i32;
    let start = Point::new(Line(-above), Column(0));
    let end = Point::new(term.bottommost_line(), term.last_column());
    let all = term.bounds_to_string(start, end);
    let lines: Vec<&str> = all.lines().map(str::trim_end).collect();
    let text = lines.join("\n");
    let text = text.trim_matches('\n');
    if text.len() <= cap {
        return text.to_string();
    }
    let mut at = text.len() - cap;
    while !text.is_char_boundary(at) {
        at += 1;
    }
    text[at..].to_string()
}

impl TerminalView {
    /// What the screen and its last history lines show, as plain text.
    pub fn screen_text(&self) -> String {
        text(&*self.pty.term.lock(), HISTORY, CAP)
    }

    /// The shell runs a command, as OSC 133 C without its D says; a shell without marks runs none.
    pub fn busy(&self) -> bool {
        self.ran.is_some()
    }
}

#[cfg(test)]
mod tests {
    use alacritty_terminal::{
        event::VoidListener,
        term::{Config, test::TermSize},
        vte::ansi::Processor,
    };

    use super::*;

    fn fed(bytes: &[u8], lines: usize) -> Term<VoidListener> {
        let mut term = Term::new(Config::default(), &TermSize::new(10, lines), VoidListener);
        let mut parser: Processor = Processor::new();
        parser.advance(&mut term, bytes);
        term
    }

    #[test]
    fn the_screen_and_its_history_read_as_lines() {
        let term = fed(b"one\r\ntwo  \r\nthree\r\nfour\r\n", 2);
        assert_eq!(text(&term, 200, CAP), "one\ntwo\nthree\nfour");
        assert_eq!(text(&term, 1, CAP), "three\nfour", "one history line above");
        assert_eq!(text(&fed(b"", 3), 200, CAP), "");
    }

    #[test]
    fn a_long_text_keeps_its_last_bytes_whole() {
        let term = fed("가나다라\r\n".repeat(10).as_bytes(), 20);
        let cut = text(&term, 200, 10);
        assert_eq!(cut, "나다라", "cut at a character's start");
    }
}
