//! F and B page a shown document, and p, m and the strip's play mark play media, through the production Shell.
pub(super) use super::previewtypes_fixture_tests::{
    answer, ask, field, file, local, meta, open, open_at, open_with_preview, pump, thumbed,
};
use crate::{
    g2::pv3::{
        engine::{self, FeedSender, Playback, Request},
        sound::MediaSound,
    },
    state::Shell,
    window::{
        selection_preview::{docs::Doc, Mounted},
        u07_tests::{press, read},
    },
};
use flea::ui_logic_pv3::frame::{Command, Event};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::{cell::RefCell, rc::Rc, sync::Arc};

fn code_top(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> usize {
    shell.read_with(cx, |shell, cx| {
        let preview = shell.selection_preview(0).expect("the column is mounted");
        let preview = preview.read(cx);
        match preview.drawn.as_ref().map(|frame| &frame.mounted) {
            Some(Mounted::Doc(Doc::Code(code))) => code.read(cx).top,
            _ => panic!("the code view is drawn"),
        }
    })
}

#[gpui::test]
fn a_code_row_asks_the_code_engine_and_f_and_b_page_it(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "/files");
    let rows = vec![file("main.rs", "text-x-rust", "Rust source code", 0o100644)];
    super::test_shell::land(&shell, cx, "/files", rows, 1);
    let lines = local(&shell, cx, "/files");
    let code = ask(&lines, "code").to_owned();
    assert!(
        code.contains(r#""slot":"column""#)
            && code.contains(r#""path":"/files/main.rs""#)
            && !code.contains("raw"),
        "the column's code view asks the styled lines: {code}"
    );
    assert!(ask(&lines, "meta").contains(r#""text":true"#));
    meta(&shell, cx, &lines, r#","lines":200"#);
    let body = (0..200)
        .map(|n| format!(r#"[5,"fn",0," line{n}()"]"#))
        .collect::<Vec<_>>()
        .join(",");
    answer(
        &shell,
        cx,
        &format!(
            r#"{{"t":"code","token":{},"slot":"column","state":"styled","first":{},"total":200,"lines":[{body}]}}"#,
            field(&code, "token"),
            field(&code, "first")
        ),
    );
    pump(cx);
    assert_eq!(read(&shell, cx, "previewColumnState", &[]), "code");
    assert!(read(&shell, cx, "columnTextLines", &[]).starts_with("fn line0()|fn line1()"));
    let facts = read(&shell, cx, "previewFacts", &[]);
    assert!(
        facts.starts_with("Kind=Rust source code|") && facts.contains("|Lines=200"),
        "Kind is the description and Lines the meta count: {facts}"
    );
    assert_eq!(code_top(&shell, cx), 0);
    cx.simulate_keystrokes("shift-f->F");
    pump(cx);
    let paged = code_top(&shell, cx);
    assert!(
        paged > 1,
        "F pages the code by its frame height, not one row: {paged}"
    );
    cx.simulate_keystrokes("shift-b->B");
    pump(cx);
    assert_eq!(code_top(&shell, cx), 0, "B pages back to the top");
}

/// A decoder that plays what it is told and keeps its feed, so the test moves the clock the way playback does.
#[derive(Clone, Default)]
pub(super) struct Decoder {
    pub(super) started: Rc<RefCell<usize>>,
    commands: Rc<RefCell<Vec<Command>>>,
    feed: Rc<RefCell<Option<FeedSender>>>,
}
struct Playing(Decoder);
impl engine::Backend for Decoder {
    fn start(&self, _: &Request) -> Result<(Box<dyn Playback>, engine::Feed), String> {
        *self.started.borrow_mut() += 1;
        let (sender, feed) = engine::feed();
        sender.event(Event::Loaded {
            video: true,
            duration_ms: 4000,
        });
        sender.frame(Arc::new(
            gpui::RenderImage::from_bgra(2, 2, vec![128; 16]).unwrap(),
        ));
        sender.event(Event::Playing);
        *self.feed.borrow_mut() = Some(sender);
        Ok((Box::new(Playing(self.clone())), feed))
    }
}
impl Playback for Playing {
    fn send(&mut self, command: Command) {
        let event = match command {
            Command::Play => Some(Event::Playing),
            Command::Pause => Some(Event::Paused),
            _ => None,
        };
        if let (Some(event), Some(feed)) = (event, self.0.feed.borrow().as_ref()) {
            feed.event(event);
        }
        self.0.commands.borrow_mut().push(command);
    }
    fn pids(&self) -> Vec<u32> {
        Vec::new()
    }
}
impl Decoder {
    fn at(&self, ms: i64) {
        self.feed
            .borrow()
            .as_ref()
            .expect("playing")
            .event(Event::Position(ms));
    }
}

fn position(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> i64 {
    read(shell, cx, "columnMediaPosition", &[]).parse().unwrap()
}

#[gpui::test]
fn p_the_play_mark_and_m_play_and_mute_a_column_clip(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "/files");
    let rows = vec![file(
        "clip.mp4",
        "video-x-generic",
        "MPEG-4 video",
        0o100644,
    )];
    super::test_shell::land(&shell, cx, "/files", rows, 1);
    let decoder = Decoder::default();
    let backend: Rc<dyn engine::Backend> = Rc::new(decoder.clone());
    cx.update(|_, cx| {
        let preview = shell
            .read(cx)
            .selection_preview(0)
            .expect("the column is mounted");
        preview.update(cx, |view, _| view.set_media_backend(backend));
    });
    let lines = local(&shell, cx, "/files");
    assert!(ask(&lines, "meta").contains(r#""media":true"#));
    meta(&shell, cx, &lines, r#","w":2,"h":2,"ms":4000"#);
    thumbed(&shell, cx, &lines, 0, "");
    pump(cx);
    assert_eq!(read(&shell, cx, "previewColumnState", &[]), "video");
    assert_eq!(read(&shell, cx, "columnPlayerLoaded", &[]), "false");
    // Ctrl+Tab focuses the column in the media key context, where p plays.
    cx.simulate_keystrokes("ctrl-tab");
    cx.simulate_keystrokes("p");
    pump(cx);
    assert_eq!(
        *decoder.started.borrow(),
        1,
        "p builds the player and starts it"
    );
    assert_eq!(read(&shell, cx, "columnMediaPlaying", &[]), "true");
    decoder.at(700);
    pump(cx);
    assert_eq!(position(&shell, cx), 700, "the clock moves while it plays");
    let centre = read(&shell, cx, "columnPlayCentre", &[]);
    press(cx, &centre, 1);
    pump(cx);
    assert_eq!(
        read(&shell, cx, "columnMediaPlaying", &[]),
        "false",
        "the play mark pauses"
    );
    assert_eq!(decoder.commands.borrow().last(), Some(&Command::Pause));
    cx.simulate_keystrokes("p");
    pump(cx);
    assert_eq!(
        read(&shell, cx, "columnMediaPlaying", &[]),
        "true",
        "p resumes"
    );
    decoder.at(1900);
    pump(cx);
    assert!(
        position(&shell, cx) > 700,
        "the clock moves on after the resume"
    );
    let muted = |cx: &mut VisualTestContext| cx.update(|_, cx| MediaSound::get(cx).muted);
    assert!(!muted(cx));
    cx.simulate_keystrokes("m");
    pump(cx);
    assert!(muted(cx), "m mutes the clip");
    cx.simulate_keystrokes("m");
    pump(cx);
    assert!(!muted(cx), "m again unmutes it");
}

#[gpui::test]
fn a_column_focused_before_its_clip_draws_takes_the_media_keys(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "/files");
    let rows = vec![file(
        "clip.mp4",
        "video-x-generic",
        "MPEG-4 video",
        0o100644,
    )];
    super::test_shell::land(&shell, cx, "/files", rows, 1);
    let backend: Rc<dyn engine::Backend> = Rc::new(Decoder::default());
    cx.update(|_, cx| {
        let preview = shell
            .read(cx)
            .selection_preview(0)
            .expect("the column is mounted");
        preview.update(cx, |view, _| view.set_media_backend(backend));
    });
    let lines = local(&shell, cx, "/files");
    // Focused while the clip still loads, so the context taken at focus is the plain preview one.
    cx.simulate_keystrokes("ctrl-tab");
    meta(&shell, cx, &lines, r#","w":2,"h":2,"ms":4000"#);
    thumbed(&shell, cx, &lines, 0, "");
    pump(cx);
    assert_eq!(read(&shell, cx, "previewColumnState", &[]), "video");
    cx.simulate_keystrokes("m");
    pump(cx);
    assert!(
        cx.update(|_, cx| MediaSound::get(cx).muted),
        "m mutes the clip drawn after the focus"
    );
}

// EY-206: the play mark and the strip are two drawn boxes, and a paused clip at its start reads position 0 as Ipc.qml's mediaPosition does.
#[gpui::test]
fn ey206_strip_centres_come_from_drawn_boxes_and_a_paused_clip_reads_zero(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "/files");
    let rows = vec![file(
        "clip.mp4",
        "video-x-generic",
        "MPEG-4 video",
        0o100644,
    )];
    super::test_shell::land(&shell, cx, "/files", rows, 1);
    let decoder = Decoder::default();
    let backend: Rc<dyn engine::Backend> = Rc::new(decoder.clone());
    cx.update(|_, cx| {
        let preview = shell
            .read(cx)
            .selection_preview(0)
            .expect("the column is mounted");
        preview.update(cx, |view, _| view.set_media_backend(backend));
    });
    let lines = local(&shell, cx, "/files");
    meta(&shell, cx, &lines, r#","w":2,"h":2,"ms":2000"#);
    thumbed(&shell, cx, &lines, 0, "");
    pump(cx);
    assert_eq!(read(&shell, cx, "columnPlayerLoaded", &[]), "false");
    assert_eq!(
        position(&shell, cx),
        0,
        "a clip with no player yet reads position 0"
    );
    cx.simulate_keystrokes("ctrl-tab");
    cx.simulate_keystrokes("p");
    pump(cx);
    let centre = read(&shell, cx, "columnPlayCentre", &[]);
    press(cx, &centre, 1);
    pump(cx);
    assert_eq!(
        read(&shell, cx, "columnMediaPlaying", &[]),
        "false",
        "the play mark pauses"
    );
    assert_eq!(
        position(&shell, cx),
        0,
        "a paused clip at its start reads position 0"
    );
    let strip = cx
        .update(|_, cx| {
            let preview = shell
                .read(cx)
                .selection_preview(0)
                .expect("the column is mounted");
            let view = preview.read(cx);
            let strip = *view.play_rect.borrow();
            strip.zip(view.caches.strip.borrow().play)
        })
        .expect("the strip and its play mark are drawn");
    let (strip, play) = strip;
    let (left, width) = (f32::from(strip.left()), f32::from(strip.size.width));
    let at = |value: String| -> (f32, f32) {
        let mut values = value.split_whitespace().map(|v| v.parse::<f32>().unwrap());
        (values.next().unwrap(), values.next().unwrap())
    };
    let (play_x, play_y) = at(read(&shell, cx, "columnPlayCentre", &[]));
    let (strip_x, strip_y) = at(read(&shell, cx, "columnStripCentre", &[]));
    assert!(
        strip_x > left && (strip_x - (left + width / 2.)).abs() <= 1.,
        "the strip centre is the middle of the drawn strip: {strip_x} in {strip:?}"
    );
    assert!(
        left < play_x && play_x < left + width / 4.,
        "the play centre is the drawn play mark, one slot at the strip's left: {play_x} in {strip:?}"
    );
    assert!(
        play_x < strip_x,
        "the play mark is not the strip's middle: {play_x} {strip_x}"
    );
    let painted = f32::from(play.left()) + f32::from(play.size.width) / 2.;
    assert!(
        (play_x - painted).abs() <= 1.,
        "the play centre is the middle of the painted play box: {play_x} in {play:?}"
    );
    assert!(
        (play_y - strip_y).abs() <= 1.,
        "both sit on the strip's middle row"
    );
}

/// The plain lines of the drawn frame's code view, and whether a pending frame waits behind it.
fn drawn_code(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> (Vec<String>, bool) {
    shell.read_with(cx, |shell, cx| {
        let preview = shell.selection_preview(0).expect("the column is mounted");
        let preview = preview.read(cx);
        match preview.drawn.as_ref().map(|frame| &frame.mounted) {
            Some(Mounted::Doc(Doc::Code(code))) => {
                (code.read(cx).lines.plain_lines(), preview.pending.is_some())
            }
            _ => panic!("the code view is drawn"),
        }
    })
}

#[gpui::test]
fn a_code_reply_for_the_pending_frame_leaves_the_drawn_lines_alone(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "/files");
    let rows = vec![
        file("a.rs", "text-x-rust", "Rust source code", 0o100644),
        file("b.rs", "text-x-rust", "Rust source code", 0o100644),
    ];
    super::test_shell::land(&shell, cx, "/files", rows, 1);
    let lines = local(&shell, cx, "/files");
    let code = ask(&lines, "code").to_owned();
    meta(&shell, cx, &lines, r#","lines":1"#);
    let reply = |token: u64, first: u64, text: &str| {
        format!(
            r#"{{"t":"code","token":{token},"slot":"column","state":"styled","first":{first},"total":1,"lines":[[5,"fn",0," {text}()"]]}}"#
        )
    };
    answer(
        &shell,
        cx,
        &reply(field(&code, "token"), field(&code, "first"), "drawn"),
    );
    pump(cx);
    assert_eq!(
        drawn_code(&shell, cx),
        (vec!["fn drawn()".to_owned()], false)
    );
    cx.simulate_keystrokes("down");
    let lines = pump(cx);
    let next = ask(&lines, "code").to_owned();
    assert_ne!(
        field(&next, "token"),
        field(&code, "token"),
        "b.rs asks with its own token"
    );
    answer(
        &shell,
        cx,
        &reply(field(&next, "token"), field(&next, "first"), "pending"),
    );
    pump(cx);
    assert_eq!(
        drawn_code(&shell, cx),
        (vec!["fn drawn()".to_owned()], true),
        "the pending frame's reply never reaches the drawn view while b.rs waits on its meta"
    );
    meta(&shell, cx, &lines, r#","lines":1"#);
    pump(cx);
    assert_eq!(
        drawn_code(&shell, cx),
        (vec!["fn pending()".to_owned()], false),
        "b.rs lands with the reply it took while pending"
    );
}
