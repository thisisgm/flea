//! A subject whose bytes are not a real ODF or OOXML container: unsupported, with no conversion asked.
use super::{Office, Reason, State, NOT_OFFICE};

impl Office {
    /// A subject whose bytes are not a real ODF or OOXML container: unsupported, and no conversion is ever asked.
    pub fn reject_container(&mut self) {
        if self.state == State::Converting {
            self.id = 0;
            self.pending = None;
            self.diagnostic = NOT_OFFICE.to_string();
            self.state = State::Unsupported(Reason::NotOffice);
        }
    }
}

#[cfg(test)]
#[path = "office_container_tests.rs"]
mod tests;
