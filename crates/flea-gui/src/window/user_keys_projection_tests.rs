//! Consumer regressions use raw keystrokes, held modifiers and the production projection cache.
use super::{layout::Cache, test_fixture, Bound};
use flea::keys::user::{parse, Verdict};
use gpui::{Capslock, KeyDownEvent, Keystroke, Modifiers, TestAppContext, VisualTestContext};

type Samples = fn(Modifiers, Capslock) -> Option<Vec<(Keystroke, Modifiers)>>;

fn uk(held: Modifiers, caps: Capslock) -> Option<Vec<(Keystroke, Modifiers)>> {
    let mut samples = test_fixture::samples(held, caps)?;
    if held.shift && !held.control && !held.alt && !held.platform {
        for (stroke, _) in &mut samples {
            if stroke.us_key.as_deref() == Some("#") {
                stroke.key = "sterling".into();
                stroke.key_char = Some("£".into());
                stroke.modifiers = held;
            }
        }
    }
    Some(samples)
}

fn ru(held: Modifiers, caps: Capslock) -> Option<Vec<(Keystroke, Modifiers)>> {
    let mut samples = test_fixture::samples(held, caps)?;
    if !held.control && !held.alt && !held.platform {
        for (stroke, _) in &mut samples {
            if stroke.us_key.as_deref() == Some("q") {
                stroke.key_char = Some(if held.shift ^ caps.on { "Й" } else { "й" }.into());
            }
        }
    }
    Some(samples)
}

fn stroke(source: Samples, shift: bool, caps: bool, us: &str) -> (KeyDownEvent, Modifiers) {
    let held = Modifiers {
        shift,
        ..Default::default()
    };
    let (keystroke, held) = source(held, Capslock { on: caps })
        .unwrap()
        .into_iter()
        .find(|(key, _)| key.us_key.as_deref() == Some(us))
        .unwrap();
    (
        KeyDownEvent {
            keystroke,
            is_held: false,
            prefer_character_input: false,
        },
        held,
    )
}

fn mount<'a>(
    cx: &'a mut TestAppContext,
    text: &str,
    samples: Samples,
) -> (gpui::Entity<crate::state::Shell>, &'a mut VisualTestContext) {
    let (backend, _requests) = crate::backend::Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let mut shell = crate::state::tests::shell_with(backend, cx);
        shell.launch.settings.data = flea::uistate::from_file("{}");
        shell.user_keys.facts.keys = parse(text);
        let masks = shell
            .user_keys
            .facts
            .keys
            .bindings
            .iter()
            .flat_map(|binding| binding.required_modifiers());
        shell
            .user_keys
            .layout
            .refresh(cx.keyboard_layout().id(), masks, samples);
        shell.user_keys_changed(window, cx);
        window.focus(&shell.focus, cx);
        shell
    });
    cx.run_until_parked();
    (shell, cx)
}

#[gpui::test]
fn user_keys_us0_and_shift0_keep_distinct_live_dispatch(cx: &mut TestAppContext) {
    let (shell, cx) = mount(cx, "[[text]]\nkey = \"0\"\naction = \"settings\"\n[[shift]]\nkey = \"0\"\naction = \"toggleHidden\"\n", test_fixture::samples);
    shell.update(cx, |shell, _| {
        assert_eq!(
            shell.user_keys.facts.current_verdicts("default").unwrap(),
            &[Verdict::Live, Verdict::Live]
        );
        for (shift, us, action) in [(false, "0", "settings"), (true, ")", "toggleHidden")] {
            let (event, held) = stroke(test_fixture::samples, shift, false, us);
            assert_eq!(
                shell.user_key("default", &event, held),
                Some(Bound::Builtin(action))
            );
        }
        let rows =
            flea::ui_logic_settings_pages::pages::user_key_rows(&shell.user_keys.facts, "default");
        assert_eq!(
            rows[1].items.len(),
            2,
            "Settings and dispatch share both live verdicts"
        );
    });
}

#[gpui::test]
fn user_keys_uk_pound_and_shift3_refuse_later_binding_in_either_order(cx: &mut TestAppContext) {
    for (first, second) in [("shift", "text"), ("text", "shift")] {
        let key = |table| if table == "shift" { "3" } else { "£" };
        let text = format!("[[{first}]]\nkey = \"{}\"\naction = \"settings\"\n[[{second}]]\nkey = \"{}\"\naction = \"toggleHidden\"\n", key(first), key(second));
        let (shell, cx) = mount(cx, &text, uk);
        shell.update(cx, |shell, _| {
            assert_eq!(
                shell.user_keys.facts.current_verdicts("default").unwrap(),
                &[Verdict::Live, Verdict::Repeated]
            );
            let (event, held) = stroke(uk, true, false, "#");
            assert_eq!(event.keystroke.key, "sterling");
            assert_eq!(event.keystroke.key_char.as_deref(), Some("£"));
            assert!(held.shift && event.keystroke.modifiers.shift);
            assert_eq!(
                shell.user_key("default", &event, held),
                Some(Bound::Builtin("settings"))
            );
            assert_eq!(shell.user_keys.said.len(), 1);
            assert!(shell.user_keys.said[0].contains("earlier line"));
            let rows = flea::ui_logic_settings_pages::pages::user_key_rows(
                &shell.user_keys.facts,
                "default",
            );
            assert_eq!(rows[1].items.len(), 1);
        });
    }
}

#[gpui::test]
fn user_keys_russian_caps_overlap_uses_printed_text_and_us_position(cx: &mut TestAppContext) {
    let (shell, cx) = mount(cx, "[[code]]\nkey = \"Q\"\naction = \"settings\"\n[[text]]\nkey = \"Й\"\naction = \"toggleHidden\"\n", ru);
    shell.update(cx, |shell, _| {
        assert_eq!(
            shell.user_keys.facts.current_verdicts("default").unwrap(),
            &[Verdict::Live, Verdict::Repeated]
        );
        let (event, held) = stroke(ru, false, true, "q");
        assert_eq!(event.keystroke.key_char.as_deref(), Some("Й"));
        assert!(!held.shift);
        assert_eq!(
            shell.user_key("default", &event, held),
            Some(Bound::Builtin("settings"))
        );
    });
}

#[test]
fn user_keys_cache_replaces_same_named_custom_map_and_reuses_supported_states() {
    let keys = parse("[[text]]\nkey = \"£\"\naction = \"settings\"\n[[text]]\nkey = \"€\"\naction = \"toggleHidden\"\n");
    let mut cache = Cache::default();
    let masks = || {
        keys.bindings
            .iter()
            .flat_map(|binding| binding.required_modifiers())
    };
    let mut calls = 0;
    let mut load = |identity: &str, currency: &str| {
        cache.refresh(identity, masks(), |held, caps| {
            calls += 1;
            let mut samples = test_fixture::samples(held, caps).unwrap();
            if !held.shift && !held.control && !held.alt && !held.platform {
                for (stroke, _) in &mut samples {
                    if stroke.us_key.as_deref() == Some("0") {
                        stroke.key = "currency".into();
                        stroke.key_char = Some(currency.into());
                    }
                }
            }
            Some(samples)
        });
        keys.verdicts("default", &|_| false, &cache.projection)
    };
    assert_eq!(
        load("xkb:1:0:Custom", "£"),
        vec![Verdict::Live, Verdict::Unavailable]
    );
    assert_eq!(
        load("xkb:1:0:Custom", "€"),
        vec![Verdict::Live, Verdict::Unavailable],
        "same generation retains immutable old map"
    );
    assert_eq!(
        load("xkb:2:0:Custom", "€"),
        vec![Verdict::Unavailable, Verdict::Live],
        "generation changes even when display name does not"
    );
    assert_eq!(
        calls, 8,
        "two masks and both Caps values, once per generation"
    );
}

#[gpui::test]
fn user_keys_unsupported_projection_is_explicit_and_never_fabricated(cx: &mut TestAppContext) {
    let (shell, cx) = mount(
        cx,
        "[[ctrlshift]]\nkey = \"K\"\naction = \"settings\"\n",
        |_, _| None,
    );
    shell.update(cx, |shell, _| {
        assert_eq!(
            shell.user_keys.facts.current_verdicts("default").unwrap(),
            &[Verdict::Unavailable]
        );
        let event = KeyDownEvent {
            keystroke: Keystroke::parse("ctrl-shift-k").unwrap(),
            is_held: false,
            prefer_character_input: false,
        };
        assert_eq!(shell.user_key("default", &event, Modifiers::none()), None);
        assert_eq!(shell.user_keys.said.len(), 1);
        assert!(shell.user_keys.said[0].contains("cannot verify"));
        let rows =
            flea::ui_logic_settings_pages::pages::user_key_rows(&shell.user_keys.facts, "default");
        assert!(rows.iter().all(|row| row.items.is_empty()));
    });
}

#[gpui::test]
fn user_keys_raw_projection_never_bypasses_native_composition_guards(cx: &mut TestAppContext) {
    let (shell, cx) = mount(
        cx,
        "[[ctrlshift]]\nkey = \"K\"\naction = \"settings\"\n",
        test_fixture::samples,
    );
    shell.update(cx, |shell, _| {
        let mut event = KeyDownEvent {
            keystroke: Keystroke::parse("ctrl-shift-k").unwrap(),
            is_held: false,
            prefer_character_input: false,
        };
        assert_eq!(
            shell.user_key("default", &event, Modifiers::none()),
            Some(Bound::Builtin("settings"))
        );
        event.prefer_character_input = true;
        assert_eq!(shell.user_key("default", &event, Modifiers::none()), None);
        event.prefer_character_input = false;
        shell.keyboard.set_composing(true);
        assert_eq!(shell.user_key("default", &event, Modifiers::none()), None);
    });
}
