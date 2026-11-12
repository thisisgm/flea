//! Dual lists are consecutive keyboard stops; optional stops belong to mounted surfaces.
use crate::{state::Shell, window::startup::View};
use gpui::{Context, FocusHandle, Window};

pub struct Focus {
    pub last_side: usize,
    /// The single view Dual was entered from, where the Dual key returns; List when the window opened in Dual.
    pub before: View,
    pub rail: FocusHandle,
    pub chrome: FocusHandle,
    pub strip: FocusHandle,
}

impl Focus {
    pub fn new(cx: &mut Context<Shell>) -> Self {
        Self {
            last_side: 0,
            before: View::List,
            rail: cx.focus_handle(),
            chrome: cx.focus_handle(),
            strip: cx.focus_handle(),
        }
    }
}

impl Shell {
    pub fn focus_view(&self, window: &Window, cx: &gpui::App) -> &'static str {
        if let Some(stop) = self.input.current(window) {
            stop.name
        } else if self.dual_focus.rail.contains_focused(window, cx) {
            "rail"
        } else if self.dual_focus.chrome.contains_focused(window, cx) {
            "chrome"
        } else if self.dual_focus.strip.contains_focused(window, cx) {
            "strip"
        } else if self.focus.is_focused(window) {
            "list"
        } else {
            ""
        }
    }

    pub(crate) fn dual_tab(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.view != View::Dual {
            return false;
        }
        let current = match self.focus_view(window, cx) {
            "list" => self.focus_side,
            "rail" => 2,
            "chrome" => 3,
            "strip" => 4,
            "reload-left" => 5,
            "reload-right" => 6,
            _ => return false,
        };
        self.dual_walk(current, forward, window, cx);
        true
    }

    /// One step of the Dual ring from stop `current`: 0 and 1 the lists, 2 the rail, 3 the chrome, 4 the tab strip.
    pub(crate) fn dual_walk(
        &mut self,
        current: usize,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let places = self
            .launch
            .settings
            .data
            .get("places")
            .unwrap_or(&flea::jsondoc::Json::Null);
        let shown = [
            true,
            self.panes[1].is_some(),
            self.slots.rail.is_some()
                && crate::rail::Mode::from_places(places) != crate::rail::Mode::Hidden,
            self.slots.chrome.is_some(),
            self.slots.tabs.is_some() && self.geometry(window).tabs.height > 0.,
            self.state_reload_focus(0, cx).is_some(),
            self.state_reload_focus(1, cx).is_some(),
        ];
        let order = [0, 5, 1, 6, 2, 3, 4];
        let mut at = order.iter().position(|&stop| stop == current).unwrap_or(0);
        let mut to = current;
        for _ in 0..shown.len() {
            at = (at + if forward { 1 } else { shown.len() - 1 }) % shown.len();
            to = order[at];
            if shown[to] {
                break;
            }
        }
        self.keyboard.disarm();
        // The chrome and the strip keep the side the walk left from; the rail belongs to the left pane.
        if to <= 2 {
            self.focus_pane(usize::from(to == 1), cx);
        } else if to >= 5 {
            self.focus_pane(to - 5, cx);
        }
        match to {
            2 => window.focus(&self.dual_focus.rail, cx),
            3 => window.focus(&self.dual_focus.chrome, cx),
            4 => window.focus(&self.dual_focus.strip, cx),
            5 | 6 => {
                if let Some(focus) = self.state_reload_focus(to - 5, cx) {
                    window.focus(&focus, cx);
                }
            }
            _ => self.enter_list_stop(forward, window, cx),
        }
        if to == 3 {
            self.enter_chrome(forward, window, cx);
        } else if to == 4 {
            self.enter_strip(forward);
        }
        self.keyboard.context = match to {
            2 => flea::keys::Context::Rail,
            3 | 4 => flea::keys::Context::Panel,
            _ => flea::keys::Context::Listing,
        };
        self.focus_rail(to == 2, cx);
        cx.notify();
    }

    pub(crate) fn focus_rail(&self, held: bool, cx: &mut Context<Self>) {
        if let Some(join) = &self.rail {
            join.places.update(cx, |producer, cx| {
                // New requests take the pane selected by this synchronous focus handoff.
                producer.side = self.focus_side;
                producer.model.borrow_mut().focused = held;
                if !held {
                    producer.cancel_rename(cx);
                }
            });
        }
        if let Some(host) = self
            .slots
            .rail
            .clone()
            .and_then(|view| view.downcast::<crate::rail::host::Host>().ok())
        {
            host.update(cx, |host, cx| host.focus(held, cx));
        }
    }
}
