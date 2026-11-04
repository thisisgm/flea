//! Tab enters the toolbar's first live glyph forward and its last backward, as ChromeKeys.enter does.
use crate::{input::focus::Stop, state::Shell};
use gpui::{Context, KeyDownEvent, Window};

impl Shell {
    pub(crate) fn sync_chrome_stops(&mut self, window: &Window) {
        self.input.register(Stop {
            name: "chrome",
            handle: self.dual_focus.chrome.clone(),
            context: flea::keys::Context::Panel,
        });
        if self.slots.tabs.is_some() && self.geometry(window).tabs.height > 0. {
            self.input.register(Stop {
                name: "strip",
                handle: self.dual_focus.strip.clone(),
                context: flea::keys::Context::Panel,
            });
        } else {
            self.input.remove("strip");
        }
    }

    pub(crate) fn enter_chrome(&self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(chrome) = &self.chrome.view else {
            return;
        };
        let chrome = chrome.read(cx);
        let frame = chrome.frame.borrow();
        let mut live = frame
            .controls
            .iter()
            .enumerate()
            .filter(|(_, control)| control.visible && control.enabled);
        let target = if forward {
            live.next()
        } else {
            live.next_back()
        };
        let handle = target.map(|(index, _)| chrome.focus[index].clone());
        drop(frame);
        if let Some(handle) = handle {
            window.focus(&handle, cx);
        }
    }

    pub(crate) fn enter_strip(&mut self, forward: bool) {
        let tabs = &self.strip.tabs[self.focus_side];
        self.strip.bar.focused_index = if forward {
            tabs.current()
        } else {
            tabs.count()
        };
    }

    /// ChromeKeys.act reads these keys before the preset; Tab still walks the outer ring.
    pub(crate) fn chrome_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let stop = self.focus_view(window, cx);
        if !matches!(stop, "chrome" | "strip") || self.keyboard.composing(event) {
            return false;
        }
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        let blocked = modifiers.control || modifiers.alt || modifiers.platform;
        if key == "tab" && !blocked {
            return false;
        }
        if blocked {
            return true;
        }
        if key == "escape" {
            self.input_action("escape", window, cx);
            return true;
        }
        let press = matches!(key, "enter" | "space");
        if stop == "strip" {
            let count = self.strip.tabs[self.focus_side].count();
            if count <= 1 || self.slots.tabs.is_none() {
                self.input_action("escape", window, cx);
                return false;
            }
            let at = self.strip.bar.focused_index.min(count);
            if press {
                let hit = if at < count {
                    crate::tabs::bar::Hit::Tab(at)
                } else {
                    crate::tabs::bar::Hit::New
                };
                self.input_action("escape", window, cx);
                self.tab_tap_hit(hit, false, window, cx);
            } else if let Some(to) = target(key, Some(at), count + 1) {
                self.strip.bar.focused_index = to;
                cx.notify();
            }
            return true;
        }
        self.sync_chrome(window, cx);
        let Some(chrome) = &self.chrome.view else {
            return false;
        };
        let chrome = chrome.read(cx);
        let frame = chrome.frame.borrow();
        let live: Vec<_> = frame
            .controls
            .iter()
            .enumerate()
            .filter(|(_, control)| chrome.inputs.live && control.visible && control.enabled)
            .map(|(index, _)| index)
            .collect();
        let focused = chrome
            .focus
            .iter()
            .position(|handle| handle.is_focused(window));
        let at = focused.and_then(|index| live.iter().position(|live| *live == index));
        let to = if press && at.is_none() {
            live.iter()
                .enumerate()
                .min_by_key(|(_, index)| index.abs_diff(focused.unwrap_or(0)))
                .map(|(at, _)| at)
        } else {
            target(key, at, live.len())
        };
        let handle = to.map(|to| chrome.focus[live[to]].clone());
        let activation = press.then(|| at.map(|at| live[at])).flatten();
        let keep = activation.map(|index| chrome.focus[index].clone());
        drop(frame);
        if live.is_empty() {
            self.input_action("escape", window, cx);
            return false;
        }
        if let Some(handle) = handle {
            window.focus(&handle, cx);
            cx.notify();
        }
        if let Some(index) = activation {
            // The glyph invokes the same action as a pointer, then keeps focus unless it opened a field or Settings.
            self.keyboard.context = flea::keys::Context::Listing;
            self.chrome_action(super::chrome::ACTIONS[index], window, cx);
            if !matches!(index, 2 | 3 | 9) {
                window.focus(&keep.expect("activated glyph"), cx);
                self.keyboard.context = flea::keys::Context::Panel;
            }
            cx.notify();
        }
        true
    }
}

fn target(key: &str, at: Option<usize>, count: usize) -> Option<usize> {
    let last = count.checked_sub(1)?;
    Some(match key {
        "home" => 0,
        "end" => last,
        "left" => at.unwrap_or(0).saturating_sub(1),
        "right" => at.map_or(0, |at| (at + 1).min(last)),
        _ => return None,
    })
}
