//! The keyboard at Flea's text sizes, in a real text system: its keys grow with the text, a window too small for
//! legends at that size asks for a larger one, and a search result's keys, a user's own chord among them, draw whole.
use super::super::SheetView;
use crate::g2::settings_pages::sheet::{Sheet, Sources};
use crate::g2::settings_pages::tests::english;
use crate::list::Typeface;
use crate::window::user_keys::test_fixture::samples;
use flea::keys::{CONTROL, SHIFT};
use flea::ui_logic_settings_pages::sheet_keyboard::{Bound, SheetKeyboard};
use gpui::{font, AppContext, Bounds, Pixels, TestAppContext, TestDispatcher};
use std::borrow::Cow;

// Layout places edges on fractional pixels, so a drawn edge may sit up to half a pixel past its parent's.
const SUBPIXEL: f32 = 0.5;

struct Drawn {
    unit: Option<f32>,
    too_small: bool,
    // The Settings result's keys, the box its drawn parts span and the key column box it sits in.
    settings: Option<(String, Bounds<Pixels>, Bounds<Pixels>)>,
}

// The user's own Ctrl+Shift+Y for Settings and, as if the built-in tables bound it, Ctrl+Shift+U.
const ONE: &[(&str, bool)] = &[("y", true), ("u", false)];
// As many of the user's keys for Settings as a row has no room for.
const MANY: &[(&str, bool)] = &[
    ("y", true),
    ("u", false),
    ("i", true),
    ("o", true),
    ("p", true),
];

/// The default preset's keyboard over the US fixture, with these Ctrl+Shift letters bound to Settings, by the user
/// or as if by the built-in tables.
fn keyboard(app: &mut TestAppContext, settings: &[(&str, bool)]) -> SheetKeyboard {
    let (backend, requests) = crate::backend::Backend::test_channel();
    std::mem::forget(requests);
    let shell = app.new(|cx| crate::state::tests::shell_with(backend, cx));
    let mut keyboard = shell
        .read_with(app, |shell, _| {
            shell.sheet_keyboard_from("default", false, samples)
        })
        .expect("the fixture samples a keymap");
    for &(key, user) in settings {
        let cap = keyboard.cap_of(key, Some(key)).expect("a letter cap");
        keyboard
            .bound
            .retain(|bound| bound.cap != cap || bound.mask != CONTROL | SHIFT);
        keyboard.bound.push(Bound {
            cap,
            mask: CONTROL | SHIFT,
            action: "settings".into(),
            label: "Settings".into(),
            user,
        });
    }
    keyboard
}

fn drawn(base: u32, width: f32, height: f32, query: &str, settings: &[(&str, bool)]) -> Drawn {
    let platform = gpui_platform::current_platform(true).text_system();
    let mut app = TestAppContext::build_with_text_system(TestDispatcher::new(0), None, platform);
    app.update(|cx| {
        cx.text_system()
            .add_fonts(vec![Cow::Borrowed(
                include_bytes!("../../../../tests/fonts/JetBrainsMono-Regular.ttf").as_slice(),
            )])
            .unwrap();
    });
    let keyboard = keyboard(&mut app, settings);
    let (view, cx) = app.add_window_view(|_, cx| {
        let mut view = SheetView::new(Sheet::new("default", english(), Sources::default()), cx);
        view.typeface = Typeface {
            font: font("JetBrains Mono"),
            ..Typeface::omarchy_default().at_base(base)
        };
        view.sheet.set_keyboard(Some(keyboard));
        if !query.is_empty() {
            view.sheet.typing = true;
            view.sheet.query_changed(query.into());
        }
        view
    });
    cx.simulate_resize(gpui::size(gpui::px(width), gpui::px(height)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let (unit, too_small, keys) = cx.update(|window, cx| {
        view.read_with(cx, |view, _| {
            let at = view
                .sheet
                .ranked
                .iter()
                .position(|row| row.action == "settings");
            let keys = at.map(|at| view.sheet.result_keys(at).to_owned());
            (view.unit(window), view.too_small(window), keys)
        })
    });
    let settings = keys.map(|keys| {
        let span = view
            .read_with(cx, |view, _| view.probe.bounds(&format!("keys:{keys}")))
            .unwrap_or_else(|| panic!("at {base} the {keys} chord is drawn"));
        let selector: &'static str = Box::leak(format!("sheet-keys-{keys}").into_boxed_str());
        let cell = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("at {base} the {keys} key column is drawn"));
        (keys, span, cell)
    });
    Drawn {
        unit,
        too_small,
        settings,
    }
}

#[test]
fn the_keys_grow_with_the_text_size() {
    assert_eq!(
        drawn(14, 2560., 1400., "", &[]).unit,
        Some(40.),
        "text size 14 keeps the board's unit"
    );
    // Text size 20's caption is 17, so the unit's cap of 40 grows to 56.
    assert_eq!(drawn(20, 2560., 1400., "", &[]).unit, Some(56.));
}

#[test]
fn a_window_too_small_for_legends_at_the_text_size_asks_for_a_larger_one() {
    let small = drawn(14, 700., 1000., "", &[]);
    assert!(
        small.unit.is_some_and(|unit| unit < 40.) && !small.too_small,
        "text size 14 fits a smaller keyboard"
    );
    // At text size 20 the same width would draw legends below the text around them.
    let large = drawn(20, 700., 1000., "", &[]);
    assert!(large.unit.is_none() && large.too_small);
}

#[test]
fn a_row_with_more_keys_than_fit_keeps_the_one_found_in_view() {
    for base in [14, 20] {
        let drawn = drawn(base, 2560., 1400., "ctrl-shift-y", MANY);
        let (keys, span, cell) = drawn
            .settings
            .unwrap_or_else(|| panic!("at {base} ctrl-shift-y finds Settings"));
        let chords: Vec<&str> = keys.split(" / ").collect();
        for user in ["Y", "I", "O", "P"] {
            assert!(
                chords.contains(&format!("CTRL + SHIFT + {user}").as_str()),
                "at {base} the user's {user} joins the row: {keys}"
            );
        }
        assert!(
            chords.len() > 4,
            "at {base} the built-in chord stays: {keys}"
        );
        let unique: std::collections::HashSet<_> = chords.iter().collect();
        assert_eq!(
            unique.len(),
            chords.len(),
            "at {base} no chord twice: {keys}"
        );
        // Too many chords for the column: it clips from the left, so the one the search typed reads last and whole.
        assert_eq!(
            chords.last(),
            Some(&"CTRL + SHIFT + Y"),
            "at {base}: {keys}"
        );
        assert!(
            f32::from(span.right()) <= f32::from(cell.right()) + SUBPIXEL,
            "at {base} {keys} ends past its {cell:?} column at {span:?}"
        );
    }
}

#[test]
fn a_users_key_joins_its_actions_search_row_whole() {
    for base in [14, 20] {
        let drawn = drawn(base, 2560., 1400., "ctrl-shift-y", ONE);
        let (keys, span, cell) = drawn
            .settings
            .unwrap_or_else(|| panic!("at {base} ctrl-shift-y finds Settings"));
        assert!(keys.ends_with(" / CTRL + SHIFT + Y"), "at {base}: {keys}");
        assert!(
            f32::from(span.left()) >= f32::from(cell.left()) - SUBPIXEL
                && f32::from(span.right()) <= f32::from(cell.right()) + SUBPIXEL,
            "at {base} {keys} spans {span:?}, past its {cell:?} column"
        );
    }
}

#[test]
fn a_built_in_key_of_a_listed_action_is_not_added_again() {
    let drawn = drawn(14, 2560., 1400., "ctrl-shift-y", ONE);
    let (keys, ..) = drawn.settings.expect("Settings is found");
    assert!(
        !keys.contains("CTRL + SHIFT + U"),
        "the built-in tables' chords are the row's own: {keys}"
    );
}
