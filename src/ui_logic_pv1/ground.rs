// Quick Look's ground (#331): every pointer event over the overlay ends there, and none reaches the window behind it.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    Middle,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pointer {
    Press { button: Button, clicks: usize },
    Release { button: Button },
    Move,
    Wheel,
    Pinch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    // Consumed and nothing else happens.
    Swallow,
    // Consumed; the unrevealed card reveals at once.
    Reveal,
    // Consumed; a pointer move reveals the media strip.
    RevealStrip,
    // Consumed; a left click outside the card closes it (rule of 2026-09-11).
    Close,
}

// One overlay's pointer state: a press taken by the hold shield keeps its release, and a close needs its own press.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ground {
    shield_held: bool,
    close_armed: bool,
}

impl Ground {
    // inside: the pointer is over the card's surface; revealed: the card has drawn.
    pub fn handle(&mut self, event: Pointer, inside: bool, revealed: bool) -> Effect {
        if !revealed || self.shield_held {
            // During the hold a press or the wheel reveals at once and reaches nothing unseen; its release clicks nothing.
            return match event {
                Pointer::Press { .. } => {
                    self.shield_held = true;
                    self.close_armed = false;
                    Effect::Reveal
                }
                Pointer::Release { .. } => {
                    self.shield_held = false;
                    Effect::Swallow
                }
                Pointer::Wheel if !revealed => Effect::Reveal,
                _ => Effect::Swallow,
            };
        }
        match event {
            Pointer::Press { button: Button::Left, clicks } => {
                self.close_armed = !inside && clicks == 1;
                Effect::Swallow
            }
            Pointer::Press { .. } => Effect::Swallow,
            Pointer::Release { button: Button::Left } => {
                let close = std::mem::take(&mut self.close_armed) && !inside;
                if close { Effect::Close } else { Effect::Swallow }
            }
            Pointer::Release { .. } | Pointer::Wheel | Pointer::Pinch => Effect::Swallow,
            Pointer::Move => Effect::RevealStrip,
        }
    }

    // A closed overlay keeps no shield: the first click after it reaches the listing again.
    pub fn reset(&mut self) { *self = Self::default(); }
}
