//! Compatibility entry points use VF3's shared target and feedback owner for every view.
use super::*;

impl Shell {
    /// Source-window keys reach every destination's existing shared drag cue.
    pub(crate) fn root_modifiers_changed(
        &mut self,
        event: &gpui::ModifiersChangedEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if cx.try_global::<Drags>().is_none() {
            self.run_card_modifiers(event, window, cx);
            return;
        }
        if let Some(drags) = cx.try_global::<Drags>() {
            for hover in drags.hovers.values() {
                if let Some(hover) = hover.borrow_mut().as_mut() {
                    hover.ctrl = event.modifiers.control;
                    hover.shift = event.modifiers.shift;
                }
            }
        }
        self.file_drag_modifiers(event.modifiers, cx);
        self.run_card_modifiers(event, window, cx);
        for handle in cx.windows() {
            if handle.window_id().as_u64() == window_key(window) {
                continue;
            }
            if let Some(handle) = handle.downcast::<Self>() {
                let _ = handle.update(cx, |shell, window, cx| shell.refresh_file_cue(window, cx));
            } else if let Some(handle) = handle.downcast::<crate::window::root::Root>() {
                let _ = handle.update(cx, |root, window, cx| {
                    root.shell
                        .update(cx, |shell, cx| shell.refresh_file_cue(window, cx));
                });
            }
        }
    }

    /// A lift still held elsewhere keeps its unnamed line; a lift whose native drag ended is dropped here.
    pub(crate) fn settle_row_drag(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.clear_file_cue(window, cx);
        let key = window_key(window);
        let held = cx
            .try_global::<Drags>()
            .and_then(|d| d.held.get(&key))
            .is_some_and(|held| held.borrow().is_some());
        self.clear_rail_aim(window, cx);
        if held {
            self.say_held_drag(window, cx);
        } else {
            cx.default_global::<Drags>().take(key);
        }
    }
}

#[cfg(test)]
use super::super::files::Arrived;
#[cfg(test)]
#[path = "row_drop_tests.rs"]
mod tests;
