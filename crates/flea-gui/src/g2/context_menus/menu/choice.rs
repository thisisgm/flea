//! Live choice validation and the flyout leaf and tag shortcuts.
use super::*;

impl Menu {
    fn moved(&self, identity_now: &str) -> bool {
        self.face.holds_selection() && identity_now != self.identity
    }

    fn fire(
        &mut self,
        live: Option<&[Entry]>,
        action: String,
        leaf: String,
        identity_now: &str,
    ) -> Outcome {
        let rows = live.map_or_else(
            || {
                self.card
                    .as_ref()
                    .map(|c| c.entries.clone())
                    .unwrap_or_default()
            },
            <[Entry]>::to_vec,
        );
        let checked = refresh::validate_choice(&rows, &action, &leaf, self.moved(identity_now));
        // A Tags choice keeps the menu and its flyout up for the answer, as v0.3.12 ContextMenu.qml does; a refusal still closes.
        if action != "tags" || checked.is_err() {
            self.close();
        }
        match checked {
            Ok(()) => Outcome::Fire { action, leaf },
            Err(refusal) => Outcome::Refused(refusal.key()),
        }
    }

    pub(super) fn choose(
        &mut self,
        live: Option<&[Entry]>,
        identity_now: &str,
        env: &Env,
        window: &Window,
    ) -> Outcome {
        if let Some(flyout) = &self.flyout {
            let Some(leaf) = flyout
                .card
                .cursor
                .map(|at| flyout.card.entries[at].id.to_string())
            else {
                return Outcome::Nothing;
            };
            return self.choose_leaf(&leaf, live, identity_now);
        }
        let Some(card) = &self.card else {
            return Outcome::Nothing;
        };
        let Some(row) = card.cursor else {
            return Outcome::Nothing;
        };
        if card.entries[row].has_submenu() {
            return self.enter_row(env, window);
        }
        let action = card.entries[row].action.to_string();
        self.fire(live, action, String::new(), identity_now)
    }

    pub(super) fn choose_leaf(
        &mut self,
        leaf: &str,
        live: Option<&[Entry]>,
        identity_now: &str,
    ) -> Outcome {
        let Some(flyout) = &self.flyout else {
            return Outcome::Nothing;
        };
        let action = flyout.action.clone();
        if flyout.parent.is_some() {
            return self.fire(live, action, leaf.to_owned(), identity_now);
        }
        let for_rail = matches!(self.face, Face::Rail | Face::Place);
        let lone = rows::lone_choice(
            &action,
            leaf,
            for_rail,
            self.face == Face::Header,
            self.face.holds_selection(),
            &self.identity,
            identity_now,
        );
        self.close();
        match lone {
            Lone::Moved => Outcome::Refused(Refusal::Moved.key()),
            Lone::Unknown => Outcome::Refused(Refusal::Unavailable.key()),
            Lone::Fire(_) => Outcome::Fire {
                action,
                leaf: leaf.to_owned(),
            },
        }
    }

    // A flyout leaf's own letter, or a tag digit for the Tags flyout.
    pub(super) fn letter(
        &mut self,
        action: &str,
        live: Option<&[Entry]>,
        identity_now: &str,
    ) -> Outcome {
        if let Some(flyout) = &self.flyout {
            if flyout
                .card
                .entries
                .iter()
                .any(|leaf| leaf.id == action && leaf.steppable())
            {
                return self.choose_leaf(action, live, identity_now);
            }
        }
        let Some(slot) = action
            .strip_prefix("tag")
            .and_then(|n| n.parse::<u8>().ok())
        else {
            return Outcome::Nothing;
        };
        let tags = self.card.as_ref().and_then(|card| {
            card.entries
                .iter()
                .find(|e| e.action == "tags" && !e.disabled)
        });
        let Some(leaf) = tags
            .and_then(|row| row.submenu.as_ref())
            .and_then(|leaves| leaves.iter().find(|l| l.tag == Some(slot)))
        else {
            return Outcome::Nothing;
        };
        let leaf = leaf.id.to_string();
        self.fire(live, "tags".into(), leaf, identity_now)
    }
}
