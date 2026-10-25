//! Space mounts every kind's own view in the Quick Look card, its first ask on slot "look"; a blob keeps the calm refusal.
use super::preview_tests::{body, close, context, draw, file, opened, sent, Lines};
use super::test_shell::{land, row, set_cursor};
use crate::{
    g2::{
        pv1::{doc::View, Body},
        pv3::engine::{feed, Backend, Feed, Playback, Request},
    },
    state::Shell,
    window::u07_tests::read,
};
use flea::{
    jsondoc::{self, Json},
    keys::Context,
    protocol::RowFacts,
    ui_logic_pv3::frame::Command,
};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::{cell::RefCell, rc::Rc};

#[derive(Default)]
struct Script {
    starts: RefCell<Vec<Request>>,
    commands: Rc<RefCell<Vec<Command>>>,
}

struct Fake(Rc<Script>);
struct Decoder(Rc<RefCell<Vec<Command>>>);

impl Playback for Decoder {
    fn send(&mut self, command: Command) {
        self.0.borrow_mut().push(command);
    }
    fn pids(&self) -> Vec<u32> {
        Vec::new()
    }
}

impl Backend for Fake {
    fn start(&self, request: &Request) -> Result<(Box<dyn Playback>, Feed), String> {
        self.0.starts.borrow_mut().push(request.clone());
        let (_sender, feed) = feed();
        Ok((Box::new(Decoder(self.0.commands.clone())), feed))
    }
}

/// One row per kind: the file, its icon, the kind previewKind reads and the ask its view sends first.
const KINDS: &[(&str, &str, &str, &str)] = &[
    ("notes.txt", "text-plain", "text", "code"),
    ("main.rs", "text-x-rust", "text", "code"),
    ("readme.md", "text-markdown", "text", ""),
    ("page.html", "text-html", "text", ""),
    ("table.csv", "text-csv", "text", "code"),
    ("report.docx", "x-office-document", "office", "officepdf"),
    ("book.pdf", "application-pdf", "pdf", ""),
    ("Inter-Bold.ttf", "font-x-generic", "font", ""),
    ("pack.zip", "package-x-generic", "archive", "meta"),
    ("benchy.stl", "model-stl", "model3d", "model3d"),
    ("benchy.gcode", "text-plain", "model3d", "model3d"),
    ("song.ogg", "audio-ogg", "audio", "meta"),
    ("clip.mp4", "video-mp4", "video", "meta"),
];

fn rows() -> Vec<RowFacts> {
    let mut rows = vec![
        row("alpha", true),
        file("disk.img", "application-octet-stream"),
    ];
    rows.extend(KINDS.iter().map(|(name, icon, _, _)| file(name, icon)));
    rows
}

/// The window with the card built once, its media playing through the scripted decoder.
fn mounted(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext, Lines, Rc<Script>) {
    // The Markdown parse, picture and PDF render threads are real, so their wakes come from outside the scheduler.
    cx.executor().allow_parking();
    let (shell, cx, mut requests) = opened(cx);
    land(&shell, cx, "/files", rows(), 1);
    set_cursor(&shell, cx, 1);
    cx.simulate_keystrokes("space");
    draw(cx);
    cx.simulate_keystrokes("escape");
    draw(cx);
    let script = Rc::new(Script::default());
    let fake = Rc::new(Fake(script.clone())) as Rc<dyn Backend>;
    shell.update(cx, |shell, _| {
        shell.quick_look.as_mut().unwrap().backend = Some(fake)
    });
    std::iter::from_fn(&mut requests).for_each(drop);
    (shell, cx, requests, script)
}

fn open_kind(shell: &Entity<Shell>, cx: &mut VisualTestContext, name: &str) {
    let index = 2 + KINDS
        .iter()
        .position(|(n, ..)| *n == name)
        .expect("a listed kind");
    set_cursor(shell, cx, index);
    cx.simulate_keystrokes("space");
    draw(cx);
}

fn doc_kind(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Option<&'static str> {
    match body(shell, cx)? {
        Body::Doc(doc) => Some(doc.kind()),
        _ => None,
    }
}

fn check(cx: &mut TestAppContext, name: &str) {
    let (shell, cx, mut requests, _) = mounted(cx);
    let (_, _, kind, ask) = *KINDS.iter().find(|(n, ..)| *n == name).unwrap();
    open_kind(&shell, cx, name);
    assert_eq!(
        read(&shell, cx, "previewOpen", &[]),
        "true",
        "{name} opens the card"
    );
    assert_eq!(
        doc_kind(&shell, cx),
        Some(kind),
        "{name} mounts its own view, not the refusal"
    );
    assert_eq!(read(&shell, cx, "previewKind", &[]), kind);
    assert_ne!(
        read(&shell, cx, "previewState", &[]),
        "This file cannot be previewed.",
        "{name} is never the calm refusal"
    );
    let lines: Vec<Json> = std::iter::from_fn(&mut requests)
        .filter_map(|line| jsondoc::parse(&line).ok())
        .collect();
    if !ask.is_empty() {
        let first = lines
            .iter()
            .find(|doc| doc.get("c").and_then(Json::as_str) == Some(ask))
            .unwrap_or_else(|| panic!("{name} asks {ask}: {lines:?}"));
        if let Some(slot) = first.get("slot").and_then(Json::as_str) {
            assert_eq!(slot, "look", "{name} asks on the card's slot");
        }
    }
    let surface = if matches!(kind, "audio" | "video") {
        "pv1-media"
    } else {
        "pv1-bar"
    };
    assert!(cx.debug_bounds(surface).is_some(), "{name} draws {surface}");
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert_eq!(
        read(&shell, cx, "previewOpen", &[]),
        "false",
        "Escape closes {name}"
    );
    close(&shell, cx);
}

macro_rules! kinds {
    ($($test:ident => $name:literal,)*) => {$(
        #[gpui::test]
        fn $test(cx: &mut TestAppContext) {
            check(cx, $name);
        }
    )*};
}

kinds! {
    plain_text_mounts_the_code_view_unnumbered => "notes.txt",
    code_mounts_the_code_view_numbered => "main.rs",
    markdown_mounts_the_markdown_view => "readme.md",
    html_mounts_the_html_view => "page.html",
    csv_mounts_the_table_view => "table.csv",
    office_mounts_the_office_view => "report.docx",
    pdf_mounts_the_pdf_view => "book.pdf",
    font_mounts_the_font_view => "Inter-Bold.ttf",
    archive_mounts_the_archive_view => "pack.zip",
    stl_mounts_the_model_view => "benchy.stl",
    gcode_mounts_the_model_view => "benchy.gcode",
    audio_mounts_the_player => "song.ogg",
    video_mounts_the_player => "clip.mp4",
}

#[gpui::test]
fn a_blob_keeps_the_calm_refusal_as_the_negative_control(cx: &mut TestAppContext) {
    let (shell, cx, mut requests, script) = mounted(cx);
    set_cursor(&shell, cx, 1);
    cx.simulate_keystrokes("space");
    draw(cx);
    assert_eq!(doc_kind(&shell, cx), None, "a blob mounts no view");
    assert!(matches!(body(&shell, cx), Some(Body::CannotShow { .. })));
    assert_eq!(
        read(&shell, cx, "previewState", &[]),
        "This file cannot be previewed."
    );
    assert!(sent(&mut requests, "code").is_empty());
    assert!(script.starts.borrow().is_empty());
    close(&shell, cx);
}

#[gpui::test]
fn w_wraps_the_open_text_and_a_tab_switch_closes_the_card(cx: &mut TestAppContext) {
    let (shell, cx, _requests, _) = mounted(cx);
    open_kind(&shell, cx, "notes.txt");
    let wrap = |shell: &Entity<Shell>, cx: &mut VisualTestContext| match body(shell, cx) {
        Some(Body::Doc(doc)) => match &doc.view {
            View::Code(code) => cx.read(|cx| code.read(cx).wrap),
            _ => panic!("plain text mounts the code view"),
        },
        other => panic!("no mounted view: {other:?}"),
    };
    assert!(!wrap(&shell, cx));
    cx.simulate_keystrokes("w");
    draw(cx);
    assert!(wrap(&shell, cx), "w toggles wrap in the card");
    assert_eq!(
        read(&shell, cx, "previewOpen", &[]),
        "true",
        "w never closes the card"
    );
    cx.simulate_keystrokes("ctrl-pagedown");
    draw(cx);
    assert_eq!(
        read(&shell, cx, "previewOpen", &[]),
        "false",
        "a tab switch closes the card"
    );
    assert_eq!(context(&shell, cx), Context::Listing);
    close(&shell, cx);
}

#[gpui::test]
fn media_plays_and_p_m_and_the_arrows_reach_the_player(cx: &mut TestAppContext) {
    let (shell, cx, _requests, script) = mounted(cx);
    open_kind(&shell, cx, "song.ogg");
    assert_eq!(script.starts.borrow().len(), 1, "Quick Look starts playing");
    assert!(script.starts.borrow()[0].autoplay);
    assert_eq!(context(&shell, cx), Context::Media);
    script.commands.borrow_mut().clear();
    for key in ["p", "m", "right", "left"] {
        cx.simulate_keystrokes(key);
        draw(cx);
        assert_eq!(
            read(&shell, cx, "previewOpen", &[]),
            "true",
            "{key} keeps the card open"
        );
    }
    cx.simulate_keystrokes("space");
    draw(cx);
    assert_eq!(
        read(&shell, cx, "previewOpen", &[]),
        "false",
        "Space closes the player"
    );
    close(&shell, cx);
}

#[gpui::test]
fn the_cursor_keys_still_move_under_a_mounted_view(cx: &mut TestAppContext) {
    let (shell, cx, _requests, _) = mounted(cx);
    open_kind(&shell, cx, "notes.txt");
    let at = read(&shell, cx, "cursor", &[]);
    cx.simulate_keystrokes("down");
    draw(cx);
    assert_ne!(
        read(&shell, cx, "cursor", &[]),
        at,
        "the cursor moved under the card"
    );
    assert_eq!(read(&shell, cx, "previewOpen", &[]), "true");
    assert_eq!(
        doc_kind(&shell, cx),
        Some("text"),
        "the card follows onto main.rs"
    );
    close(&shell, cx);
}
