//! Focus.js leavesLine resolves non-printable cursor keys through the current preset.
use crate::window::input::{event_input, printable};
use flea::keys::{self, Context, Frontend, Match};
use gpui::{KeyDownEvent, Modifiers};

pub(super) fn leaves_line(preset: &str, event: &KeyDownEvent, held: Modifiers) -> bool {
    let input = event_input(event, held);
    !printable(input.text)
        && matches!(
            keys::lookup(preset, Context::Listing, Frontend::Gui, input),
            Match::Action(
                "cursorDown" | "cursorUp" | "cursorFirst" | "cursorLast" | "pageDown" | "pageUp"
            )
        )
}
