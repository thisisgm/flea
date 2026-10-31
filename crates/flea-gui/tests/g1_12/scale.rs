//! The path surfaces' geometry follows the text size and is the board's at body 14.
use super::{
    jump::board,
    view::{ranked, Bar},
};
use flea_gui::{
    list::Typeface,
    path::{paint::Size, scale::Scale, suggestions::Popup},
    presentation::Fonts,
};
use gpui::{px, TestAppContext};

#[test]
fn the_geometry_follows_the_text_size_and_is_the_board_at_fourteen() {
    let at14 = board();
    let fourteen = (
        at14.field,
        at14.inset,
        at14.wash,
        at14.caret,
        at14.chrome,
        at14.row,
        at14.pad,
        at14.pad_x,
        at14.gap,
        at14.mark,
        at14.fade,
        at14.clear,
    );
    assert_eq!(
        fourteen,
        (13., 9., 16., 17., 27., 37., 5., 14., 9., 18., 31., 19.),
        "the board's pixels at body 14"
    );
    assert_eq!(at14.top(), 30., "3 px under the 27 px chrome");
    let typeface = Typeface {
        body: gpui::px(20.),
        body_small: gpui::px(18.),
        caption: gpui::px(17.),
        ..Typeface::omarchy_default()
    };
    let twenty = Scale::at(&typeface);
    let at20 = (
        twenty.field,
        twenty.inset,
        twenty.wash,
        twenty.caret,
        twenty.chrome,
        twenty.row,
        twenty.pad,
        twenty.pad_x,
        twenty.gap,
        twenty.mark,
        twenty.fade,
        twenty.clear,
    );
    assert_eq!(
        at20,
        (18., 13., 22., 23., 37., 52., 7., 20., 13., 26., 43., 27.),
        "the same formulas at body 20, the 10 px clear scaled to 14"
    );
    assert_eq!(
        (twenty.mark_top(), twenty.label_top(), twenty.wash_line()),
        (13., 14., 24.)
    );
    assert_eq!(twenty.top(), 40.);
    let short = Popup::measure(twenty, 360., 80., 788., 9);
    assert_eq!(
        (short.y, short.height),
        (40., 256.),
        "the room is the window less the top, the chrome and the 27 px clear"
    );
    let three = Popup::measure(twenty, 700., 80., 788., 3);
    assert_eq!(
        (three.y, three.height, three.scale.row),
        (40., 172., 52.),
        "3 rows of 52 and 7 px of padding each end, plus the frame"
    );
}

#[test]
fn the_mark_and_the_label_centre_on_whole_pixels_in_the_row() {
    let s = board();
    assert_eq!(
        (s.mark, s.mark_top()),
        (18., 10.),
        "an 18 px mark in 37 px centred on 9.5, rounded up to 10"
    );
    assert_eq!(
        (s.label_top(), s.wash_line()),
        (10., 17.),
        "the 16.8 px line box centred to 10, its run wash 17 tall"
    );
    let first_row = 30. + 1. + s.pad;
    assert_eq!(
        (first_row + s.mark_top(), first_row + s.label_top()),
        (46., 46.),
        "mark box and label line box both start on row 46"
    );
}

#[gpui::test]
fn the_field_and_the_popup_follow_the_text_size(cx: &mut TestAppContext) {
    use flea_gui::list::Typeface;
    let mut bar = ranked(cx, "o");
    let read = |bar: &mut Bar<'_>| {
        bar.read(|view| {
            (
                view.paint.borrow().pixels(Size::Field),
                view.paint.borrow().popup.expect("the popup drew"),
            )
        })
    };
    let (field, popup) = read(&mut bar);
    assert_eq!(
        (f32::from(field), popup.y, popup.height, popup.scale.row),
        (13., 30., 123., 37.),
        "the board at body 14"
    );
    bar.update(|view, _, cx| {
        view.paint.borrow_mut().typeface = Typeface {
            body: px(20.),
            body_small: px(18.),
            caption: px(17.),
            ..Typeface::omarchy_default()
        };
        cx.notify();
    });
    let (field, popup) = read(&mut bar);
    assert_eq!(
        (f32::from(field), popup.y, popup.height, popup.scale.row),
        (18., 40., 172., 52.),
        "the field and the popup at body 20"
    );
}

#[gpui::test]
fn the_launch_fonts_hand_the_field_the_windows_face_over_the_callers(cx: &mut TestAppContext) {
    let mut bar = ranked(cx, "o");
    let caller = Typeface {
        body: px(20.),
        body_small: px(18.),
        caption: px(17.),
        ..Typeface::omarchy_default()
    };
    let window_face = bar.update(|view, window, cx| {
        view.paint.borrow_mut().typeface = caller.clone();
        cx.set_global(Fonts::from_sources("", "", "monospace".into()));
        cx.notify();
        Typeface::for_window(window, cx)
    });
    assert_ne!(
        window_face, caller,
        "the window's face must differ for the check to bite"
    );
    assert_eq!(
        bar.read(|view| view.paint.borrow().typeface.clone()),
        window_face,
        "with the launch's fonts installed the field takes the window's face"
    );
}
