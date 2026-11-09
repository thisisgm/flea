//! Filter hints and search match ink through the mounted window's native key route.
use super::{
    search_open_tests::answer,
    test_shell::{land, open, open_in, row},
};
use crate::{
    columns::geometry::Slot,
    window::{columns_tests::host, u07_tests::read},
};
use gpui::{KeyDownEvent, Keystroke, TestAppContext};

#[gpui::test]
fn unbound_printable_letters_show_the_filter_hint(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", vec![row("notes.txt", false)], 1);
    cx.simulate_keystrokes("n");
    assert_eq!(
        read(&shell, cx, "lastMessage", &[]),
        "Press / to filter this folder by name"
    );
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .filter
            .owner
            .query()
            .to_owned()),
        ""
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn action_keys_and_open_query_lines_keep_their_own_text(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", vec![row("notes.txt", false)], 1);
    cx.simulate_keystrokes("j");
    assert_eq!(read(&shell, cx, "lastMessage", &[]), "");
    cx.simulate_keystrokes("/ n");
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .filter
            .owner
            .query()
            .to_owned()),
        "n"
    );
    assert_eq!(read(&shell, cx, "lastMessage", &[]), "");
    cx.simulate_keystrokes("escape f n");
    assert_eq!(read(&shell, cx, "lastMessage", &[]), "");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[test]
fn columns_search_washes_only_the_non_cursor_typed_runs() {
    let mut app = host::context();
    app.update(|cx| {
        cx.set_global(crate::presentation::Fonts::from_sources(
            "[font]\nbase-size=14\n",
            "",
            "JetBrains Mono".into(),
        ));
        cx.text_system()
            .add_fonts(vec![
                std::borrow::Cow::Borrowed(
                    include_bytes!("../../../tests/fonts/JetBrainsMono-Regular.ttf").as_slice(),
                ),
                std::borrow::Cow::Borrowed(
                    include_bytes!("../../../tests/fonts/JetBrainsMono-Bold.ttf").as_slice(),
                ),
            ])
            .unwrap();
    });
    let (shell, cx) = open_in(&mut app, "default", "columns");
    land(&shell, cx, "/files", vec![row("Kinds", true)], 1);
    cx.update(|window, cx| shell.update(cx, |shell, cx| host::settle(shell, window, cx)));
    cx.run_until_parked();
    cx.simulate_keystrokes("f c o d e");
    answer(
        &shell,
        cx,
        4,
        vec![
            row("code-c.c", false),
            row("code-js.js", false),
            row("code-py.py", false),
        ],
    );
    cx.simulate_keystrokes("home");
    cx.update(|window, cx| shell.update(cx, |shell, cx| host::settle(shell, window, cx)));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    let (bounds, colour) = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.pane.state.query.as_str(), "code");
        let paint = host
            .columns
            .as_ref()
            .unwrap()
            .painters
            .get(Slot::Active)
            .unwrap()
            .borrow();
        assert_eq!(paint.visible().collect::<Vec<_>>(), vec![0, 1, 2]);
        (
            paint.drawn.as_ref().unwrap().bounds,
            paint.painted.unwrap().accent.opacity(0.14),
        )
    });
    let washes = cx.update(|window, _| {
        let scale = window.scale_factor();
        window
            .painted_quads()
            .into_iter()
            .filter(|quad| {
                quad.background.as_solid() == Some(colour)
                    && quad.bounds.origin.x.as_f32() >= f32::from(bounds.origin.x) * scale
                    && quad.bounds.origin.x.as_f32() < f32::from(bounds.right()) * scale
            })
            .map(|quad| quad.bounds.origin.y.as_f32())
            .collect::<Vec<_>>()
    });
    assert_eq!(
        washes.len(),
        2,
        "each non-cursor name paints its typed run: {washes:?}"
    );
    cx.update(|window, cx| {
        shell.read_with(cx, |shell, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let paint = host
                .columns
                .as_ref()
                .unwrap()
                .painters
                .get(Slot::Active)
                .unwrap()
                .borrow();
            let bold = window.text_system().resolve_font(&gpui::Font {
                weight: gpui::FontWeight::BOLD,
                ..paint.typeface.font.clone()
            });
            let regular = window.text_system().resolve_font(&paint.typeface.font);
            assert_ne!(
                bold, regular,
                "the real text system must resolve both weights"
            );
            let name = paint.prepared(0).unwrap().name_for(false, true).line();
            assert_eq!(
                name.runs[0].font_id, bold,
                "the cursor's typed run uses bold glyphs"
            );
            assert_eq!(
                name.runs.last().unwrap().font_id,
                regular,
                "the suffix stays regular"
            );
            let other = paint.prepared(1).unwrap().name_for(false, false).line();
            assert!(other.runs.iter().all(|run| run.font_id == regular));
        });
    });
    let before = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let paint = host
            .columns
            .as_ref()
            .unwrap()
            .painters
            .get(Slot::Active)
            .unwrap()
            .borrow();
        (
            paint.cache.misses,
            paint.cache.len(),
            paint.cache.capacity(),
        )
    });
    cx.simulate_keystrokes("down up");
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let paint = host
            .columns
            .as_ref()
            .unwrap()
            .painters
            .get(Slot::Active)
            .unwrap()
            .borrow();
        assert_eq!(
            paint.cache.misses, before.0,
            "moving the cursor reuses cached rows"
        );
        assert!(paint.cache.len() <= paint.cache.capacity());
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn composition_does_not_publish_a_filter_hint(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", vec![row("notes.txt", false)], 1);
    cx.simulate_event(KeyDownEvent {
        keystroke: Keystroke {
            key: "n".into(),
            key_char: Some("n".into()),
            ..Default::default()
        },
        is_held: false,
        prefer_character_input: true,
    });
    assert_eq!(read(&shell, cx, "lastMessage", &[]), "");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
