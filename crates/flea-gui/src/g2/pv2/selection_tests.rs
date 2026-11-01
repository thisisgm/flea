//! Selection retains source bytes and treats only visible, clipped runs as endpoints.
use super::*;

fn selected(chunks: &[(usize, &str)], start: Position, end: Position) -> Selection {
    let s = Selection::default();
    *s.state.borrow_mut() = State {
        anchor: Some(start),
        head: end,
        ..State::default()
    };
    Selection {
        chunks: chunks
            .iter()
            .map(|(n, text)| (*n, Chunk::new((*text).into())))
            .collect(),
        ..s
    }
}

#[test]
fn gm_prevsel_original_tabs_crlf_unicode_and_terminal_newline_survive_copy() {
    let s = selected(
        &[(0, "α\tβ\r\n"), (1, "second\n")],
        Position { chunk: 0, byte: 0 },
        Position { chunk: 1, byte: 7 },
    );
    assert_eq!(s.text().as_deref(), Some("α\tβ\r\nsecond\n"));
    assert_eq!(s.chunks[&0].text, "α       β\r\n");
    assert_eq!(s.part(0, 0, 2), Some(0..2));
    assert_eq!(s.part(2, 0, 4), None);
}

#[test]
fn gm_prevsel_reversed_partial_range_has_identical_bytes() {
    let mut s = selected(
        &[(0, "héllo\n"), (1, "第二 line")],
        Position { chunk: 1, byte: 6 },
        Position { chunk: 0, byte: 1 },
    );
    assert_eq!(s.text().as_deref(), Some("éllo\n第二"));
    s.clear();
    assert_eq!(s.text(), None);
    assert!(!s.dragging());
}

#[test]
fn gm_prevsel_missing_source_lines_never_copy_a_partial_range() {
    let mut s = selected(
        &[(0, "one\n"), (2, "three")],
        Position { chunk: 0, byte: 0 },
        Position { chunk: 2, byte: 5 },
    );
    s.contiguous = true;
    assert_eq!(s.text(), None);
    s.chunks.insert(1, Chunk::new("two\n".into()));
    assert_eq!(s.text().as_deref(), Some("one\ntwo\nthree"));
}

#[test]
fn gm_prevsel_unicode_words_and_punctuation_use_word_boundaries() {
    let s = selected(
        &[(0, "héllo, world")],
        Position::default(),
        Position::default(),
    );
    assert_eq!(
        s.word(Position { chunk: 0, byte: 3 }),
        Position::default()..Position { chunk: 0, byte: 6 }
    );
    assert_eq!(
        s.word(Position { chunk: 0, byte: 6 }),
        Position { chunk: 0, byte: 6 }..Position { chunk: 0, byte: 7 }
    );
    assert_eq!(
        s.word(Position { chunk: 0, byte: 10 }),
        Position { chunk: 0, byte: 8 }..Position { chunk: 0, byte: 13 }
    );
}

#[gpui::test]
fn gm_prevsel_clipped_text_cannot_become_a_selection_endpoint(cx: &mut gpui::TestAppContext) {
    let (_, cx) = cx.add_window_view(|_, _| gpui::Empty);
    cx.update(|window, _| {
        let run = gpui::TextRun {
            len: 6,
            font: gpui::font("monospace"),
            color: Hsla::default(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let line = window
            .text_system()
            .layout_line("hidden", px(14.), &[run], None);
        let visible = Bounds::new(point(px(0.), px(0.)), size(px(50.), px(20.)));
        let mut s = Selection::default();
        for (chunk, x) in [(0, 0.), (1, 200.)] {
            let bounds = Bounds::new(point(px(x), px(0.)), size(px(50.), px(20.)));
            s.spans.push(Span {
                chunk,
                start: 0,
                len: line.len,
                bounds,
                clip: bounds.intersect(&visible),
                line: line.clone(),
                reach: bounds.right(),
            });
        }
        assert_eq!(s.hit(point(px(210.), px(10.))).unwrap().chunk, 0);
    });
}

#[test]
fn gm_prevsel_unlaid_markdown_blocks_never_produce_a_partial_clipboard() {
    let mut s = selected(
        &[(0, "first\n"), (2 << 32, "last")],
        Position::default(),
        Position {
            chunk: 2 << 32,
            byte: 4,
        },
    );
    s.gaps.push((1 << 32)..(2 << 32));
    assert_eq!(s.text(), None);
    s.chunks.insert(1 << 32, Chunk::new("middle\n".into()));
    s.gaps.clear();
    assert_eq!(s.text().as_deref(), Some("first\nmiddle\nlast"));
}

#[gpui::test]
fn gm_prevsel_escape_clears_a_range_waiting_for_missing_text(cx: &mut gpui::TestAppContext) {
    let (_, cx) = cx.add_window_view(|_, _| gpui::Empty);
    cx.update(|window, cx| {
        let mut s = selected(
            &[(0, "first\n"), (2, "last")],
            Position::default(),
            Position { chunk: 2, byte: 4 },
        );
        s.contiguous = true;
        s.publish(window, cx);
        assert_eq!(s.text(), None);
        let event = gpui::KeyDownEvent {
            keystroke: gpui::Keystroke::parse("escape").unwrap(),
            is_held: false,
            prefer_character_input: false,
        };
        assert!(key(&event, window, cx));
        assert!(s.range().is_none());
    });
}

fn ctrl_c() -> gpui::KeyDownEvent {
    gpui::KeyDownEvent {
        keystroke: gpui::Keystroke::parse("ctrl-c").unwrap(),
        is_held: false,
        prefer_character_input: false,
    }
}

#[gpui::test]
fn gm_prevsel_copy_with_no_text_coming_leaves_the_key_to_its_table(cx: &mut gpui::TestAppContext) {
    let (_, cx) = cx.add_window_view(|_, _| gpui::Empty);
    cx.update(|window, cx| {
        let mut s = selected(
            &[(0, "first\n"), (2, "last")],
            Position::default(),
            Position { chunk: 2, byte: 4 },
        );
        s.contiguous = true;
        s.publish(window, cx);
        assert!(
            !key(&ctrl_c(), window, cx),
            "a range whose text can never arrive does not swallow Copy"
        );
        assert!(!s.state.borrow().copy_requested);
    });
}

#[test]
fn gm_prevsel_a_partly_covered_tab_is_copied_whole() {
    let tab = |from, to| {
        selected(
            &[(0, "a\tb\n")],
            Position {
                chunk: 0,
                byte: from,
            },
            Position { chunk: 0, byte: to },
        )
        .text()
    };
    assert_eq!(tab(3, 9).as_deref(), Some("\tb"));
    assert_eq!(tab(0, 3).as_deref(), Some("a\t"));
    assert_eq!(tab(3, 5).as_deref(), Some("\t"));
}

fn clipboard(cx: &App) -> Option<String> {
    cx.read_from_clipboard().and_then(|item| item.text())
}

#[gpui::test]
fn gm_prevsel_a_copy_waiting_when_the_window_blurs_never_lands_later(
    cx: &mut gpui::TestAppContext,
) {
    let (_, cx) = cx.add_window_view(|_, _| gpui::Empty);
    cx.update(|window, cx| {
        for blurred in [false, true] {
            let s = selected(
                &[(0, "a\n")],
                Position::default(),
                Position { chunk: 0, byte: 2 },
            );
            s.publish(window, cx);
            s.set_pending(true);
            cx.write_to_clipboard(ClipboardItem::new_string("other app".into()));
            assert!(key(&ctrl_c(), window, cx), "Copy waits for text on its way");
            if blurred {
                blur(window, cx);
            }
            s.set_pending(false);
            s.complete_copy(cx);
            let want = if blurred { "other app" } else { "a\n" };
            assert_eq!(clipboard(cx).as_deref(), Some(want), "blurred={blurred}");
        }
    });
}

#[gpui::test]
fn gm_prevsel_copy_waits_for_the_original_bytes_while_they_are_read(cx: &mut gpui::TestAppContext) {
    let (_, cx) = cx.add_window_view(|_, _| gpui::Empty);
    cx.update(|window, cx| {
        let mut s = selected(
            &[(0, "a\tb\n")],
            Position::default(),
            Position { chunk: 0, byte: 11 },
        );
        s.publish(window, cx);
        s.set_pending(true);
        s.complete_copy(cx);
        cx.write_to_clipboard(ClipboardItem::new_string("sentinel".into()));
        assert!(key(&ctrl_c(), window, cx));
        assert_eq!(
            clipboard(cx).as_deref(),
            Some("sentinel"),
            "the drawn row, without its carriage return, is never copied while the file is read"
        );
        s.lines = Some(Lines::new("a\tb\r\n".into(), Arc::new(vec![0])));
        s.set_pending(false);
        s.complete_copy(cx);
        assert_eq!(clipboard(cx).as_deref(), Some("a\tb\r\n"));
    });
}

#[gpui::test]
fn gm_prevsel_a_double_click_selects_only_the_word_under_the_pointer(
    cx: &mut gpui::TestAppContext,
) {
    let (_, cx) = cx.add_window_view(|_, _| gpui::Empty);
    cx.update(|window, cx| {
        let text = "héllo world";
        let run = gpui::TextRun {
            len: text.len(),
            font: gpui::font("monospace"),
            color: Hsla::default(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let line = window
            .text_system()
            .layout_line(text, px(14.), &[run], None);
        let bounds = Bounds::new(point(px(0.), px(0.)), size(line.width, px(20.)));
        let mut s = Selection::default();
        s.chunks.insert(0, Chunk::new(format!("{text}\n")));
        s.spans.push(Span {
            chunk: 0,
            start: 0,
            len: line.len,
            bounds,
            clip: bounds,
            line: line.clone(),
            reach: bounds.right(),
        });
        let double = |x: Pixels| MouseDownEvent {
            position: point(x, px(10.)),
            button: gpui::MouseButton::Left,
            modifiers: gpui::Modifiers::none(),
            click_count: 2,
            first_mouse: false,
        };
        let d = line.x_for_index(text.len() - 1);
        let right_half = d + (line.width - d) * 0.75;
        let space = line.x_for_index(6) + px(1.);
        for (x, y, want) in [
            (line.width + px(40.), px(10.), None),
            (px(1.), px(40.), None),
            (px(1.), px(-10.), None),
            (px(-10.), px(10.), None),
            (right_half, px(10.), Some("world")),
            (space, px(10.), None),
            (px(1.), px(10.), Some("héllo")),
        ] {
            let mut event = double(x);
            event.position.y = y;
            s.press(&event, event.position, window, cx);
            s.release(cx);
            assert_eq!(s.text().as_deref(), want, "double-click at {x:?},{y:?}");
            assert_eq!(
                s.range().is_some(),
                want.is_some(),
                "a range is what is washed"
            );
        }
    });
}
