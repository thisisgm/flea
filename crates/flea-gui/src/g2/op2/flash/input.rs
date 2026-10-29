//! Flash keyboard focus and step activation.
use super::*;

impl Flash {
    /// After Eject no button is drawn, so the keyboard rests on the pane itself.
    pub fn park(&self) -> Seat {
        if !self.shown || self.ejected {
            return Seat::Pane;
        }
        if self.cursor == self.flash_row() {
            if self.running() {
                return Seat::Cancel;
            }
            if let Some(ending) = &self.runner.ending {
                if ending.eject || ending.flash_back {
                    return Seat::End;
                }
                return Seat::Pane;
            }
            return Seat::Flash;
        }
        if self.cursor == 0 {
            return Seat::Image;
        }
        Seat::Drive
    }

    /// No drawn button owns Enter or Space, so the scope presses where the cursor parks.
    pub fn key(&mut self, key: super::super::keys::Key) -> Vec<Effect> {
        use super::super::keys::Key;
        match key {
            Key::Escape | Key::Backspace => self.close(),
            Key::Up => {
                self.move_cursor(-1);
                Vec::new()
            }
            Key::Down => {
                self.move_cursor(1);
                Vec::new()
            }
            // No drawn button owns Enter or Space, so the scope presses where the cursor parks.
            Key::Enter | Key::Space => self.context_press(),
            _ => Vec::new(),
        }
    }
}
