use crate::{backend::Backend, state::Shell};
use gpui::TestAppContext;

fn shell(backend: Backend, cx: &mut gpui::Context<Shell>) -> Shell {
    let mut shell = crate::state::tests::shell_with(backend, cx);
    shell.launch.settings.data = flea::uischema::defaults();
    shell
}

#[gpui::test]
fn fxuish2b_tail_text_size_notice_does_not_acknowledge_an_unreadable_settings_error(
    cx: &mut TestAppContext,
) {
    let (backend, _requests) = Backend::test_channel();
    let (entity, cx) = cx.add_window_view(|window, cx| {
        let shell = shell(backend, cx);
        window.focus(&shell.focus, cx);
        shell
    });
    entity.update_in(cx, |shell, window, cx| {
        shell.receive_settings(
            super::store::Update::Notice("settings.error.unreadable"),
            window,
            cx,
        );
    });
    let expected = "Your saved settings could not be read, so these are the defaults.";
    assert_eq!(
        entity
            .read_with(cx, |shell, cx| shell.settings_message(cx))
            .as_deref(),
        Some(expected)
    );
    cx.simulate_keystrokes("ctrl-shift--");
    entity.update_in(cx, |shell, window, cx| {
        shell.receive_settings(
            super::store::Update::Failed {
                through: 1,
                keep: false,
            },
            window,
            cx,
        );
        assert_eq!(shell.settings_message(cx).as_deref(), Some(expected));
    });
    cx.simulate_keystrokes("escape");
    assert_eq!(
        entity
            .read_with(cx, |shell, cx| shell.settings_message(cx))
            .as_deref(),
        Some("That setting could not be saved.")
    );
    entity.update(cx, |shell, _| shell.stop_for_app_quit());
}

// The two the state file says at launch: opened on the defaults, or saves refused under a held lock.
#[gpui::test]
fn fx_state_launch_notices_say_what_happened_to_the_state_file(cx: &mut TestAppContext) {
    for (key, said) in [
        (
            "settings.error.unreadable",
            "Your saved settings could not be read, so these are the defaults.",
        ),
        (
            "settings.error.save_session",
            "Could not save settings · changes apply to this session only",
        ),
    ] {
        let (backend, _requests) = Backend::test_channel();
        let (entity, cx) = cx.add_window_view(|_, cx| shell(backend, cx));
        entity.update_in(cx, |shell, window, cx| {
            shell.receive_settings(super::store::Update::Notice(key), window, cx);
            assert_eq!(shell.settings_message(cx).as_deref(), Some(said));
        });
    }
}
