//! F and B page a shown document, and p, m and the strip's play mark play media, through the production Shell.
use crate::{
    backend::Backend,
    g2::pv3::{
        engine::{self, FeedSender, Playback, Request},
        sound::MediaSound,
    },
    navigation::folder_state::Writer,
    state::Shell,
    window::{
        locale::Snapshot,
        selection_preview::{docs::Doc, Mounted},
        startup::{Arguments, Launch, Settings},
        u07_tests::{press, read},
    },
};
use flea::{
    protocol::RowFacts,
    ui_logic_pv3::frame::{Command, Event},
};
use gpui::{px, size, Entity, TestAppContext, VisualTestContext};
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};

type Lines = Box<dyn FnMut() -> Option<String>>;
thread_local! {
    static LINES: RefCell<Vec<Lines>> = RefCell::new(Vec::new());
}

/// The native constructor in the columns view, the backend replaced by one that keeps every request line.
pub(super) fn open<'a>(
    cx: &'a mut TestAppContext,
    path: &str,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    open_at(cx, path, None)
}

/// The same constructor at an explicit text size, the way a user's text setting reaches the window.
pub(super) fn open_at<'a>(
    cx: &'a mut TestAppContext,
    path: &str,
    rem: Option<f32>,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    let document = flea::jsondoc::parse(
        r#"{"keys":"default","view":"columns","preview":{"column":true},"places":{"rail":"shown","showHome":true,"showTrash":true}}"#,
    )
    .unwrap();
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
    let launch = Launch::resolve(
        Settings::from_json(document, true),
        Arguments {
            path: Some(path.into()),
            select: None,
        },
        &home,
    );
    let locale = Snapshot {
        revision: 1,
        catalog: Arc::new(flea::i18n::I18n::default()),
    };
    let (_sender, events) = async_channel::unbounded();
    LINES.with(|slot| slot.borrow_mut().clear());
    cx.update(|cx| {
        cx.set_global(crate::state::constructor_fixture::Services {
            backend: |_, _| {
                let (backend, lines) = Backend::test_lines();
                LINES.with(|slot| slot.borrow_mut().push(Box::new(lines)));
                Ok(backend)
            },
            monitor: None,
        })
    });
    let (shell, cx) =
        cx.add_window_view(|window, cx| {
            if let Some(rem) = rem {
                window.set_rem_size(px(rem));
            }
            Shell::new(launch, locale, events, window, cx)
        });
    shell.update(cx, |shell, _| {
        *shell.panes[0].as_ref().unwrap().nav.writer.borrow_mut() = Writer::default();
    });
    cx.simulate_resize(size(px(1100.), px(700.)));
    cx.run_until_parked();
    (shell, cx)
}

/// A file row as the rows reply carries it, Kind description included.
pub(super) fn file(name: &str, icon: &str, kind: &str, mode: u32) -> RowFacts {
    RowFacts {
        name: name.into(),
        size: 600,
        mtime: 1,
        mode,
        icon: icon.into(),
        kind: kind.into(),
        ..RowFacts::default()
    }
}

/// Frames and time for the settle and the swap, then every request line the backend received.
pub(super) fn pump(cx: &mut VisualTestContext) -> Vec<String> {
    for _ in 0..6 {
        cx.executor().advance_clock(Duration::from_millis(300));
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
    }
    LINES.with(|slot| {
        let mut out = Vec::new();
        for receive in slot.borrow_mut().iter_mut() {
            out.extend(std::iter::from_fn(&mut *receive));
        }
        out
    })
}

/// The folder's storage class answered local, which is what the column waits on before it loads, then the asks.
pub(super) fn local(shell: &Entity<Shell>, cx: &mut VisualTestContext, path: &str) -> Vec<String> {
    let mut lines = pump(cx);
    answer(
        shell,
        cx,
        &format!(r#"{{"t":"fsinfo","path":"{path}","fs":"tmpfs","dev":1,"total":100,"free":90,"class":"local"}}"#),
    );
    lines.extend(pump(cx));
    lines
}

/// One backend reply line, delivered the way the reader thread delivers it.
pub(super) fn answer(shell: &Entity<Shell>, cx: &mut VisualTestContext, line: &str) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(0, Backend::test_reply_line(line), window, cx)
        })
    });
}

/// The request whose command is `c`, and one of its numeric fields.
pub(super) fn ask<'a>(lines: &'a [String], c: &str) -> &'a str {
    let tag = format!(r#""c":"{c}""#);
    lines
        .iter()
        .rev()
        .find(|line| line.contains(&tag))
        .unwrap_or_else(|| panic!("the column asked {c}: {lines:?}"))
}
pub(super) fn field(line: &str, key: &str) -> u64 {
    flea::jsondoc::parse(line)
        .unwrap()
        .get(key)
        .and_then(flea::jsondoc::Json::as_f64)
        .unwrap_or_else(|| panic!("{key} in {line}")) as u64
}

/// The meta reply for the column's last meta ask, with the extra fields given.
pub(super) fn meta(shell: &Entity<Shell>, cx: &mut VisualTestContext, lines: &[String], extra: &str) {
    let asked = ask(lines, "meta");
    let (row, token) = (field(asked, "row"), field(asked, "token"));
    answer(shell, cx, &format!(r#"{{"t":"meta","row":{row},"token":{token}{extra}}}"#));
}

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
    assert!(paged > 1, "F pages the code by its frame height, not one row: {paged}");
    cx.simulate_keystrokes("shift-b->B");
    pump(cx);
    assert_eq!(code_top(&shell, cx), 0, "B pages back to the top");
}

/// A decoder that plays what it is told and keeps its feed, so the test moves the clock the way playback does.
#[derive(Clone, Default)]
struct Decoder {
    started: Rc<RefCell<usize>>,
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
        self.feed.borrow().as_ref().expect("playing").event(Event::Position(ms));
    }
}

fn position(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> i64 {
    read(shell, cx, "columnMediaPosition", &[]).parse().unwrap()
}

#[gpui::test]
fn p_the_play_mark_and_m_play_and_mute_a_column_clip(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "/files");
    let rows = vec![file("clip.mp4", "video-x-generic", "MPEG-4 video", 0o100644)];
    super::test_shell::land(&shell, cx, "/files", rows, 1);
    let decoder = Decoder::default();
    let backend: Rc<dyn engine::Backend> = Rc::new(decoder.clone());
    cx.update(|_, cx| {
        let preview = shell.read(cx).selection_preview(0).expect("the column is mounted");
        preview.update(cx, |view, _| view.set_media_backend(backend));
    });
    let lines = local(&shell, cx, "/files");
    assert!(ask(&lines, "meta").contains(r#""media":true"#));
    meta(&shell, cx, &lines, r#","w":2,"h":2,"ms":4000"#);
    answer(&shell, cx, r#"{"t":"thumbed","row":0,"file":""}"#);
    pump(cx);
    assert_eq!(read(&shell, cx, "previewColumnState", &[]), "video");
    assert_eq!(read(&shell, cx, "columnPlayerLoaded", &[]), "false");
    // Ctrl+Tab focuses the column in the media key context, where p plays.
    cx.simulate_keystrokes("ctrl-tab");
    cx.simulate_keystrokes("p");
    pump(cx);
    assert_eq!(*decoder.started.borrow(), 1, "p builds the player and starts it");
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
    assert_eq!(read(&shell, cx, "columnMediaPlaying", &[]), "true", "p resumes");
    decoder.at(1900);
    pump(cx);
    assert!(position(&shell, cx) > 700, "the clock moves on after the resume");
    let muted = |cx: &mut VisualTestContext| cx.update(|_, cx| MediaSound::get(cx).muted);
    assert!(!muted(cx));
    cx.simulate_keystrokes("m");
    pump(cx);
    assert!(muted(cx), "m mutes the clip");
    cx.simulate_keystrokes("m");
    pump(cx);
    assert!(!muted(cx), "m again unmutes it");
}
