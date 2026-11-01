//! The verb a held drag's cue shows: a shelf drag's fixed verb, else the held keys over the lift's flags.
use super::*;

impl Cue {
    pub(super) fn verb(&self, window: &Window) -> Verb {
        if let Some(verb) = self.fixed {
            return verb;
        }
        let keys = window.modifiers();
        let seen = |held| {
            if held {
                Seen::Held
            } else if self.own {
                Seen::Released
            } else {
                Seen::Unseen
            }
        };
        if let Some((copy, shift)) = self.flags {
            core::verb_for(
                drop_held(copy, seen(keys.control)),
                drop_held(shift, seen(keys.shift)),
                self.facts.src_dev,
                self.facts.dest_dev,
                self.facts.deletable,
            )
        } else if keys.control && keys.shift {
            Verb::Link
        } else if keys.control {
            Verb::Copy
        } else if keys.shift {
            Verb::Move
        } else {
            core::drop_verb(
                None,
                self.proposed,
                self.facts.dest_dev,
                Seen::Unseen,
                Seen::Unseen,
            )
        }
    }
}
