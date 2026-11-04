use crate::{
    backend::testdir::TestDir,
    jsondoc::{self, Json},
    tui::{
        model::{Model, Row},
        office, pane, preview,
        theme::Theme,
        wire,
    },
};
use std::time::{Duration, Instant};

#[test]
fn tui_parity17_hig_malformed_document_fails_before_conversion_and_draws_the_board_state() {
    let root = TestDir::new("tui17-invalid-office");
    root.file("letter.odt", "PK not really an office document\n");
    let mut m = Model::new(root.path().to_path_buf(), &Json::Null);
    m.total = 1;
    m.height = 28;
    m.columns = 130;
    let row =
        jsondoc::parse(r#"{"n":"letter.odt","s":33,"p":33188,"i":"x-office-document"}"#).unwrap();
    m.rows.insert(0, Row::parse(&row, &[]));
    preview::load(&mut m, true);
    let (mut w, reader) = wire::echo_wire();
    let deadline = Instant::now() + Duration::from_secs(10);
    while m.preview_state.unshown.is_none() {
        office::step(&mut m, &mut w, true).unwrap();
        if let Ok(Ok(value)) = w.events.try_recv() {
            assert_ne!(
                wire::text(&value, "c"),
                "officepdf",
                "a malformed container was sent to the converter"
            );
        }
        assert!(
            Instant::now() < deadline,
            "malformed Office source stayed blank"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        m.preview_state.unshown.as_ref().unwrap().sentence,
        "This document could not be previewed."
    );
    let mark = m.preview_state.unshown_mark.unwrap();
    assert_eq!(mark.glyph, pane::ALERT);
    assert!(mark.error);
    let theme = Theme::from_text("");
    let body = (0..28)
        .map(|row| pane::preview(&m, row, 50, 28, &theme))
        .collect::<String>();
    assert!(body.contains("This document could not be previewed."));
    assert!(body.contains("Open With"));
    wire::finish(w, reader);
}
