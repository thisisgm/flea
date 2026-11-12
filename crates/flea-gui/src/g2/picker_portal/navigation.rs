//! Back, Forward and Parent share the chooser's folder owner and confirmed destinations.
use super::Picker;
use gpui::Context;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Travel {
    #[default]
    Open,
    Back,
    Forward,
}

#[derive(Default)]
pub(super) struct Navigation {
    current: String,
    pending: Option<Attempt>,
}

struct Attempt {
    path: String,
    travel: Travel,
    pushed_source: bool,
    // Confirmed folders a run of Back presses popped on its way past, oldest press first.
    passed: Vec<String>,
}

impl Picker {
    pub fn can_go_up(&self) -> bool {
        !self.backend_unavailable()
            && !self.accept_state().submitting()
            && self.listing.location.starts_with('/')
            && self.listing.location != "/"
    }

    pub fn go_up(&mut self, cx: &mut Context<Self>) {
        if !self.can_go_up() {
            return;
        }
        let parent = flea::ui_logic_picker_listing::parent_of(&self.listing.location).to_owned();
        self.open_folder(&parent, cx);
    }

    pub fn go_back(&mut self, cx: &mut Context<Self>) {
        if self.backend_unavailable() || self.accept_state().submitting() {
            return;
        }
        let returning = self
            .chrome
            .navigation
            .pending
            .as_ref()
            .is_some_and(|attempt| attempt.travel == Travel::Back);
        if !returning
            && !self.chrome.navigation.current.is_empty()
            && self.listing.location != self.chrome.navigation.current
        {
            let path = self.chrome.navigation.current.clone();
            self.open_folder_travel(&path, Travel::Open, cx);
        } else if let Some(path) = self.chrome.history.last().cloned() {
            self.open_folder_travel(&path, Travel::Back, cx);
        }
    }

    pub(super) fn can_go_back(&self) -> bool {
        !self.chrome.history.is_empty()
            || (!self.chrome.navigation.current.is_empty()
                && self.listing.location != self.chrome.navigation.current
                && !self
                    .chrome
                    .navigation
                    .pending
                    .as_ref()
                    .is_some_and(|attempt| attempt.travel == Travel::Back))
    }

    pub fn go_forward(&mut self, cx: &mut Context<Self>) {
        if self.backend_unavailable()
            || self.accept_state().submitting()
            || self.browse.listing.read(cx).listing().is_pending()
            || !self.browse.route.asked.is_empty()
        {
            return;
        }
        if let Some(path) = self.chrome.ahead.last().cloned() {
            self.open_folder_travel(&path, Travel::Forward, cx);
        }
    }

    pub(super) fn remember_folder(&mut self, path: &str, travel: Travel) {
        // A watch refresh of an outstanding destination keeps that destination's travel direction.
        if travel == Travel::Open
            && path == self.listing.location
            && self.chrome.navigation.pending.is_some()
        {
            return;
        }
        let continuing = travel == Travel::Back
            && self
                .chrome
                .navigation
                .pending
                .as_ref()
                .is_some_and(|attempt| attempt.travel == Travel::Back);
        let earlier = if continuing {
            self.chrome.navigation.pending.take()
        } else {
            self.navigation_failed();
            None
        };
        let mut pushed_source = earlier
            .as_ref()
            .is_some_and(|attempt| attempt.pushed_source);
        let passed = earlier.map_or_else(Vec::new, |attempt| {
            let mut passed = attempt.passed;
            passed.push(attempt.path);
            passed
        });
        let source = &self.chrome.navigation.current;
        if source != path || travel != Travel::Open {
            match travel {
                Travel::Open | Travel::Forward => {
                    if travel == Travel::Forward {
                        self.chrome.ahead.pop();
                    }
                    if !source.is_empty() {
                        self.chrome.history.push(source.clone());
                        pushed_source = true;
                    }
                }
                Travel::Back => {
                    self.chrome.history.pop();
                    if !continuing && !source.is_empty() {
                        self.chrome.ahead.push(source.clone());
                        pushed_source = true;
                    }
                }
            }
        }
        self.chrome.navigation.pending = Some(Attempt {
            path: path.to_owned(),
            travel,
            pushed_source,
            passed,
        });
    }

    // Undo only this attempt; a failed or replaced destination never becomes a history source.
    pub(super) fn navigation_failed(&mut self) {
        let Some(attempt) = self.chrome.navigation.pending.take() else {
            return;
        };
        if attempt.pushed_source {
            match attempt.travel {
                Travel::Back => {
                    self.chrome.ahead.pop();
                }
                Travel::Open | Travel::Forward => {
                    self.chrome.history.pop();
                }
            }
        }
        match attempt.travel {
            Travel::Back => {
                self.chrome.history.push(attempt.path);
                self.chrome.history.extend(attempt.passed.into_iter().rev());
            }
            Travel::Forward => self.chrome.ahead.push(attempt.path),
            Travel::Open => {}
        }
    }

    // The departure already changed the trails; success confirms the destination and ends an ordinary Forward trail.
    pub(super) fn navigation_landed(&mut self) {
        let navigation = &mut self.chrome.navigation;
        let Some(attempt) = navigation.pending.take() else {
            return;
        };
        if attempt.path != self.listing.location {
            return;
        }
        navigation.current = attempt.path;
        if attempt.travel == Travel::Open && attempt.pushed_source {
            self.chrome.ahead.clear();
        }
    }
}
