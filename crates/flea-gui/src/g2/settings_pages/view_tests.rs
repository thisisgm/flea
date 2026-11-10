//! The Settings card and the sheet drawn in a test window, driven by real keystrokes.
use super::seams::Request;
use super::sheet::{Sheet, Sources};
use super::sheet_view::SheetView;
use super::tests::{english, panel};
use super::view::SettingsView;
use gpui::{Focusable, TestAppContext};
use std::cell::RefCell;
use std::rc::Rc;
#[path = "plugin_layout_tests.rs"]
mod plugin_layout;

impl Focusable for SettingsView {
    fn focus_handle(&self, _: &gpui::App) -> gpui::FocusHandle {
        self.focus.clone()
    }
}

#[gpui::test]
fn keystrokes_reach_the_panel_and_requests_reach_the_host(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| SettingsView::new(panel("{}"), cx));
    let seen = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&seen);
    cx.update(|window, cx| {
        let focus = view.read(cx).focus.clone();
        window.focus(&focus, cx);
        cx.subscribe(&view, move |_, request: &Request, _| {
            sink.borrow_mut().push(request.clone())
        })
        .detach();
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("j space");
    view.update(cx, |view, _| {
        assert_eq!(
            view.panel
                .document()
                .get("rememberViewPerFolder")
                .and_then(flea::jsondoc::Json::as_bool),
            Some(true)
        );
    });
    cx.simulate_keystrokes("/ t h u m b");
    view.update(cx, |view, _| assert_eq!(view.panel.title(), "Settings"));
    cx.simulate_keystrokes("escape escape");
    assert_eq!(*seen.borrow(), [Request::Close]);
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

#[gpui::test]
fn every_section_draws_in_a_frame(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| SettingsView::new(panel("{}"), cx));
    for section in flea::ui_logic_settings_pages::pages::SECTIONS
        .iter()
        .map(|section| section.id)
        .chain(["columns"])
    {
        view.update(cx, |view, cx| {
            view.panel.show_section(section);
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
}

#[gpui::test]
fn settings_display_hint_wraps_before_the_chord_not_inside_it(cx: &mut TestAppContext) {
    use gpui::{px, SharedString, TextRun};
    let (view, cx) = cx.add_window_view(|_, cx| SettingsView::new(panel("{}"), cx));
    let raw =
        "Ctrl+Shift++ and Ctrl+Shift+- step through them; Ctrl+Shift+0 follows Omarchy again.";
    // The last chord's byte range in the raw text, the one the Display hint wraps on.
    let chord = (
        raw.rfind("Ctrl+Shift+0").unwrap(),
        raw.len() - " follows Omarchy again.".len(),
    );
    let mut raw_breaks = Vec::new();
    let mut packed: Vec<Vec<String>> = Vec::new();
    cx.update(|window, cx| {
        let face = view.read(cx).typeface.clone();
        let run = TextRun {
            len: raw.len(),
            font: face.font.clone(),
            color: Default::default(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        for width in (160..=420).step_by(8) {
            let lines = window
                .text_system()
                .shape_text(
                    SharedString::from(raw.to_owned()),
                    face.caption,
                    std::slice::from_ref(&run),
                    Some(px(width as f32)),
                    None,
                )
                .unwrap();
            for line in &lines {
                let glyphs: Vec<usize> = line
                    .runs()
                    .iter()
                    .flat_map(|run| run.glyphs.iter().map(|glyph| glyph.index))
                    .collect();
                for boundary in line.wrap_boundaries() {
                    raw_breaks.push(
                        glyphs
                            .get(boundary.glyph_ix + 1)
                            .copied()
                            .unwrap_or(raw.len()),
                    );
                }
            }
            // The pane's own packing at this width: units shaped whole, lines rejoined.
            let units = super::geometry::hint_units(raw);
            let widths: Vec<f32> = units
                .iter()
                .map(|unit| {
                    let run = TextRun {
                        len: unit.len(),
                        font: face.font.clone(),
                        color: Default::default(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    f32::from(
                        window
                            .text_system()
                            .shape_line(
                                SharedString::from(unit.clone()),
                                face.caption,
                                &[run],
                                None,
                            )
                            .width,
                    )
                })
                .collect();
            let mut lines: Vec<String> = Vec::new();
            let mut used = 0.;
            for (unit, unit_width) in units.iter().zip(&widths) {
                if used > 0. && used + unit_width > width as f32 {
                    lines.push(String::new());
                    used = *unit_width;
                } else {
                    used += unit_width;
                }
                if lines.is_empty() {
                    lines.push(String::new());
                }
                lines.last_mut().unwrap().push_str(unit);
            }
            packed.push(lines);
        }
    });
    // Control: the raw hint really does break inside the chord at some pane width.
    assert!(
        raw_breaks.iter().any(|at| *at > chord.0 && *at < chord.1),
        "the raw hint never breaks inside its chord, so this test proves nothing"
    );
    // Packed as the pane places it, the text is whole and every line holds the chord whole or not at all.
    assert!(
        packed.iter().all(|lines| lines.concat() == raw),
        "the packing lost hint text: {packed:?}"
    );
    assert!(
        packed
            .iter()
            .flatten()
            .all(|line| !line.contains("Shift+0") || line.contains("Ctrl+Shift+0")),
        "a packed line splits Ctrl+Shift+0: {packed:?}"
    );
    assert!(
        packed
            .iter()
            .flatten()
            .any(|line| line.contains("Ctrl+Shift+0")),
        "the chord was never placed by the packing, so this test proves nothing"
    );
}

#[gpui::test]
fn the_drawn_display_hint_places_its_chord_whole_on_one_line(cx: &mut TestAppContext) {
    use gpui::{px, size};
    let (view, cx) = cx.add_window_view(|_, cx| SettingsView::new(panel("{}"), cx));
    view.update(cx, |view, cx| {
        view.panel.show_section("display");
        cx.notify();
    });
    let mut wrapped_before = false;
    for width in (520..=900).step_by(20) {
        cx.simulate_resize(size(px(width as f32), px(760.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let chord = cx
            .debug_bounds("st1-hint-unit-Ctrl+Shift+0")
            .expect("the renderer places the chord as its own unit");
        let word = cx
            .debug_bounds("st1-hint-unit-follows")
            .expect("the renderer places each word as its own unit");
        let lead = cx
            .debug_bounds("st1-hint-unit-them;")
            .expect("the word before the chord is drawn");
        // The chord is one line tall wherever the pane wraps, never split across two.
        assert!(
            chord.size.height < word.size.height * 1.5,
            "Ctrl+Shift+0 wraps inside itself at {width}: {chord:?}"
        );
        wrapped_before |= chord.origin.y > lead.origin.y;
    }
    assert!(
        wrapped_before,
        "no width wrapped the hint before the chord, so this test proves nothing"
    );
}

#[gpui::test]
fn the_sheet_draws_its_groups_and_its_query(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| {
        SheetView::new(Sheet::new("default", english(), Sources::default()), cx)
    });
    cx.update(|window, cx| {
        let focus = view.read(cx).focus.clone();
        window.focus(&focus, cx);
        window.draw(cx).clear(cx);
    });
    cx.simulate_keystrokes("m u t e");
    view.update(cx, |view, _| assert_eq!(view.sheet.ranked.len(), 1));
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

// EY-422: at text size 16 the card grows with the text as v0.3.12's does, so the Scale hint keeps one line.
#[gpui::test]
fn ey422_the_card_grows_with_the_text_size_and_the_scale_hint_keeps_one_line(
    cx: &mut TestAppContext,
) {
    use gpui::{px, size};
    let (view, cx) = cx.add_window_view(|_, cx| SettingsView::new(panel("{}"), cx));
    view.update(cx, |view, cx| {
        view.typeface = crate::list::Typeface::omarchy_default().at_base(16);
        view.panel.show_section("display");
        cx.notify();
    });
    cx.simulate_resize(size(px(1100.), px(900.)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let first = cx
        .debug_bounds("st1-hint-unit-Flea")
        .expect("the Scale hint's first word is drawn");
    let last = cx
        .debug_bounds("st1-hint-unit-it.")
        .expect("the Scale hint's last word is drawn");
    assert_eq!(
        last.origin.y, first.origin.y,
        "the hint wraps it. onto a second line"
    );
    // 760 and 168 at 14 grow to 869 and 192 at 16: the pane is the card less its frame and the rail.
    view.update(cx, |view, _| {
        assert_eq!(view.panel.pane.0, 869. - 2. - 192.)
    });
}

/// KeymapSheet.qml elides a description to its half of the card; one too long for it ends inside its column, never at the edge.
#[gpui::test]
fn a_description_wider_than_its_column_ends_inside_it(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| {
        SheetView::new(Sheet::new("default", english(), Sources::default()), cx)
    });
    cx.simulate_resize(gpui::size(gpui::px(420.), gpui::px(700.)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let (text, face) = view.read_with(cx, |view, _| {
        (
            view.sheet.groups()[0].2[0].label.clone(),
            view.typeface.clone(),
        )
    });
    let whole = cx.update(|window, _| {
        let font = face.font.clone();
        crate::g2::status_feedback::caption::measure(
            text.clone().into(),
            font,
            face.caption,
            gpui::px(0.),
            window,
        )
    });
    let column = cx
        .debug_bounds("sheet-column-0")
        .expect("the left column is drawn");
    let label = cx
        .debug_bounds("sheet-label-0-0")
        .expect("the first description is drawn");
    assert!(
        whole > f32::from(column.right() - label.left()),
        "{text:?} must not fit its column at 420 px, so this test proves something"
    );
    assert!(
        label.right() <= column.right(),
        "{text:?} runs past its column: {label:?} in {column:?}"
    );
}
