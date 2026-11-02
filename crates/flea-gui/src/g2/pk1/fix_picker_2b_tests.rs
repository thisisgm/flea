//! EY-548: the chooser draws its bands, rail, field and buttons at Flea's text size, as v0.3.12's chooser does.
use super::ipc::PickerGeometry;
use super::process::Line;
use super::tests::fixture::{fixture, Fixture};
use flea::jsondoc::{self, Json};
use gpui::{px, size, TestAppContext};

// The bands, the field, New Folder and the first rail row, in that order.
const MEASURED: [&str; 6] = [
    "picker-title-band",
    "picker-answers-band",
    "picker-status-band",
    "pk1-filename",
    "picker-new-folder",
    "picker-rail-row-0",
];

fn heights(f: &mut Fixture<'_>) -> [f32; 6] {
    MEASURED.map(|selector| {
        f32::from(
            f.cx.debug_bounds(selector)
                .unwrap_or_else(|| panic!("{selector} is drawn"))
                .size
                .height,
        )
    })
}

// Installs Omarchy's fonts and Flea's stored text size as the chooser's launch does, then draws a fresh frame.
fn install(f: &mut Fixture<'_>, settings: &str) -> u32 {
    let base = f.cx.update(|window, cx| {
        let fonts = crate::presentation::Fonts::from_sources("", "", "DejaVu Sans Mono".into());
        let settings: Json = jsondoc::parse(settings).unwrap();
        cx.set_global(fonts.typeface(&settings));
        let base = fonts.base;
        cx.set_global(fonts);
        window.refresh();
        base
    });
    f.cx.run_until_parked();
    f.read(|picker| picker.drawn.arm());
    f.cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    base
}

#[gpui::test]
fn fix_picker_2b_save_chooser_follows_flea_text_size(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"mode":"save","folder":"/d","name":"sa-d3-new.txt"}"#,
        None,
    );
    f.cx.simulate_resize(size(px(1100.), px(700.)));
    f.listing
        .say(Line::Reply(r#"{"t":"listed","n":0,"path":"/d"}"#.into()));
    f.cx.run_until_parked();
    let base = install(&mut f, "{}");
    let board = heights(&mut f);
    assert_eq!(
        board,
        [34., 59., 27., 30., 30., 29.],
        "following Omarchy's {base} px base draws the boards"
    );
    install(&mut f, r#"{"display":{"textSize":{"mode":20}}}"#);
    let ratio = 20. / base as f32;
    assert_eq!(
        heights(&mut f),
        board.map(|length| (length * ratio).round()),
        "at text size 20 every measured box scales by 20/{base}"
    );
    let members = f.read(|picker| picker.drawn.snapshot_members());
    assert!(
        members.contains(r#""body":20,"#),
        "the chooser's body type is 20 px: {members}"
    );
}
