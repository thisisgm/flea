//! The window registers one keyboard-delivery reader; its snapshot builder also serves direct reader callers.
use crate::{
    ipc::{keyboard_state::KeyboardState, Adapter},
    state::Shell,
};
use flea::ipc::wire::Reply;
use gpui::{App, Window};

pub(crate) struct Reader<'a> {
    pub shell: &'a Shell,
    pub window: &'a Window,
    pub cx: &'a App,
}

impl Adapter for Reader<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
        KeyboardState {
            shell: self.shell,
            window: self.window,
            cx: self.cx,
        }
        .call(function, arguments)
    }
}

#[cfg(test)]
#[path = "key_delivery_tests.rs"]
mod tests;
