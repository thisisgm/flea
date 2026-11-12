//! The verb a held drag's cue shows: a shelf drag's fixed verb, else the source keys over the lift's flags.
use super::*;

impl Cue {
    pub(super) fn verb(&self, window: &Window) -> Verb {
        if let Some(verb) = self.fixed {
            return verb;
        }
        let keys = window.modifiers();
        let (control, shift_held) = self.source_keys.unwrap_or((keys.control, keys.shift));
        let seen = |held| {
            if held {
                Seen::Held
            } else if self.own || self.source_keys.is_some() {
                Seen::Released
            } else {
                Seen::Unseen
            }
        };
        let facts = self.facts.unwrap_or_default();
        if let Some((copy, shift)) = self.flags {
            core::verb_for(
                drop_held(copy, seen(control)),
                drop_held(shift, seen(shift_held)),
                facts.src_dev,
                facts.dest_dev,
                facts.deletable,
            )
        } else if control && shift_held {
            Verb::Link
        } else if control {
            Verb::Copy
        } else if shift_held {
            Verb::Move
        } else {
            core::drop_verb(
                None,
                self.proposed,
                facts.dest_dev,
                Seen::Unseen,
                Seen::Unseen,
            )
        }
    }
}

impl Shell {
    pub(in crate::g2::drag_drop::join) fn file_drag_modifiers(
        &mut self,
        modifiers: gpui::Modifiers,
        cx: &mut Context<Self>,
    ) {
        if let Some((_, cue)) = cx.default_global::<Drags>().cue.as_mut() {
            cue.source_keys = Some((modifiers.control, modifiers.shift));
        }
    }
}
