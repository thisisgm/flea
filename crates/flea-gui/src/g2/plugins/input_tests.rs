use crate::{backend::Backend, state::Shell};
use flea::ui_logic_plugin_settings::contributions::ActionInput;
use gpui::{KeyDownEvent, Keystroke, TestAppContext};

fn descriptor() -> ActionInput {
    ActionInput {
        key: "checksum".into(),
        title: "Verify Checksum".into(),
        label: "Checksum".into(),
        placeholder: String::new(),
        submit: "Verify".into(),
        max_chars: 128,
    }
}

fn key(word: &str) -> KeyDownEvent {
    KeyDownEvent {
        keystroke: Keystroke::parse(word).unwrap(),
        is_held: false,
        prefer_character_input: false,
    }
}

#[gpui::test]
fn plugin_input_uses_shared_editor_and_submits_once(cx: &mut TestAppContext) {
    let (backend, _) = Backend::test_channel();
    let (entity, cx) = cx.add_window_view(|_, cx| crate::state::tests::shell_with(backend, cx));
    entity.update_in(cx, |shell: &mut Shell, window, cx| {
        let receiver = shell.plugin_text_input(&descriptor(), window, cx).unwrap();
        assert!(shell.modal_card_holds_keyboard(cx));
        assert!(shell.plugin_input_layer(window, cx).is_some());
        assert_eq!(
            shell.rename.ime.owner,
            Some(crate::g2::file_ops::dialog_editor::Slot::PluginAction)
        );
        shell.finish_plugin_input(true, window, cx);
        assert!(receiver.try_recv().is_err(), "empty field cannot submit");
        shell.rename.ime.entry.ime_replace(None, "abc123");
        shell.dialog_edited(cx);
        assert_eq!(shell.plugins.input.as_ref().unwrap().value, "abc123");
        assert!(shell.plugin_input_key(&key("tab"), window, cx));
        assert_eq!(shell.plugins.input.as_ref().unwrap().focus, 1);
        shell.plugin_input_key(&key("tab"), window, cx);
        assert_eq!(shell.plugins.input.as_ref().unwrap().focus, 2);
        shell.plugin_input_key(&key("enter"), window, cx);
        assert_eq!(receiver.try_recv().unwrap(), Some("abc123".into()));
        assert!(shell.plugins.input.is_none());
        shell.finish_plugin_input(true, window, cx);
        assert!(receiver.try_recv().is_err(), "no second submission");
        assert!(!shell.modal_card_holds_keyboard(cx));
    });
}

#[gpui::test]
fn plugin_input_escape_cancel_and_dismiss_never_submit(cx: &mut TestAppContext) {
    let (backend, _) = Backend::test_channel();
    let (entity, cx) = cx.add_window_view(|_, cx| crate::state::tests::shell_with(backend, cx));
    entity.update_in(cx, |shell: &mut Shell, window, cx| {
        for action in ["escape", "enter on cancel", "cancel", "dismiss"] {
            let receiver = shell.plugin_text_input(&descriptor(), window, cx).unwrap();
            match action {
                "escape" => {
                    shell.plugin_input_key(&key("escape"), window, cx);
                }
                "enter on cancel" => {
                    shell.rename.ime.entry.ime_replace(None, "abc123");
                    shell.dialog_edited(cx);
                    shell.plugin_input_key(&key("tab"), window, cx);
                    assert_eq!(shell.plugins.input.as_ref().unwrap().focus, 1);
                    shell.plugin_input_key(&key("enter"), window, cx);
                }
                "cancel" => shell.finish_plugin_input(false, window, cx),
                _ => shell.dismiss_cards(cx),
            }
            assert_eq!(receiver.try_recv().unwrap(), None, "{action}");
            assert!(shell.plugins.input.is_none());
            assert!(!shell.modal_card_holds_keyboard(cx));
        }
    });
}

#[gpui::test]
fn plugin_input_submits_a_value_of_exactly_its_character_bound(cx: &mut TestAppContext) {
    let (backend, _) = Backend::test_channel();
    let (entity, cx) = cx.add_window_view(|_, cx| crate::state::tests::shell_with(backend, cx));
    entity.update_in(cx, |shell: &mut Shell, window, cx| {
        let receiver = shell.plugin_text_input(&descriptor(), window, cx).unwrap();
        // Two bytes a character, so a byte count would refuse what the character bound allows.
        let value = "é".repeat(descriptor().max_chars);
        shell.rename.ime.entry.ime_replace(None, &value);
        shell.dialog_edited(cx);
        assert_eq!(shell.plugins.input.as_ref().unwrap().value, value);
        shell.finish_plugin_input(true, window, cx);
        assert_eq!(receiver.try_recv().unwrap(), Some(value));
    });
}
