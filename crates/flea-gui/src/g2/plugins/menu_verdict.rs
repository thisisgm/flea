use crate::state::Shell;
use flea::{jsondoc::Json, ui_logic_status_feedback::status::Hint};
use gpui::Context;
use std::collections::{HashMap, HashSet};

#[derive(Hash, PartialEq, Eq)]
pub(super) struct Owner {
    side: usize,
    plugin: String,
    action: String,
}

#[derive(Default)]
pub(super) struct Attempts(HashMap<Owner, Attempt>);

#[derive(Default)]
struct Attempt {
    latest: u64,
    pending: usize,
}

impl Attempts {
    fn sent(&mut self, owner: Owner, id: u64) {
        let attempt = self.0.entry(owner).or_default();
        attempt.latest = id;
        attempt.pending += 1;
    }

    // Keep the latest id until every overlapping reply ends, including older replies arriving last.
    fn response(&mut self, owner: &Owner, id: u64, terminal: bool) -> bool {
        let Some(attempt) = self.0.get_mut(owner) else {
            return true;
        };
        let current = attempt.latest == id;
        if terminal {
            attempt.pending -= 1;
            if attempt.pending == 0 {
                self.0.remove(owner);
            }
        }
        current
    }
}

// Sample input: {"op":"run","action":"sha256"} identifies one plugin action's retry.
pub(super) fn owner(side: usize, plugin: &str, request: &Json) -> Option<Owner> {
    (request.get("op").and_then(Json::as_str) == Some("run")).then_some(())?;
    let action = request
        .get("action")
        .and_then(Json::as_str)
        .filter(|s| !s.is_empty())?;
    Some(Owner {
        side,
        plugin: plugin.into(),
        action: action.into(),
    })
}

impl Shell {
    fn prune_plugin_errors(&mut self, cx: &mut Context<Self>) {
        let live: HashSet<_> = self.status_messages(cx, |messages, _| {
            messages.errors().iter().map(|entry| entry.serial).collect()
        });
        self.menu.plugin_runtime.errors.retain(|_, tokens| {
            tokens.retain(|token| live.contains(token));
            !tokens.is_empty()
        });
    }

    // An explicit retry acknowledges only that action's earlier verdicts, after its request was accepted.
    pub(super) fn plugin_action_sent(
        &mut self,
        owner: Option<Owner>,
        id: u64,
        cx: &mut Context<Self>,
    ) {
        let Some(owner) = owner else {
            return;
        };
        self.prune_plugin_errors(cx);
        if let Some(tokens) = self.menu.plugin_runtime.errors.remove(&owner) {
            self.status_messages(cx, |messages, now| {
                for token in tokens {
                    messages.forget_error(token, now);
                }
            });
        }
        self.menu.plugin_runtime.attempts.sent(owner, id);
    }

    pub(super) fn plugin_action_response(
        &mut self,
        side: usize,
        response: &super::super::Response,
        terminal: bool,
    ) -> bool {
        let Some(owner) = owner(side, &response.plugin, &response.request) else {
            return true;
        };
        self.menu
            .plugin_runtime
            .attempts
            .response(&owner, response.id, terminal)
    }

    pub(super) fn plugin_response_notice(
        &mut self,
        side: usize,
        response: &super::super::Response,
        sentence: &str,
        cx: &mut Context<Self>,
    ) {
        let owner = owner(side, &response.plugin, &response.request);
        if let (false, Some(owner)) = (response.ok, owner) {
            if sentence.is_empty() {
                return;
            }
            self.prune_plugin_errors(cx);
            if let Some(pane) = self.panes[side].as_mut() {
                pane.status = sentence.to_owned().into();
            }
            let token = self.status_messages(cx, |messages, now| {
                messages.say_tracked_error(sentence, now)
            });
            if let Some(token) = token {
                self.menu
                    .plugin_runtime
                    .errors
                    .entry(owner)
                    .or_default()
                    .push(token);
            }
            cx.notify();
        } else {
            self.owner_notice(side, sentence, !response.ok, Hint::None, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_attempts_keep_latest_until_both_terminal_replies_then_release_state() {
        let owner = || Owner {
            side: 0,
            plugin: "example.checksums".into(),
            action: "sha256".into(),
        };
        for order in [[101, 102], [102, 101]] {
            let mut attempts = Attempts::default();
            attempts.sent(owner(), 101);
            attempts.sent(owner(), 102);
            assert!(!attempts.response(&owner(), 101, false));
            assert!(attempts.response(&owner(), 102, false));
            assert_eq!(attempts.response(&owner(), order[0], true), order[0] == 102);
            assert_eq!(attempts.0.len(), 1);
            assert_eq!(attempts.response(&owner(), order[1], true), order[1] == 102);
            assert!(attempts.0.is_empty());
        }
    }
}
