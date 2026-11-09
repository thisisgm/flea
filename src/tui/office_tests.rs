use crate::{
    jsondoc::{self, Json},
    tui::{
        graphics::Graphics,
        model::{Model, Row},
        pane,
        pane_state::{self, Kind},
        preview, stage,
        theme::{Depth, Theme},
        wire::{self, Wire},
    },
};
use flea::ui_logic_doc_preview::office::State as Subject;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[path = "office_test_container.rs"]
mod container_fixture;
pub(super) use container_fixture::container;

pub(super) const DOCX: &str = "garden-report.docx";
// How long a test waits to show that no request went out.
pub(super) const NO_REQUEST_WINDOW: Duration = Duration::from_millis(100);
pub(super) const KEYS: [&str; 3] = [
    "document.office_unsupported",
    "document.office_locked",
    "document.office_failed",
];
pub(super) const KEYS_CALM: &str = KEYS[0];
pub(super) const KEYS_LOCKED: &str = KEYS[1];

// Every test owns one mktemp root under this unit's evidence directory, marked for the sandbox guard.
pub(super) struct Fixture(pub(super) PathBuf);
impl Fixture {
    pub(super) fn new() -> Self {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join(".superpowers/m9-tui-07");
        std::fs::create_dir_all(&parent).unwrap();
        let output = std::process::Command::new("mktemp")
            .args(["-d"])
            .arg(parent.join("office.XXXXXXXX"))
            .output()
            .unwrap();
        assert!(output.status.success());
        let path = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
        assert!(path.is_absolute() && path.parent() == Some(parent.as_path()));
        std::fs::write(
            path.join(".flea-test-sandbox"),
            b"owned m9 TUI office tests\n",
        )
        .unwrap();
        Self(path)
    }
    pub(super) fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        assert!(
            self.0.is_absolute()
                && !self.0.as_os_str().is_empty()
                && self.0.join(".flea-test-sandbox").is_file()
        );
        assert!(self
            .0
            .starts_with(Path::new(env!("CARGO_MANIFEST_DIR")).join(".superpowers/m9-tui-07")));
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

// A minimal PDF with the given page count; Title and Author are written only when given, as a converter omits them.
pub(super) fn pdf(pages: usize, title: &str, author: &str) -> Vec<u8> {
    let kids: Vec<String> = (0..pages).map(|i| format!("{} 0 R", 4 + i)).collect();
    let mut info = String::from("<<");
    if !title.is_empty() {
        info += &format!(" /Title ({title})");
    }
    if !author.is_empty() {
        info += &format!(" /Author ({author})");
    }
    info += " >>";
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            kids.join(" "),
            pages
        ),
        info,
    ];
    objects.extend(
        (0..pages).map(|_| "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>".to_string()),
    );
    let mut out = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        out += &format!("{} 0 obj\n{}\nendobj\n", i + 1, object);
    }
    let xref = out.len();
    out += &format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for offset in offsets {
        out += &format!("{offset:010} 00000 n \n");
    }
    out += &format!(
        "trailer\n<< /Size {} /Root 1 0 R /Info 3 0 R >>\nstartxref\n{}\n%%EOF\n",
        objects.len() + 1,
        xref
    );
    out.into_bytes()
}

pub(super) fn row(name: &str, kind: &str, icon: &str) -> Row {
    Row {
        name: name.into(),
        directory: false,
        size: 8000,
        mode: 0o100644,
        link: String::new(),
        kind: kind.into(),
        thumbnail: false,
        modified: 0,
        birth: None,
        icon: icon.into(),
    }
}

pub(super) fn model(root: &Fixture, name: &str, kind: &str) -> Model {
    let mut m = Model::new(root.0.clone(), &Json::Null);
    m.height = 28;
    m.columns = 130;
    m.total = 1;
    m.rows.insert(0, row(name, kind, "x-office-document"));
    m.preview_state.cell = Some((8, 16));
    preview::load(&mut m, true);
    m
}

pub(super) fn step(
    m: &mut Model,
    graphics: &mut Graphics,
    shown: &mut stage::Shown,
    wire: &mut Wire,
) {
    stage::step(
        m,
        graphics,
        shown,
        wire,
        (130, 31),
        false,
        &mut String::new(),
    )
    .unwrap();
}

pub(super) fn settle(mut predicate: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(20);
    while !predicate() {
        assert!(Instant::now() < until, "office preview did not settle");
        std::thread::sleep(Duration::from_millis(5));
    }
}

// Drive the input loop until source capture has sent the request.
pub(super) fn request(
    m: &mut Model,
    graphics: &mut Graphics,
    shown: &mut stage::Shown,
    wire: &mut Wire,
) -> (usize, String) {
    settle(|| {
        step(m, graphics, shown, wire);
        m.preview_state.office.doc.id != 0
    });
    ask(wire)
}

// The next officepdf request on the wire: its id and the source path it converts.
pub(super) fn ask(wire: &Wire) -> (usize, String) {
    let value = wire
        .events
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    assert_eq!(wire::text(&value, "c"), "officepdf");
    (
        wire::count(&value, "id"),
        wire::text(&value, "path").to_string(),
    )
}

pub(super) fn receive(m: &mut Model, wire: &mut Wire, line: &str) {
    m.receive(jsondoc::parse(line).unwrap(), wire).unwrap();
}

pub(super) fn ready(id: usize, path: &Path) -> String {
    format!(
        r#"{{"t":"officepdf","id":{id},"state":"ready","path":"{}"}}"#,
        path.display()
    )
}

pub(super) fn screen(m: &Model) -> String {
    let theme = Theme::with_depth("", Depth::Truecolor);
    (0..31)
        .map(|y| pane::preview(m, y, 60, 31, &theme))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn tui_office_ready_pdf_keeps_original_identity() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let root = Fixture::new();
    let source = root.path(DOCX);
    let converted = root.path("converted.pdf");
    std::fs::write(&source, container(b"docx bytes")).unwrap();
    std::fs::write(&converted, pdf(3, "Spring report 2025", "Sam Ellis")).unwrap();
    let mut m = model(&root, DOCX, "Word 2007 document");
    let (mut wire, reader) = wire::echo_wire();
    let (mut graphics, mut shown) = (Graphics::new(), stage::Shown::default());
    assert_eq!(pane_state::kind(m.rows.get(&0).unwrap()), Kind::Office);
    step(&mut m, &mut graphics, &mut shown, &mut wire);
    let (id, path) = request(&mut m, &mut graphics, &mut shown, &mut wire);
    assert_eq!(path, source.to_string_lossy());
    receive(&mut m, &mut wire, &ready(id, &converted));
    settle(|| {
        step(&mut m, &mut graphics, &mut shown, &mut wire);
        m.pdf.as_ref().is_some_and(|pdf| pdf.pages == 3)
    });
    let pdf = m.pdf.as_ref().unwrap();
    assert_eq!(
        pdf.path, converted,
        "the page transport reads the converted file"
    );
    assert_eq!(
        pdf.source, source,
        "the page keeps the original file as its identity"
    );
    assert_eq!(pane_state::content(&m), Some(source.clone()));
    assert_eq!(m.rows.get(&0).unwrap().name, DOCX);
    let text = screen(&m);
    assert!(text.contains(DOCX) && text.contains("Pages · 3"), "{text}");
    assert!(
        text.contains("Title · Spring report 2025") && text.contains("Author · Sam Ellis"),
        "the document's own title and author draw as facts: {text}"
    );
    step(&mut m, &mut graphics, &mut shown, &mut wire);
    assert!(
        m.pdf.is_some(),
        "the same source keeps its page on later frames"
    );
    assert!(
        wire.events.try_recv().is_err(),
        "a ready source is not asked again"
    );
    drop(graphics);
    wire::finish(wire, reader);
}

#[test]
fn tui_office_cancel_ignores_late_ready() {
    let root = Fixture::new();
    let source = root.path(DOCX);
    let converted = root.path("late.pdf");
    std::fs::write(&source, container(b"docx bytes")).unwrap();
    std::fs::write(root.path("notes.txt"), b"notes\n").unwrap();
    std::fs::write(&converted, pdf(1, "", "")).unwrap();
    let mut m = model(&root, DOCX, "Word 2007 document");
    m.total = 2;
    m.rows
        .insert(1, row("notes.txt", "Plain text", "text-plain"));
    let (mut wire, reader) = wire::echo_wire();
    let (mut graphics, mut shown) = (Graphics::new(), stage::Shown::default());
    step(&mut m, &mut graphics, &mut shown, &mut wire);
    let (first, _) = request(&mut m, &mut graphics, &mut shown, &mut wire);
    m.cursor = 1;
    preview::load(&mut m, true);
    step(&mut m, &mut graphics, &mut shown, &mut wire);
    assert_eq!(
        m.preview_state.office.doc.state,
        Subject::Cancelled,
        "leaving the document cancels its request"
    );
    receive(&mut m, &mut wire, &ready(first, &converted));
    step(&mut m, &mut graphics, &mut shown, &mut wire);
    assert!(
        m.pdf.is_none() && m.preview_state.office.doc.pdf.is_none(),
        "a late ready for a left document draws nothing"
    );
    assert!(m.preview_state.unshown.is_none());
    m.cursor = 0;
    preview::load(&mut m, true);
    step(&mut m, &mut graphics, &mut shown, &mut wire);
    let (second, path) = request(&mut m, &mut graphics, &mut shown, &mut wire);
    assert_ne!(second, first, "returning asks under a fresh id");
    assert_eq!(path, source.to_string_lossy());
    receive(&mut m, &mut wire, &ready(first, &converted));
    step(&mut m, &mut graphics, &mut shown, &mut wire);
    assert!(
        m.pdf.is_none(),
        "the cancelled id never lands on the returned document"
    );
    receive(&mut m, &mut wire, &ready(second, &converted));
    settle(|| {
        step(&mut m, &mut graphics, &mut shown, &mut wire);
        m.pdf.is_some()
    });
    assert_eq!(m.pdf.as_ref().unwrap().source, source);
    drop(graphics);
    wire::finish(wire, reader);
}

#[test]
fn tui_office_cache_tracks_changed_source() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let root = Fixture::new();
    let source = root.path(DOCX);
    let first_pdf = root.path("first.pdf");
    let second_pdf = root.path("second.pdf");
    std::fs::write(&source, container(b"docx bytes")).unwrap();
    std::fs::write(&first_pdf, pdf(1, "", "")).unwrap();
    std::fs::write(&second_pdf, pdf(2, "", "")).unwrap();
    let mut m = model(&root, DOCX, "Word 2007 document");
    let (mut wire, reader) = wire::echo_wire();
    let (mut graphics, mut shown) = (Graphics::new(), stage::Shown::default());
    step(&mut m, &mut graphics, &mut shown, &mut wire);
    let (first, _) = request(&mut m, &mut graphics, &mut shown, &mut wire);
    receive(&mut m, &mut wire, &ready(first, &first_pdf));
    settle(|| {
        step(&mut m, &mut graphics, &mut shown, &mut wire);
        m.pdf.as_ref().is_some_and(|pdf| pdf.pages == 1)
    });
    settle(|| {
        step(&mut m, &mut graphics, &mut shown, &mut wire);
        m.preview_state.office.source.is_some()
    });
    std::fs::write(
        &source,
        container(b"docx bytes, edited in place with more bytes"),
    )
    .unwrap();
    settle(|| {
        step(&mut m, &mut graphics, &mut shown, &mut wire);
        m.pdf.is_none()
    });
    let (second, path) = request(&mut m, &mut graphics, &mut shown, &mut wire);
    assert_ne!(second, first);
    assert_eq!(path, source.to_string_lossy());
    receive(&mut m, &mut wire, &ready(first, &first_pdf));
    step(&mut m, &mut graphics, &mut shown, &mut wire);
    assert!(
        m.pdf.is_none(),
        "the old conversion cannot land on the edited source"
    );
    receive(&mut m, &mut wire, &ready(second, &second_pdf));
    settle(|| {
        step(&mut m, &mut graphics, &mut shown, &mut wire);
        m.pdf.as_ref().is_some_and(|pdf| pdf.pages == 2)
    });
    assert_eq!(m.pdf.as_ref().unwrap().path, second_pdf);
    let text = screen(&m);
    assert!(
        text.contains("Pages · 2") && !text.contains("Pages · 1"),
        "{text}"
    );
    drop(graphics);
    wire::finish(wire, reader);
}
