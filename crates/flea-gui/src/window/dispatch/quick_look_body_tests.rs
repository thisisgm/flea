//! Space mounts every kind's own view in the Quick Look card, its first ask on slot "look"; a blob keeps the calm refusal.
use super::preview_tests::{body, close, context, draw, file, opened, press, sent, Lines};
use super::test_shell::{land, row, set_cursor};
use crate::{
    g2::{
        pv1::{doc::View, Body},
        pv3::engine::{feed, Backend, Feed, FeedSender, Playback, Request},
    },
    state::Shell,
    window::u07_tests::read,
};
use flea::{
    jsondoc::{self, Json},
    keys::Context,
    protocol::RowFacts,
    ui_logic_pv3::frame::{Command, Event},
};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::{cell::RefCell, rc::Rc};

#[derive(Default)]
pub(super) struct Script {
    starts: RefCell<Vec<Request>>,
    commands: Rc<RefCell<Vec<Command>>>,
    pub(super) feeds: RefCell<Vec<FeedSender>>,
}

pub(super) struct Fake(pub(super) Rc<Script>);
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
        let (sender, feed) = feed();
        self.0.feeds.borrow_mut().push(sender);
        Ok((Box::new(Decoder(self.0.commands.clone())), feed))
    }
}

/// One row per kind: the file, its icon, the kind previewKind reads and the ask its view sends first.
const KINDS: &[(&str, &str, &str, &str)] = &[
    ("notes.txt", "text-plain", "text", "code"),
    ("main.rs", "text-x-rust", "text", "code"),
    ("readme.md", "text-markdown", "text", "markdown-local"),
    ("page.html", "text-html", "text", "html"),
    ("table.csv", "text-csv", "text", "code"),
    ("report.docx", "x-office-document", "office", "officepdf"),
    ("book.pdf", "application-pdf", "pdf", "pdf-local"),
    ("Inter-Bold.ttf", "font-x-generic", "font", "fontlook"),
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
pub(super) fn mounted(
    cx: &mut TestAppContext,
) -> (Entity<Shell>, &mut VisualTestContext, Lines, Rc<Script>) {
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

pub(super) fn open_kind(shell: &Entity<Shell>, cx: &mut VisualTestContext, name: &str) {
    let index = 2 + KINDS
        .iter()
        .position(|(n, ..)| *n == name)
        .expect("a listed kind");
    set_cursor(shell, cx, index);
    cx.simulate_keystrokes("space");
    draw(cx);
    cx.simulate_frame_request(gpui::RequestFrameOptions::default());
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(
            flea::ui_logic_pv2::html::SETTLE_MS,
        ));
    draw(cx);
}

fn doc_kind(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Option<&'static str> {
    match body(shell, cx)? {
        Body::Doc(doc) => Some(doc.kind()),
        _ => None,
    }
}

fn check(cx: &mut TestAppContext, name: &str) {
    let (shell, cx, mut requests, script) = mounted(cx);
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
    if ask == "markdown-local" {
        let Some(Body::Doc(doc)) = body(&shell, cx) else {
            panic!("Markdown mounted")
        };
        let View::Markdown(md) = doc.view else {
            panic!("Markdown view")
        };
        cx.read(|cx| {
            let md = md.read(cx);
            assert_eq!(
                md.path.as_deref(),
                Some(std::path::Path::new("/files/readme.md"))
            );
            assert!(
                md.active && md.doc.seq > 0,
                "local Markdown read is scheduled"
            );
            assert_eq!(md.slot, "look");
        });
    } else if ask == "pdf-local" {
        let Some(Body::Doc(doc)) = body(&shell, cx) else {
            panic!("PDF mounted")
        };
        let View::Pdf(pdf) = doc.view else {
            panic!("PDF view")
        };
        assert_eq!(
            cx.read(|cx| pdf.read(cx).surface.opener.opened.clone()),
            "/files/book.pdf"
        );
        assert!(
            cx.read(|cx| pdf.read(cx).surface.doc.is_some()),
            "local PDF open reached its renderer"
        );
    } else {
        let first = lines
            .iter()
            .find(|doc| doc.get("c").and_then(Json::as_str) == Some(ask))
            .unwrap_or_else(|| panic!("{name} asks {ask}: {lines:?}"));
        let slot = first.get("slot").and_then(Json::as_str);
        if matches!(ask, "code" | "html" | "model3d") {
            assert_eq!(slot, Some("look"), "{name} must include the card's slot");
        } else {
            assert_eq!(slot, None, "{ask} uses request identity rather than a slot");
            let identity = if matches!(ask, "fontlook" | "officepdf" | "officesheets") {
                "id"
            } else {
                "token"
            };
            assert!(
                first
                    .get(identity)
                    .and_then(Json::as_f64)
                    .is_some_and(|id| id > 0.),
                "{name} has a request identity"
            );
        }
    }
    let (selector, drawn) = match kind {
        "audio" | "video" => ("pv1-media", true),
        "archive" => ("pv1-bar", false),
        _ => ("pv1-bar", true),
    };
    let found = cx.debug_bounds(selector).is_some();
    assert_eq!(found, drawn, "{name} draws {selector}: {drawn}");
    if matches!(kind, "audio" | "video") {
        let starts = script.starts.borrow();
        assert_eq!(starts.len(), 1);
        assert_eq!(starts[0].path, format!("/files/{name}"));
        assert!(starts[0].autoplay);
    }
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
fn w_wraps_the_open_text_and_tab_keys_stay_in_the_card(cx: &mut TestAppContext) {
    let (shell, cx, _requests, _) = mounted(cx);
    // A middle tab of three tells tabNext (to 2) from tabPrevious (to 0) and from a close.
    for (action, listing) in [("tabNew", 2), ("tabNew", 3), ("tabPrevious", 4)] {
        cx.update(|window, cx| {
            shell.update(
                cx,
                |shell, cx| assert!(shell.tab_action(action, window, cx)),
            )
        });
        land(&shell, cx, "/files", rows(), listing);
    }
    assert_eq!(strip(&shell, cx), (3, 1));
    open_kind(&shell, cx, "notes.txt");
    let wrap = |shell: &Entity<Shell>, cx: &mut VisualTestContext| match body(shell, cx) {
        Some(Body::Doc(doc)) => match &doc.view {
            View::Code(code) => cx.read(|cx| code.read(cx).wrap),
            _ => panic!("plain text mounts the code view"),
        },
        other => panic!("no mounted view: {other:?}"),
    };
    assert!(!wrap(&shell, cx));
    press(cx, "w");
    assert!(wrap(&shell, cx), "w toggles wrap in the card");
    assert_eq!(
        read(&shell, cx, "previewOpen", &[]),
        "true",
        "w never closes the card"
    );
    // EY-438: v0.3.12 keeps every tab key inside Quick Look, so the card stays and no tab opens or moves.
    for key in ["t", "ctrl-pagedown", "ctrl-t", "2"] {
        cx.simulate_keystrokes(key);
        draw(cx);
        assert_eq!(
            read(&shell, cx, "previewOpen", &[]),
            "true",
            "{key} keeps the card open"
        );
        assert_eq!(strip(&shell, cx), (3, 1), "{key} opens or moves no tab");
    }
    close(&shell, cx);
}

fn strip(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> (usize, usize) {
    shell.read_with(cx, |shell, _| {
        (shell.strip.tabs[0].count(), shell.strip.tabs[0].current())
    })
}

#[gpui::test]
fn media_plays_and_p_m_and_the_arrows_reach_the_player(cx: &mut TestAppContext) {
    let (shell, cx, _requests, script) = mounted(cx);
    open_kind(&shell, cx, "song.ogg");
    assert_eq!(script.starts.borrow().len(), 1, "Quick Look starts playing");
    assert!(script.starts.borrow()[0].autoplay);
    assert_eq!(context(&shell, cx), Context::Media);
    for event in [
        Event::Loaded {
            video: false,
            duration_ms: 20_000,
        },
        Event::Playing,
        Event::Position(10_000),
    ] {
        assert!(script.feeds.borrow()[0].event(event));
    }
    draw(cx);
    // EY-447: previewState is the player's word, as PreviewMedia.status, never the kind.
    assert_eq!(read(&shell, cx, "previewState", &[]), "playing");
    let muted = shell.read_with(cx, |_, cx| crate::g2::pv3::sound::MediaSound::get(cx).muted);
    for (key, command) in [
        ("p", Command::Pause),
        ("m", Command::Mute(!muted)),
        ("right", Command::Seek(15_000)),
        ("left", Command::Seek(10_000)),
    ] {
        script.commands.borrow_mut().clear();
        cx.simulate_keystrokes(key);
        draw(cx);
        assert_eq!(
            read(&shell, cx, "previewOpen", &[]),
            "true",
            "{key} keeps the card open"
        );
        assert!(
            script.commands.borrow().contains(&command),
            "{key} sends {command:?}, got {:?}",
            script.commands.borrow()
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
    assert_eq!(
        shell.read_with(cx, |shell, cx| shell
            .quick_look
            .as_ref()
            .unwrap()
            .view
            .read(cx)
            .subject()
            .unwrap()
            .name
            .clone()),
        "main.rs"
    );
    close(&shell, cx);
}
