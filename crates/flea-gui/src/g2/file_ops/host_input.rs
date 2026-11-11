//! Reports consumed keys so a finished Jobs card leaves Escape to the window footer.
use super::*;

impl Overlay {
    pub fn dispatch_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.held_key(event, cx) {
            return true;
        }
        let keys = &event.keystroke;
        if self.copy_key(event, window, cx) {
            return true;
        }
        if keys.key == "escape" && !self.model.open() {
            if !self.cancel_drawn_activity() {
                return false;
            }
            cx.stop_propagation();
            cx.notify();
            return true;
        }
        let held = keys.modifiers.control || keys.modifiers.alt || keys.modifiers.platform;
        if held && keys.key != "escape" {
            if self.model.open() {
                cx.stop_propagation();
            }
            return self.model.open();
        }
        if self
            .model
            .key(&self.catalog, &keys.key, keys.modifiers.shift)
        {
            cx.stop_propagation();
            cx.notify();
            self.schedule(cx);
            return true;
        }
        false
    }
}
