//! The chooser's save rows and settings face against the picker boards, through the real view.
use super::{
    process::Line,
    settings_parity_tests::draw,
    tests::fixture::{fixture, Fixture},
};
use flea::jsondoc::Json;
use gpui::TestAppContext;

#[gpui::test]
fn picker_parity_save_title_matches_the_board(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"mode":"save","folder":"/d","name":"sample-download.zip"}"#,
        None,
    );
    assert_eq!(
        f.read(|picker| picker.labels.title.to_string()),
        "Save File"
    );
}

#[gpui::test]
fn picker_parity_save_filename_has_the_board_width_and_selected_stem(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"mode":"save","folder":"/d","name":"sample-download.zip"}"#,
        None,
    );
    f.cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(700.)));
    draw(&mut f);
    let field = f.read(|picker| picker.browse.filename.clone().unwrap());
    assert_eq!(
        f32::from(f.cx.debug_bounds("pk1-filename").unwrap().size.width),
        300.
    );
    f.cx.update(|_, cx| {
        let field = field.read(cx);
        assert_eq!(field.entry.selected, 0..15);
        assert!(field.focused);
    });
}

#[gpui::test]
fn picker_parity_save_and_open_rows_use_the_board_height(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    f.listing.say(Line::Reply(
        r#"{"t":"listed","path":"/d","n":2,"v":1,"w":true}"#.into(),
    ));
    f.listing.say(Line::Reply(r#"{"t":"rows","start":0,"listing":1,"rows":[{"n":"a.txt","d":false,"s":3,"m":0,"p":33188,"i":"text-plain"},{"n":"b.txt","d":false,"s":5,"m":0,"p":33188,"i":"text-plain"}]}"#.into()));
    f.cx.run_until_parked();
    draw(&mut f);
    let listing = f.read(|picker| picker.browse.listing.clone());
    f.cx.update(|_, cx| assert_eq!(listing.read(cx).drawn().drawn.metrics.row, 37.));
}

#[gpui::test]
fn picker_parity_save_space_names_a_file_like_enter(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"mode":"save","folder":"/d","name":"draft.txt"}"#,
        None,
    );
    f.listing.say(Line::Reply(
        r#"{"t":"listed","path":"/d","n":2,"v":1,"w":true}"#.into(),
    ));
    f.listing.say(Line::Reply(r#"{"t":"rows","start":0,"listing":1,"rows":[{"n":"sub","d":true,"s":0,"m":0,"p":16877,"i":"folder"},{"n":"notes.txt","d":false,"s":3,"m":0,"p":33188,"i":"text-plain"}]}"#.into()));
    f.cx.run_until_parked();
    let focus = f.read(|picker| picker.focus.clone());
    f.cx.update(|window, cx| window.focus(&focus, cx));
    let sent = f.listing.sent().len();
    f.cx.simulate_keystrokes("space");
    f.cx.run_until_parked();
    let field = f.read(|picker| picker.browse.filename.clone().unwrap());
    f.cx.update(|_, cx| assert_eq!(field.read(cx).text(), "draft.txt"));
    assert!(!f.listing.sent()[sent..]
        .iter()
        .any(|line| line.contains(r#""c":"list""#)));
    f.update(|picker, cx| picker.move_cursor(1, cx));
    f.cx.update(|window, cx| window.focus(&focus, cx));
    f.cx.simulate_keystrokes("space");
    f.cx.run_until_parked();
    assert_eq!(
        f.read(|picker| picker.listing.save_name.clone()),
        "notes.txt"
    );
    f.cx.update(|_, cx| {
        let field = field.read(cx);
        assert_eq!(field.text(), "notes.txt");
        assert_eq!(field.entry.selected, 0..5);
    });
}

// The picker window keeps GPUI's 16 px rem; the settings face is installed as launch.rs installs it.
fn settings_face(f: &mut Fixture) -> crate::list::Typeface {
    f.cx.update(|window, cx| {
        window.set_rem_size(gpui::px(16.));
        let fonts = crate::presentation::Fonts::from_sources("", "", "monospace".into());
        let face = fonts.typeface(&Json::Null);
        cx.set_global(face.clone());
        cx.set_global(fonts);
        face
    })
}

#[gpui::test]
fn picker_parity_path_chrome_keeps_the_settings_face_in_a_default_rem_window(
    cx: &mut TestAppContext,
) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    let face = settings_face(&mut f);
    f.listing.say(Line::Reply(
        r#"{"t":"listed","path":"/d","n":0,"v":1,"w":true}"#.into(),
    ));
    f.cx.run_until_parked();
    draw(&mut f);
    let path = f.read(|picker| picker.browse.path.clone());
    f.cx.update(|_, cx| {
        let view = path.read(cx).path();
        assert_eq!(view.read(cx).paint.borrow().typeface, face);
    });
}

#[gpui::test]
fn picker_parity_rows_keep_the_settings_face_in_a_default_rem_window(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    let face = settings_face(&mut f);
    f.listing.say(Line::Reply(
        r#"{"t":"listed","path":"/d","n":2,"v":1,"w":true}"#.into(),
    ));
    f.listing.say(Line::Reply(r#"{"t":"rows","start":0,"listing":1,"rows":[{"n":"a.txt","d":false,"s":3,"m":0,"p":33188,"i":"text-plain"},{"n":"b.txt","d":false,"s":5,"m":0,"p":33188,"i":"text-plain"}]}"#.into()));
    f.cx.run_until_parked();
    draw(&mut f);
    let listing = f.read(|picker| picker.browse.listing.clone());
    f.cx.update(|_, cx| {
        let state = listing.read(cx).drawn();
        assert_eq!(state.typeface, face);
        let mut normal = crate::list::ListState::new(face.clone());
        normal.density = crate::list::Density::Normal;
        assert_eq!(state.drawn.metrics.row, normal.file_metrics().row);
    });
}
