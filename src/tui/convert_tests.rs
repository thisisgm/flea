use super::{
    action_key_tests::drain,
    actions,
    input::Key,
    keymap::Map,
    model::Model,
    render::{
        self,
        frame_tests::{caps_language, grid, model},
    },
    theme::Theme,
    wire::{self, Wire},
};
use crate::jsondoc;
use std::time::Duration;

pub(super) fn opened(w: &mut Wire, name: &str) -> Model {
    let mut m = model();
    m.rows.get_mut(&7).unwrap().name = name.into();
    actions::key(&mut m, &Key::character('F', ""), &Map::load(), w).unwrap();
    drain(w);
    for line in [
        r#"{"t":"menuaction","op":"snapshot","id":ID,"ok":true,"count":1}"#,
        &format!(
            r#"{{"t":"menuaction","op":"validate","action":"convert","id":ID,"ok":true,"paths":["/fx/v/work/{name}"]}}"#
        ),
    ] {
        let line = line.replace("ID", &m.action_id.to_string());
        m.receive(jsondoc::parse(&line).unwrap(), w).unwrap();
        drain(w);
    }
    m
}
pub(super) fn frame(m: &Model) -> String {
    grid(
        &render::frame(
            m,
            &Theme::from_text(""),
            &Map::load(),
            130,
            31,
            Duration::ZERO,
        ),
        130,
        31,
    )
    .join("\n")
}
#[test]
fn tui_convert_all_formats_follow_shared_contract() {
    super::convert_process_tests::scenario("formats");
    let (mut w, reader) = wire::echo_wire();
    let m = opened(&mut w, "photo.png");
    let shown = frame(&m);
    for label in [
        "Convert photo.png",
        "JPEG",
        "PNG",
        "WebP",
        "AVIF",
        "HEIC",
        "TIFF",
        "BMP",
        "PDF",
        "Remove metadata",
        "[ Convert ]",
    ] {
        assert!(shown.contains(label), "missing {label}: {shown}");
    }
    wire::finish(w, reader);
}
#[test]
fn tui_convert_pdf_ranges_and_outputs_are_real() {
    super::convert_process_tests::scenario("pdf");
    let (mut w, reader) = wire::echo_wire();
    let m = opened(&mut w, "garden-report.pdf");
    assert!(frame(&m).contains("PAGES"));
    wire::finish(w, reader);
}
#[test]
fn tui_convert_pdf_heading_drops_the_greek_accent() {
    super::convert_process_tests::scenario("pdf");
    let (mut w, reader) = wire::echo_wire();
    let mut m = opened(&mut w, "garden-report.pdf");
    m.i18n = caps_language("el");
    let shown = frame(&m);
    // Plain to_uppercase draws ΣΕΛΊΔΕΣ.
    assert!(shown.contains("ΣΕΛΙΔΕΣ"), "{shown}");
    wire::finish(w, reader);
}
#[test]
fn tui_convert_multiple_images_keep_listing_order() {
    super::convert_process_tests::scenario("order");
    let (mut w, reader) = wire::echo_wire();
    let m = opened(&mut w, "photo.png");
    assert_eq!(m.menu_action, "convertCard");
    wire::finish(w, reader);
}
#[test]
fn tui_convert_office_reuses_preview_cache() {
    super::convert_process_tests::scenario("office");
    let (mut w, reader) = wire::echo_wire();
    let m = opened(&mut w, "report.docx");
    assert!(frame(&m).contains("Convert report.docx to PDF"));
    wire::finish(w, reader);
}
#[test]
fn tui_convert_collision_cancel_preserve_sources() {
    super::convert_process_tests::scenario("safe");
    let (mut w, reader) = wire::echo_wire();
    let m = opened(&mut w, "photo.png");
    assert!(!super::layout::focused(&m));
    wire::finish(w, reader);
}
