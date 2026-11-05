//! The scrollKeepsCursor setting reaches the panes as soon as the settings change, and never the Columns view.
use crate::{backend::Backend, state::tests::shell_with, window::startup::View};
use gpui::{px, size, TestAppContext, TestDispatcher};

#[test]
fn a_settings_change_hands_the_choice_to_the_list_pane_but_not_to_columns() {
    let mut cx = TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        gpui_platform::current_platform(true).text_system(),
    );
    let (backend, _requests) = Backend::test_channel();
    let window = cx.open_window(size(px(1000.), px(700.)), |_, cx| shell_with(backend, cx));
    window
        .update(&mut cx, |shell, window, cx| {
            let keeps = |shell: &crate::state::Shell| {
                shell.panes[0].as_ref().unwrap().input.keep_cursor_on_scroll
            };
            shell.set_view(View::List, window, cx);
            let mut data = shell.launch.settings.data.clone();
            if let flea::jsondoc::Json::Obj(fields) = &mut data {
                fields.retain(|(key, _)| key != "scrollKeepsCursor");
                fields.push(("scrollKeepsCursor".into(), flea::jsondoc::Json::Bool(true)));
            }
            // No frame in between: the change alone hands it to the pane.
            shell.apply_settings(data, window, cx);
            assert!(keeps(shell));
            shell.set_view(View::Columns, window, cx);
            shell.sync_scroll_keeps_cursor();
            assert!(!keeps(shell), "Columns keeps pulling the cursor along");
        })
        .unwrap();
}
