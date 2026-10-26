use super::tests::*;
use crate::tui::{graphics::Graphics, preview, stage, wire};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

const STALL_LIMIT: Duration = Duration::from_secs(5);

#[test]
fn tui_office_stalled_identity_never_holds_the_loop() {
    let root = Fixture::new();
    let source = root.path(DOCX);
    let converted = root.path("stall.pdf");
    std::fs::write(&source, b"docx bytes").unwrap();
    std::fs::write(&converted, pdf(3, "", "")).unwrap();
    let mut m = model(&root, DOCX, "Word 2007 document");
    let gate = Arc::new(AtomicBool::new(false));
    m.preview_state.office.gate = Some(gate.clone());
    let (mut wire, reader) = wire::echo_wire();
    let (mut graphics, mut shown) = (Graphics::new(), stage::Shown::default());
    let started = Instant::now();
    step(&mut m, &mut graphics, &mut shown, &mut wire);
    assert!(
        started.elapsed() < STALL_LIMIT,
        "the input loop waited on a stalled identity"
    );
    let (id, path) = ask(&wire);
    assert_eq!(path, source.to_string_lossy());
    receive(&mut m, &mut wire, &ready(id, &converted));
    gate.store(true, Ordering::Relaxed);
    settle(|| {
        step(&mut m, &mut graphics, &mut shown, &mut wire);
        m.pdf.as_ref().is_some_and(|pdf| pdf.pages == 3)
    });
    drop(graphics);
    wire::finish(wire, reader);
}

#[test]
fn tui_office_stalled_sniff_is_left_behind_not_joined() {
    let root = Fixture::new();
    std::fs::write(
        root.path("sealed.docx"),
        [
            &[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1][..],
            &[0; 25][..],
        ]
        .concat(),
    )
    .unwrap();
    std::fs::write(root.path("notes.txt"), b"notes\n").unwrap();
    let mut m = model(&root, "sealed.docx", "Document");
    m.total = 2;
    m.rows
        .insert(1, row("notes.txt", "Plain text", "text-plain"));
    let gate = Arc::new(AtomicBool::new(false));
    m.preview_state.office.gate = Some(gate.clone());
    let (mut wire, reader) = wire::echo_wire();
    let (mut graphics, mut shown) = (Graphics::new(), stage::Shown::default());
    step(&mut m, &mut graphics, &mut shown, &mut wire);
    let (id, _) = ask(&wire);
    receive(
        &mut m,
        &mut wire,
        &format!(
            r#"{{"t":"officepdf","id":{id},"state":"failed","err":"the converter wrote no PDF"}}"#
        ),
    );
    let started = Instant::now();
    m.cursor = 1;
    preview::load(&mut m, true);
    step(&mut m, &mut graphics, &mut shown, &mut wire);
    assert!(
        started.elapsed() < STALL_LIMIT,
        "leaving a document waited on its stalled sniff"
    );
    gate.store(true, Ordering::Relaxed);
    settle(|| {
        step(&mut m, &mut graphics, &mut shown, &mut wire);
        m.preview_state.office.retired.is_empty()
    });
    drop(graphics);
    wire::finish(wire, reader);
}

#[test]
fn tui_office_rewritten_pdf_asks_again() {
    let root = Fixture::new();
    let source = root.path(DOCX);
    let converted = root.path("converted.pdf");
    std::fs::write(&source, b"docx bytes").unwrap();
    std::fs::write(&converted, pdf(3, "", "")).unwrap();
    let mut m = model(&root, DOCX, "Word 2007 document");
    let (mut wire, reader) = wire::echo_wire();
    let (mut graphics, mut shown) = (Graphics::new(), stage::Shown::default());
    step(&mut m, &mut graphics, &mut shown, &mut wire);
    let (first, _) = ask(&wire);
    receive(&mut m, &mut wire, &ready(first, &converted));
    settle(|| {
        step(&mut m, &mut graphics, &mut shown, &mut wire);
        m.pdf.as_ref().is_some_and(|pdf| pdf.pages == 3)
    });
    std::fs::write(&converted, pdf(2, "", "")).unwrap();
    settle(|| {
        step(&mut m, &mut graphics, &mut shown, &mut wire);
        m.pdf.is_none()
    });
    let (second, path) = ask(&wire);
    assert_ne!(second, first, "a rewritten PDF asks under a fresh id");
    assert_eq!(path, source.to_string_lossy());
    step(&mut m, &mut graphics, &mut shown, &mut wire);
    assert!(m.pdf.is_none(), "the page waits for the new reply");
    receive(&mut m, &mut wire, &ready(second, &converted));
    settle(|| {
        step(&mut m, &mut graphics, &mut shown, &mut wire);
        m.pdf.as_ref().is_some_and(|pdf| pdf.pages == 2)
    });
    drop(graphics);
    wire::finish(wire, reader);
}
