//! The opt-in test probe counts entry to the actual cached pane renderer.
use crate::state::Shell;

#[derive(Default)]
pub(super) struct EmptyOmiShellRenders(pub(super) std::cell::Cell<usize>);
impl gpui::Global for EmptyOmiShellRenders {}

impl Shell {
    pub(crate) fn observe_empty_omi_shell_renders(cx: &mut gpui::App) {
        assert!(cx.try_global::<EmptyOmiShellRenders>().is_none());
        cx.set_global(EmptyOmiShellRenders::default());
    }

    pub(crate) fn empty_omi_shell_renders(cx: &gpui::App) -> usize {
        cx.global::<EmptyOmiShellRenders>().0.get()
    }
}
