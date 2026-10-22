//! The passphrase field's model: typed bytes held as a secret, and a mask drawn from a count alone.
use super::secret::Secret;

#[derive(Debug, Default)]
pub struct Field {
    text: Secret,
    /// The submitted length, kept so the dots draw after the bytes have gone to the child.
    pub held_mask: usize,
    pub error: bool,
}

impl Field {
    pub fn type_text(&mut self, text: &str) {
        self.held_mask = 0;
        self.text.push_str(text);
    }

    pub fn backspace(&mut self) {
        self.text.pop();
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.held_mask = 0;
    }

    /// Moves the typed bytes out, keeping only their length for the dots.
    pub fn take(&mut self) -> Secret {
        self.held_mask = self.text.chars();
        self.text.take()
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// How many dots to draw.
    pub fn dots(&self) -> usize {
        if self.text.is_empty() {
            self.held_mask
        } else {
            self.text.chars()
        }
    }

    pub fn masked(&self) -> bool {
        !self.text.is_empty() || self.held_mask > 0
    }
}
