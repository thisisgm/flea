//! At a larger text size the sheet's key column holds every chord whole and the card grows with the caption (S3-6).
use super::{fit, SheetView, KEY_COL};
use crate::g2::settings_pages::sheet::{Sheet, Sources};
use crate::g2::settings_pages::tests::english;
use crate::list::Typeface;
use gpui::{font, Bounds, Pixels, TestAppContext, TestDispatcher};
use std::borrow::Cow;

// Layout places edges on fractional pixels, so a drawn edge may sit up to half a pixel past its parent's.
const SUBPIXEL: f32 = 0.5;

struct Drawn {
    card: f32,
    column: f32,
    // Each chord with the box its drawn parts span and the key column box it sits in.
    chords: Vec<(String, Bounds<Pixels>, Bounds<Pixels>)>,
}

fn opened(base: u32) -> Drawn {
    let platform = gpui_platform::current_platform(true).text_system();
    let mut app = TestAppContext::build_with_text_system(TestDispatcher::new(0), None, platform);
    app.update(|cx| {
        cx.text_system()
            .add_fonts(vec![Cow::Borrowed(
                include_bytes!("../../../tests/fonts/JetBrainsMono-Regular.ttf").as_slice(),
            )])
            .unwrap();
    });
    let (view, cx) = app.add_window_view(|_, cx| {
        let mut view = SheetView::new(Sheet::new("default", english(), Sources::default()), cx);
        view.typeface = Typeface {
            font: font("JetBrains Mono"),
            ..Typeface::omarchy_default().at_base(base)
        };
        view
    });
    cx.simulate_resize(gpui::size(gpui::px(1600.), gpui::px(1000.)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let (card, column, keys) = view.read_with(cx, |view, _| {
        let keys: Vec<String> = view
            .sheet
            .groups()
            .iter()
            .flat_map(|(_, _, rows)| rows)
            .map(|row| view.sheet.cap(&row.keys).to_owned())
            .collect();
        let card = f32::from(view.probe.bounds("card").unwrap().size.width);
        (card, view.key_col, keys)
    });
    let chords = keys
        .into_iter()
        .map(|keys| {
            let span = view
                .read_with(cx, |view, _| view.probe.bounds(&format!("keys:{keys}")))
                .unwrap_or_else(|| panic!("at {base} the {keys} chord is drawn"));
            let selector: &'static str = Box::leak(format!("sheet-keys-{keys}").into_boxed_str());
            let cell = cx
                .debug_bounds(selector)
                .unwrap_or_else(|| panic!("at {base} the {keys} key column is drawn"));
            (keys, span, cell)
        })
        .collect();
    Drawn {
        card,
        column,
        chords,
    }
}

#[test]
fn the_key_column_holds_every_chord_at_every_text_size() {
    let mut last = 0.;
    for base in [14, 16, 20] {
        let drawn = opened(base);
        if base == 14 {
            assert_eq!(
                (drawn.card, drawn.column),
                (628., KEY_COL),
                "text size 14 keeps the board"
            );
        }
        // The parts SheetView::keys draws, read from the frame, end inside the column they sit in.
        for (keys, span, cell) in &drawn.chords {
            assert!(
                f32::from(span.left()) >= f32::from(cell.left()) - SUBPIXEL
                    && f32::from(span.right()) <= f32::from(cell.right()) + SUBPIXEL,
                "at {base} {keys} spans {span:?}, past its {cell:?} column"
            );
        }
        // The case S3-6 reported: at 20 a chord is wider than the board's column, so a 117px cell would cut it.
        if base == 20 {
            assert!(
                drawn
                    .chords
                    .iter()
                    .any(|(_, span, _)| f32::from(span.size.width) > KEY_COL),
                "at 20 no chord outgrows the board's {KEY_COL}px column"
            );
        }
        assert!(
            drawn.card >= last,
            "at {base} the card narrows from {last} to {}",
            drawn.card
        );
        last = drawn.card;
    }
    assert!(
        last > fit::MAX_SHEET_W,
        "at 20 the card outgrows the 642 cap, at {last}"
    );
}
