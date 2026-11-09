// The picker test harness: a marked scratch root, children the test drives, and a window to run.
use super::process::{Line, PickerChild};
use super::*;
use crate::window::locale::Snapshot;
use async_channel::{Receiver, Sender};
use flea::ui_logic_picker_portal as logic;
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

// A marked scratch root: a test writes only inside it, and removal checks the marker first.
pub(crate) struct Root(pub(crate) PathBuf);

impl Root {
    pub(crate) fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "flea-pk1-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).expect("a fresh scratch root");
        std::fs::write(path.join(".flea-test-sandbox"), "").expect("the marker");
        Self(path)
    }
    pub(crate) fn reply(&self) -> PathBuf {
        self.0.join("reply.json")
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        let path = &self.0;
        if path.is_absolute()
            && path.starts_with(std::env::temp_dir())
            && path.join(".flea-test-sandbox").is_file()
        {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

// A child the test drives: what the picker sent, how often it quit, and the lines the test answers with.
#[derive(Clone)]
pub(crate) struct Fake {
    pub(crate) sent: Arc<Mutex<Vec<String>>>,
    pub(crate) quits: Arc<AtomicUsize>,
    pub(crate) answer: Sender<Line>,
    lines: Receiver<Line>,
}

impl Fake {
    pub(crate) fn new() -> Self {
        let (answer, lines) = async_channel::unbounded();
        Self {
            sent: Arc::default(),
            quits: Arc::default(),
            answer,
            lines,
        }
    }
    pub(crate) fn sent(&self) -> Vec<String> {
        self.sent.lock().unwrap().clone()
    }
    pub(crate) fn say(&self, line: Line) {
        self.answer.try_send(line).unwrap();
    }
}

impl PickerChild for Fake {
    fn send(&self, line: &str) -> bool {
        self.sent.lock().unwrap().push(line.to_owned());
        true
    }
    fn quit(&self) {
        self.quits.fetch_add(1, Ordering::Relaxed);
    }
    fn lines(&self) -> Receiver<Line> {
        self.lines.clone()
    }
}

pub(crate) struct Fixture<'a> {
    pub(crate) picker: Entity<Picker>,
    pub(crate) cx: &'a mut VisualTestContext,
    pub(crate) checks: Fake,
    pub(crate) listing: Fake,
    pub(crate) exits: Rc<Cell<u32>>,
    pub(crate) root: Root,
}

pub(crate) fn locale() -> Snapshot {
    Snapshot {
        revision: 1,
        catalog: Arc::new(flea::i18n::I18n::load(flea::i18n::Paths {
            omarchy: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../src/i18n/fixtures/omarchy"),
            language_file: "/nonexistent/flea-picker-language".into(),
            catalogs: Vec::new(),
        })),
    }
}

pub(crate) fn fixture<'a>(
    cx: &'a mut TestAppContext,
    request: &str,
    children: Option<Children>,
) -> Fixture<'a> {
    let (checks, listing) = (Fake::new(), Fake::new());
    let root = Root::new();
    let reply = root.reply();
    let req = logic::request(request);
    let exits = Rc::new(Cell::new(0));
    let counted = exits.clone();
    let children = children.unwrap_or_else(|| Children {
        checks: Box::new(checks.clone()),
        listing: Box::new(listing.clone()),
    });
    // Real reply writes and real children answer from their own threads.
    cx.executor().allow_parking();
    let (picker, cx) = cx.add_window_view(move |_, cx| {
        let mut picker = Picker::new(req, reply, "/home/test", locale(), children, cx);
        picker.on_exit(move |_| counted.set(counted.get() + 1));
        picker
    });
    Fixture {
        picker,
        cx,
        checks,
        listing,
        exits,
        root,
    }
}

impl Fixture<'_> {
    pub(crate) fn update<R>(
        &mut self,
        f: impl FnOnce(&mut Picker, &mut gpui::Context<Picker>) -> R,
    ) -> R {
        let picker = self.picker.clone();
        self.cx.update(|_, cx| picker.update(cx, f))
    }
    pub(crate) fn read<R>(&mut self, f: impl FnOnce(&Picker) -> R) -> R {
        let picker = self.picker.clone();
        self.cx.update(|_, cx| f(picker.read(cx)))
    }
    // Runs the window until `done` holds, letting real threads (the reply writer, real children) answer in between.
    pub(crate) fn settle(&mut self, done: impl Fn(&mut Self) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            self.cx.run_until_parked();
            if done(self) {
                return;
            }
            assert!(Instant::now() < deadline, "the picker did not settle");
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    pub(crate) fn written(&self) -> Option<String> {
        std::fs::read_to_string(self.root.reply()).ok()
    }
    pub(crate) fn quits(&self) -> (usize, usize) {
        (
            self.checks.quits.load(Ordering::Relaxed),
            self.listing.quits.load(Ordering::Relaxed),
        )
    }
}
