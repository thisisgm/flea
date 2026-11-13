//! The passphrase field's model: typed bytes held as a secret, and a mask drawn from a count alone.
use super::secret::Secret;

#[derive(Default)]
pub struct Field {
    pub(super) text: Secret,
    pub(crate) entry: crate::path::entry::Entry,
    /// The submitted length, kept so the dots draw after the bytes have gone to the child.
    pub held_mask: usize,
    pub error: bool,
    /// The eye is held: the entry carries the real secret instead of its mask.
    pub revealed: bool,
}

impl Field {
    pub fn type_text(&mut self, text: &str) {
        self.conceal();
        self.held_mask = 0;
        if !self.entry.editing {
            self.begin();
        }
        super::private_entry::replace(&mut self.entry, &mut self.text, None, text, false);
    }

    pub fn backspace(&mut self) {
        self.conceal();
        // Held dots after a refused try stand for bytes already handed to udisks, so Backspace clears them whole.
        if self.text.is_empty() && self.held_mask > 0 {
            self.held_mask = 0;
            return;
        }
        if !self.entry.editing {
            self.begin();
        }
        super::private_entry::erase(&mut self.entry, &mut self.text, false);
    }

    pub fn clear(&mut self) {
        self.conceal();
        self.entry.close();
        self.text.clear();
        self.held_mask = 0;
    }

    /// Moves the typed bytes out, keeping only their length for the dots.
    pub fn take(&mut self) -> Secret {
        self.conceal();
        self.entry.close();
        self.held_mask = self.text.chars();
        self.text.take()
    }

    /// Shows the typed secret while the eye is held; the mask has the secret's byte length, so every offset holds.
    pub fn reveal(&mut self) {
        if self.revealed || !self.entry.editing {
            return;
        }
        self.revealed = true;
        self.entry.text = std::str::from_utf8(self.text.expose())
            .unwrap_or("")
            .to_string();
        self.entry.revision += 1;
    }

    /// Draws the mask again, zeroing the buffer that held the secret.
    pub fn conceal(&mut self) {
        if !self.revealed {
            return;
        }
        self.revealed = false;
        std::mem::take(&mut self.entry.text).into_bytes().fill(0);
        self.entry.text =
            super::private_entry::shadow(std::str::from_utf8(self.text.expose()).unwrap_or(""));
        self.entry.revision += 1;
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

impl Field {
    pub(crate) fn begin(&mut self) {
        super::private_entry::begin(&mut self.entry, &self.text);
        self.entry.move_to(self.entry.text.len());
    }
}

impl std::fmt::Debug for Field {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Field")
            .field("text", &self.text)
            .field("held_mask", &self.held_mask)
            .field("error", &self.error)
            .finish()
    }
}
