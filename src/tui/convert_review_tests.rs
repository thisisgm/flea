//! Delayed validation and translated cell widths exercise the complete card renderer.
use super::{
    action_key_tests::drain,
    actions,
    convert_tests::{frame, opened},
    input::Key,
    keymap::Map,
    render::frame_tests::model,
    wire::{self, Wire},
};
use crate::{
    backend::testdir::TestDir,
    i18n::{I18n, Paths},
    jsondoc::parse,
};

fn validated_after_moving(w: &mut Wire, name: &str) -> super::model::Model {
    let mut m = model();
    m.rows.get_mut(&7).unwrap().name = name.into();
    actions::key(&mut m, &Key::character('F', ""), &Map::load(), w).unwrap();
    for _ in 0..7 {
        actions::key(&mut m, &Key::named("Down", ""), &Map::load(), w).unwrap();
    }
    drain(w);
    for line in [
        format!(
            r#"{{"t":"menuaction","op":"snapshot","id":{},"ok":true,"count":1}}"#,
            m.action_id
        ),
        format!(
            r#"{{"t":"menuaction","op":"validate","id":{},"action":"convert","ok":true,"paths":["/fx/v/work/{name}"]}}"#,
            m.action_id
        ),
    ] {
        m.receive(parse(&line).unwrap(), w).unwrap();
        drain(w);
    }
    m
}
#[test]
fn tui_convert_late_office_validation_cannot_keep_an_image_choice() {
    let (mut w, reader) = wire::echo_wire();
    let m = validated_after_moving(&mut w, "report.docx");
    assert!(frame(&m).contains("Convert report.docx to PDF"));
    assert_eq!(m.converting.card.as_ref().unwrap().choice, 0);
    wire::finish(w, reader);
}
#[test]
fn tui_convert_late_pdf_validation_cannot_keep_an_image_choice() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = validated_after_moving(&mut w, "report.pdf");
    m.converting.card.as_mut().unwrap().count = Some(3);
    assert!(frame(&m).contains("report (page 1).png"));
    assert_eq!(m.converting.card.as_ref().unwrap().choice, 0);
    wire::finish(w, reader);
}
#[test]
fn tui_convert_wide_output_label_preserves_every_path_character() {
    let dir = TestDir::new("tui-convert-wide-label");
    dir.dir("en");
    dir.file("en/flea.json", r#"{"dialogs.output.output":"输出 "}"#);
    let (mut w, reader) = wire::echo_wire();
    let mut m = opened(&mut w, "photo.png");
    m.i18n = I18n::load(Paths {
        omarchy: dir.join("omarchy"),
        language_file: dir.join("language"),
        catalogs: vec![dir.path().into()],
    });
    assert!(frame(&m).contains("输出 /fx/v/work/photo (converted).jpg"));
    wire::finish(w, reader);
}
// tui-convert and tui-convert-pdf: each format mark sits on Output's column, and a page range names its last file alone.
#[test]
fn tui_convert_rows_and_page_range_match_the_boards() {
    let (mut w, reader) = wire::echo_wire();
    let m = validated_after_moving(&mut w, "photo.png");
    let text = frame(&m);
    let lines: Vec<&str> = text.lines().collect();
    let column = |line: &str, needle: &str| line.find(needle).map(|at| line[..at].chars().count());
    let output = lines
        .iter()
        .find_map(|line| column(line, "Output"))
        .unwrap();
    let jpeg = lines.iter().find(|line| line.contains("JPEG")).unwrap();
    assert_eq!(column(jpeg, "▎"), Some(output - 2), "{jpeg}");
    let frame_edge = column(jpeg, "▎").unwrap() - 1;
    assert_eq!(
        jpeg.chars().nth(frame_edge),
        Some('│'),
        "the edge takes the first air cell: {jpeg}"
    );
    assert_eq!(column(jpeg, "\u{eaea} JPEG"), Some(output), "{jpeg}");
    let pdf = lines.iter().find(|line| line.contains(" PDF")).unwrap();
    assert_eq!(column(pdf, "\u{eaeb} PDF"), Some(output), "{pdf}");
    let mut m = validated_after_moving(&mut w, "report.pdf");
    m.converting.card.as_mut().unwrap().count = Some(3);
    let text = frame(&m);
    let to = text
        .lines()
        .find(|line| line.contains("(page 3).png"))
        .unwrap();
    assert!(
        to.contains("to report (page 3).png") && !to.contains("/fx/v/work/report (page 3)"),
        "{to}"
    );
    wire::finish(w, reader);
}
