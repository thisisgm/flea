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

struct Held(Arc<AtomicBool>);
impl Held {
    fn new(m: &mut crate::tui::model::Model) -> Self {
        let gate = Arc::new(AtomicBool::new(false));
        m.preview_state.office.gate = Some(gate.clone());
        Self(gate)
    }
    fn release(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
}
impl Drop for Held {
    fn drop(&mut self) {
        self.release();
    }
}

#[test]
fn tui_office_vfix1_capture_precedes_request_and_refuses_early_ready() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let root = Fixture::new();
    let source = root.path(DOCX);
    let old_pdf = root.path("a.pdf");
    let new_pdf = root.path("b.pdf");
    std::fs::write(&source, container(b"source A")).unwrap();
    std::fs::write(&old_pdf, pdf(1, "", "")).unwrap();
    std::fs::write(&new_pdf, pdf(2, "", "")).unwrap();
    let mut m = model(&root, DOCX, "Word 2007 document");
    let held = Held::new(&mut m);
    let generation = m.preview_generation;
    let (mut wire, reader) = wire::echo_wire();
    let (mut graphics, mut shown) = (Graphics::new(), stage::Shown::default());
    step(&mut m, &mut graphics, &mut shown, &mut wire);
    assert!(
        wire.events.recv_timeout(NO_REQUEST_WINDOW).is_err(),
        "officepdf was sent before source capture completed"
    );
    receive(&mut m, &mut wire, &ready(1, &old_pdf));
    assert!(m.preview_state.office.doc.pdf.is_none());
    assert!(m.preview_state.office.converted_open.is_none());
    std::fs::write(&source, container(b"source B with new bytes")).unwrap();
    held.release();
    settle(|| {
        step(&mut m, &mut graphics, &mut shown, &mut wire);
        m.preview_state.office.source.is_some() && m.preview_state.office.doc.id != 0
    });
    let (id, path) = ask(&wire);
    assert_eq!(path, source.to_string_lossy());
    assert!(m.preview_state.office.source.as_ref().unwrap().fresh());
    assert!(m.pdf.is_none() && m.preview_state.office.doc.pdf.is_none());
    receive(&mut m, &mut wire, &ready(id + 1, &old_pdf));
    assert!(m.preview_state.office.doc.pdf.is_none());
    receive(&mut m, &mut wire, &ready(id, &new_pdf));
    settle(|| {
        step(&mut m, &mut graphics, &mut shown, &mut wire);
        m.pdf.as_ref().is_some_and(|pdf| pdf.pages == 2)
    });
    assert_eq!(m.pdf.as_ref().unwrap().path, new_pdf);
    assert_eq!(m.preview_generation, generation);
    drop(graphics);
    wire::finish(wire, reader);
}

#[test]
fn tui_office_vfix1_rewrite_after_capture_never_adopts_old_page() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let root = Fixture::new();
    let source = root.path(DOCX);
    let old_pdf = root.path("a.pdf");
    let new_pdf = root.path("b.pdf");
    std::fs::write(&source, container(b"source A")).unwrap();
    std::fs::write(&old_pdf, pdf(1, "", "")).unwrap();
    std::fs::write(&new_pdf, pdf(2, "", "")).unwrap();
    let mut m = model(&root, DOCX, "Word 2007 document");
    let generation = m.preview_generation;
    let (mut wire, reader) = wire::echo_wire();
    let (mut graphics, mut shown) = (Graphics::new(), stage::Shown::default());
    settle(|| {
        step(&mut m, &mut graphics, &mut shown, &mut wire);
        m.preview_state.office.source.is_some() && m.preview_state.office.checked.is_some()
    });
    let (first, _) = ask(&wire);
    std::fs::write(&source, container(b"source B with new bytes")).unwrap();
    receive(&mut m, &mut wire, &ready(first, &old_pdf));
    settle(|| {
        step(&mut m, &mut graphics, &mut shown, &mut wire);
        assert!(m.pdf.is_none(), "PDF(A) was adopted for source B");
        m.preview_state.office.doc.id != 0 && m.preview_state.office.doc.id != first as u64
    });
    let (second, path) = ask(&wire);
    assert_eq!(path, source.to_string_lossy());
    receive(&mut m, &mut wire, &ready(first, &old_pdf));
    assert!(m.preview_state.office.doc.pdf.is_none());
    receive(&mut m, &mut wire, &ready(second, &new_pdf));
    settle(|| {
        step(&mut m, &mut graphics, &mut shown, &mut wire);
        m.pdf.as_ref().is_some_and(|pdf| pdf.pages == 2)
    });
    assert_eq!(m.pdf.as_ref().unwrap().path, new_pdf);
    assert_eq!(m.preview_generation, generation);
    drop(graphics);
    wire::finish(wire, reader);
}

#[test]
fn tui_office_vfix1_replaced_request_recaptures_before_retry() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let root = Fixture::new();
    let source = root.path(DOCX);
    let converted = root.path("b.pdf");
    std::fs::write(&source, container(b"source A")).unwrap();
    std::fs::write(&converted, pdf(2, "", "")).unwrap();
    let mut m = model(&root, DOCX, "Word 2007 document");
    let (mut wire, reader) = wire::echo_wire();
    let (mut graphics, mut shown) = (Graphics::new(), stage::Shown::default());
    settle(|| {
        step(&mut m, &mut graphics, &mut shown, &mut wire);
        m.preview_state.office.source.is_some() && m.preview_state.office.doc.id != 0
    });
    let (first, _) = ask(&wire);
    let held = Held::new(&mut m);
    receive(
        &mut m,
        &mut wire,
        &format!(
            r#"{{"t":"officepdf","id":{first},"state":"failed","err":"replaced by a newer request"}}"#
        ),
    );
    step(&mut m, &mut graphics, &mut shown, &mut wire);
    assert!(
        wire.events.recv_timeout(NO_REQUEST_WINDOW).is_err(),
        "retry was sent before source recapture completed"
    );
    receive(&mut m, &mut wire, &ready(first, &converted));
    receive(&mut m, &mut wire, &ready(first + 1, &converted));
    assert!(m.preview_state.office.doc.pdf.is_none());
    std::fs::write(&source, container(b"source B with new bytes")).unwrap();
    held.release();
    settle(|| {
        step(&mut m, &mut graphics, &mut shown, &mut wire);
        m.preview_state.office.doc.id != 0
    });
    let (second, _) = ask(&wire);
    assert_ne!(second, first);
    assert!(m.preview_state.office.doc.reasked);
    receive(&mut m, &mut wire, &ready(second, &converted));
    settle(|| {
        step(&mut m, &mut graphics, &mut shown, &mut wire);
        m.pdf.as_ref().is_some_and(|pdf| pdf.pages == 2)
    });
    assert!(
        wire.events.try_recv().is_err(),
        "recapture sent no third request"
    );
    drop(graphics);
    wire::finish(wire, reader);
}

#[test]
fn tui_office_stalled_identity_never_holds_the_loop() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let root = Fixture::new();
    let source = root.path(DOCX);
    let converted = root.path("stall.pdf");
    std::fs::write(&source, container(b"docx bytes")).unwrap();
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
    assert!(wire.events.recv_timeout(NO_REQUEST_WINDOW).is_err());
    gate.store(true, Ordering::Relaxed);
    let (id, path) = request(&mut m, &mut graphics, &mut shown, &mut wire);
    assert_eq!(path, source.to_string_lossy());
    receive(&mut m, &mut wire, &ready(id, &converted));
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
    let (mut wire, reader) = wire::echo_wire();
    let (mut graphics, mut shown) = (Graphics::new(), stage::Shown::default());
    step(&mut m, &mut graphics, &mut shown, &mut wire);
    let (id, _) = request(&mut m, &mut graphics, &mut shown, &mut wire);
    m.preview_state.office.gate = Some(gate.clone());
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
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let root = Fixture::new();
    let source = root.path(DOCX);
    let converted = root.path("converted.pdf");
    std::fs::write(&source, container(b"docx bytes")).unwrap();
    std::fs::write(&converted, pdf(3, "", "")).unwrap();
    let mut m = model(&root, DOCX, "Word 2007 document");
    let (mut wire, reader) = wire::echo_wire();
    let (mut graphics, mut shown) = (Graphics::new(), stage::Shown::default());
    step(&mut m, &mut graphics, &mut shown, &mut wire);
    let (first, _) = request(&mut m, &mut graphics, &mut shown, &mut wire);
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
    let (second, path) = request(&mut m, &mut graphics, &mut shown, &mut wire);
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
