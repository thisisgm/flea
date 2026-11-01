//! Compatibility entry points use VF3's shared target and feedback owner for every view.
use super::*;

impl Shell {
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
