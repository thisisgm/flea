//! Native text system, production Shell and a marked-root backend fixture.
use flea_gui::{
    state::Shell,
    window::{
        locale::Snapshot,
        startup::{Arguments, Launch, Settings, View},
    },
};
use gpui::{AppContext, TestAppContext, TestDispatcher, WindowHandle};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
static SERIAL: Mutex<()> = Mutex::new(());
const READY: Duration = Duration::from_secs(20);
#[path = "keys.rs"]
pub mod keys;
#[path = "host_watchdog.rs"]
mod watchdog;
use watchdog::Watchdog;

struct Host {
    cx: TestAppContext,
    window: WindowHandle<Shell>,
    root: PathBuf,
    view: View,
    _watchdog: Watchdog,
    previous_binary: Option<std::ffi::OsString>,
    events: async_channel::Sender<flea_gui::ipc::server::Event>,
}
impl Host {
    fn new(view: View) -> Self {
        let parent =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/m9-integ-00");
        std::fs::create_dir_all(&parent).unwrap();
        let output = std::process::Command::new("mktemp")
            .arg("-d")
            .arg(parent.join("consumer.XXXXXXXX"))
            .output()
            .unwrap();
        assert!(output.status.success());
        let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
        std::fs::write(root.join(".flea-test-sandbox"), "m9-integ-00\n").unwrap();
        std::fs::write(
            root.join("a.pdf"),
            super::pdf::pdf(&[
                super::pdf::LETTER,
                super::pdf::LANDSCAPE,
                super::pdf::TURNED,
            ]),
        )
        .unwrap();
        std::fs::write(root.join("backend"), include_str!("backend.py")).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(root.join("backend"), std::fs::Permissions::from_mode(0o700))
            .unwrap();
        let previous_binary = std::env::var_os("FLEA_BIN");
        std::env::set_var("FLEA_BIN", root.join("backend"));
        let mut cx = TestAppContext::build_with_text_system(
            TestDispatcher::new(0),
            None,
            gpui_platform::current_platform(true).text_system(),
        );
        cx.executor().allow_parking();
        let (events, receive) = async_channel::unbounded();
        let mut settings = Settings::from_json(
            flea::jsondoc::parse(
                r#"{"places":{"rail":"hidden"},"preview":{"column":true,"loadOn":"automatic"}}"#,
            )
            .unwrap(),
        );
        settings.view = view;
        let launch = Launch::resolve(
            settings,
            Arguments {
                path: Some(root.to_string_lossy().into()),
                select: None,
            },
            root.to_str().unwrap(),
        );
        let none = root.join("absent");
        let catalog = flea::i18n::I18n::load(flea::i18n::Paths {
            omarchy: none.clone(),
            language_file: none,
            catalogs: vec![crate::test_catalogs::english_only()],
        });
        let locale = Snapshot {
            revision: 1,
            catalog: Arc::new(catalog),
        };
        let window = cx.open_window(gpui::size(gpui::px(1100.), gpui::px(700.)), |window, cx| {
            Shell::new(launch, locale, receive, window, cx)
        });
        let mut host = Self {
            cx,
            window,
            root,
            view,
            _watchdog: Watchdog::start(),
            previous_binary,
            events,
        };
        let deadline = Instant::now() + READY;
        loop {
            host.draw();
            let mounted = host
                .window
                .update(&mut host.cx, |shell, _, _| {
                    shell.selection_preview(0).is_some()
                })
                .unwrap();
            // Before the first listing the window answers "Target not found." with exit 0, so readers wait for ready, within READY.
            if mounted && host.call("ready") == "true" {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the production pane was created and is ready"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        host
    }
    fn draw(&mut self) {
        self.cx.executor().advance_clock(Duration::from_millis(5));
        self.cx
            .update_window(self.window.into(), |_, window, cx| {
                window.simulate_next_frame(cx);
                window.refresh();
                window.draw(cx).clear(cx);
            })
            .unwrap();
        self.cx.run_until_parked();
    }
    fn call(&mut self, name: &str) -> String {
        self.call_args(name, &[])
    }
    fn call_args(&mut self, name: &str, arguments: &[&str]) -> String {
        let (answer, reply) = std::sync::mpsc::sync_channel(1);
        self.events
            .send_blocking(flea_gui::ipc::server::Event::Call {
                request: flea::ipc::wire::Request::Call {
                    target: "flea".into(),
                    function: name.into(),
                    arguments: arguments.iter().map(|arg| (*arg).into()).collect(),
                },
                answer,
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            self.draw();
            if let Ok(answer) = reply.try_recv() {
                assert_eq!(answer.reply.code, 0, "{}", answer.reply.stderr);
                return answer.reply.stdout.trim().into();
            }
            assert!(Instant::now() < deadline, "IPC {name} answered");
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    fn until(&mut self, name: &str, value: &str) {
        let deadline = Instant::now() + READY;
        loop {
            let actual = self.call(name);
            if actual == value {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "{name} reached {value}, actual {actual}; requests {}",
                std::fs::read_to_string(self.root.join("requests.jsonl")).unwrap_or_default()
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    fn select(&mut self, index: usize) {
        let current: usize = self.call("cursor").parse().unwrap();
        let (previous, next) = if self.view == View::Grid {
            ("h", "l")
        } else {
            ("k", "j")
        };
        for _ in index..current {
            self.cx.simulate_keystrokes(self.window.into(), previous);
        }
        for _ in current..index {
            self.cx.simulate_keystrokes(self.window.into(), next);
        }
        self.draw();
        assert_eq!(self.call("cursor"), index.to_string());
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        self.window
            .update(&mut self.cx, |shell, window, cx| {
                shell.close(window, cx);
            })
            .unwrap();
        match self.previous_binary.take() {
            Some(value) => std::env::set_var("FLEA_BIN", value),
            None => std::env::remove_var("FLEA_BIN"),
        }
    }
}

pub fn mounts() {
    let _lock = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut pdf_failures = Vec::new();
    {
        let mode = View::Columns;
        let mut host = Host::new(mode);
        let deadline = Instant::now() + Duration::from_secs(5);
        while host.call("columnPdfLoaded") != "true" && host.call("previewColumnState") != "error" {
            assert!(
                Instant::now() < deadline,
                "PDF reached a complete preview or its actual refusal"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        if host.call("columnPdfLoaded") == "true" {
            assert_eq!(host.call("columnPdfPages"), "3");
            keys::list_and_grid(&mut host);
        } else {
            let reason = match flea_gui::g2::doc_preview::poppler::Document::open(
                &host.root.join("a.pdf"),
            ) {
                Err(flea_gui::g2::doc_preview::poppler::Failure::Library(reason)) => reason,
                Err(_) => "unreadable fixture".into(),
                Ok(_) => "renderer failed with an available PDF library".into(),
            };
            pdf_failures.push(format!("{mode:?}: {reason}"));
        }
        host.select(1);
        host.until("previewColumnState", "video");
        assert_eq!(host.call("columnThumbShown"), "true");
        assert_eq!(host.call("columnPlayerLoaded"), "false");
        host.select(2);
        host.until("previewColumnState", "model3d");
        host.window
            .update(&mut host.cx, |shell, _, cx| {
                let preview = shell.selection_preview(0).unwrap();
                let preview = preview.read(cx);
                let (_, flea_gui::window::selection_preview::Mounted::Model(model)) =
                    preview.mounted_kind().unwrap()
                else {
                    panic!("actual Model3dView mounted");
                };
                let model = model.read(cx);
                assert_eq!(model.phase, flea_gui::g2::model_preview::view::Phase::Ready);
                assert_eq!(
                    model
                        .live
                        .borrow()
                        .model
                        .as_ref()
                        .unwrap()
                        .scene
                        .triangles
                        .len(),
                    1
                );
                assert!(
                    model.presented.borrow().image.is_some(),
                    "the bounded cache produced actual pixels"
                );
            })
            .unwrap();
    }
    assert!(
        pdf_failures.is_empty(),
        "PDF decode is BLOCKED: {}",
        pdf_failures.join("; ")
    );
}
pub fn readers() {
    let _lock = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut host = Host::new(View::Columns);
    host.select(3);
    host.until("previewColumnState", "text");
    assert_eq!(host.call("previewOpen"), "false");
    assert_eq!(
        host.call("previewSurfaceRect"),
        "",
        "closed Quick Look has no surface"
    );
    assert!(!host.call("columnFrameRect").is_empty());
    assert_eq!(
        host.call("columnTextLines"),
        "first actual line|second actual line"
    );
    host.select(4);
    host.until("previewColumnState", "image");
    assert_eq!(host.call("columnThumbShown"), "true");
    super::geometry::square_inside(
        &host.call("columnFrameRect"),
        &host.call("columnPictureRect"),
    );
    let requests = std::fs::read_to_string(host.root.join("requests.jsonl")).unwrap();
    assert!(requests.contains("\"code\"") && requests.contains("\"thumb\""));
    host.cx.simulate_keystrokes(host.window.into(), "ctrl-a");
    host.until("previewColumnState", "multi");
    assert_eq!(host.call("selectionCount"), "6");
    let facts = host.call("previewFacts");
    assert!(
        facts.starts_with("Kinds=3 files, 1 video, 1 text, 1 image|"),
        "{facts}"
    );
    assert!(
        facts.split('|').any(|row| row == "Combined=3.6 kB"),
        "{facts}"
    );
}
pub fn aba() {
    let _lock = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut host = Host::new(View::Columns);
    host.select(3);
    host.until("previewColumnState", "text");
    host.select(1);
    host.select(3);
    host.draw();
    std::thread::sleep(Duration::from_millis(150));
    host.until("previewColumnState", "text");
    assert_eq!(
        host.call("columnTextLines"),
        "first actual line|second actual line"
    );
    let requests = std::fs::read_to_string(host.root.join("requests.jsonl")).unwrap();
    let tokens: Vec<u64> = requests
        .lines()
        .filter_map(|line| flea::jsondoc::parse(line).ok())
        .filter(|d| d.get("c").and_then(flea::jsondoc::Json::as_str) == Some("meta"))
        .filter_map(|d| {
            d.get("token")
                .and_then(flea::jsondoc::Json::as_f64)
                .map(|n| n as u64)
        })
        .collect();
    assert!(tokens.len() >= 3 && tokens.windows(2).all(|pair| pair[0] != pair[1]));
}
pub fn releases() {
    use std::{rc::Rc, sync::atomic::Ordering};
    let _lock = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut host = Host::new(View::Columns);
    host.select(1);
    host.until("previewColumnState", "video");
    let decoder = Rc::new(super::media::Decoder::default());
    host.window
        .update(&mut host.cx, |shell, window, cx| {
            shell
                .selection_preview(0)
                .unwrap()
                .update(cx, |preview, cx| {
                    preview.set_media_backend(decoder.clone());
                    preview.play_pause(window, cx);
                });
        })
        .unwrap();
    host.until("columnMediaPlaying", "true");
    assert_eq!(decoder.started.load(Ordering::SeqCst), 1);
    host.window
        .update(&mut host.cx, |shell, window, cx| {
            shell.set_view(View::List, window, cx)
        })
        .unwrap();
    host.until("previewColumnState", "");
    assert_eq!(host.call("columnPlayerLoaded"), "false");
    assert_eq!(decoder.ended.load(Ordering::SeqCst), 1);
    assert_eq!(
        decoder.started.load(Ordering::SeqCst),
        1,
        "leaving Columns cannot restart playback"
    );
}
pub fn folder() {
    let _lock = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut host = Host::new(View::Columns);
    host.select(4);
    host.until("previewColumnState", "image");
    host.select(5);
    assert_eq!(
        host.call("columnThumbShown"),
        "true",
        "unanswered child keeps the complete image frame"
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while host.call_args("columnNames", &["child"]) != "seed.txt" {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        host.call("columnPdfPages"),
        "0",
        "folder listing replaces the complete preview"
    );
}
