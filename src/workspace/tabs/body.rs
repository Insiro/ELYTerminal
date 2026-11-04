//! What a tab shows.

use super::Tab;
use crate::sessions::Session;

pub enum Body {
    Term(Session),
}

impl Tab {
    /// The tab's terminal session, when it shows one.
    pub fn term(&self) -> Option<&Session> {
        match &self.body {
            Body::Term(session) => Some(session),
        }
    }

    pub fn term_mut(&mut self) -> Option<&mut Session> {
        match &mut self.body {
            Body::Term(session) => Some(session),
        }
    }
}
