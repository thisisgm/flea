//! Send with rclone (#176) against a fake backend that speaks the int4 rclone producer's lines.
use flea::i18n::{I18n, Paths};
use flea::ui_logic_sd2::present::Bar;
use flea_gui::g2::sd2::seams::Wire;
use flea_gui::g2::sd2::sweep::{Sweep, SweepBar, BOARD_AT, HEIGHT};
use flea_gui::g2::sd2::RcloneSend;
use flea_gui::palette::Theme;
use gpui::{
    div, px, AppContext, Context, IntoElement, ParentElement, Render, Styled, TestAppContext,
    Window,
};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;
use std::time::Duration;

/// The backend's stdin as the controller sees it: every line it queued, and whether the process is still there.
#[derive(Default)]
struct Fake {
    lines: RefCell<Vec<String>>,
    dead: Cell<bool>,
}

impl Wire for Fake {
    fn send(&self, line: String) -> bool {
        if self.dead.get() {
            return false;
        }
        self.lines.borrow_mut().push(line);
        true
    }
}

impl Fake {
    fn last(&self) -> String {
        self.lines.borrow().last().cloned().unwrap_or_default()
    }
    fn count(&self) -> usize {
        self.lines.borrow().len()
    }
}

struct Root(PathBuf);

impl Root {
    fn new() -> Self {
        let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/g2-sd2");
        std::fs::create_dir_all(&parent).unwrap();
        let output = Command::new("mktemp")
            .arg("-d")
            .arg(parent.join("test.XXXXXXXX"))
            .output()
            .unwrap();
        assert!(output.status.success());
        let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
        assert!(root.is_absolute());
        std::fs::write(root.join(".flea-test-sandbox"), "g2-sd2\n").unwrap();
        Self(root)
    }

    fn write(&self, path: &str, text: &str) {
        let path = self.0.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn i18n(&self, language: &str) -> I18n {
        self.write(
            "omarchy/default/i18n/locales.json",
            r#"{"en":{},"de":{"name":"Deutsch"}}"#,
        );
        self.write(
            "omarchy/default/i18n/en/shell.json",
            r#"{"common.cancel":"Cancel"}"#,
        );
        self.write(
            "omarchy/default/i18n/de/shell.json",
            r#"{"common.cancel":"Abbrechen"}"#,
        );
        self.write("language", language);
        I18n::load(Paths {
            omarchy: self.0.join("omarchy"),
            language_file: self.0.join("language"),
            catalogs: vec![self.0.join("catalog")],
        })
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        let path = self.0.to_string_lossy();
        if self.0.is_absolute()
            && path.contains("/.superpowers/g2-sd2/test.")
            && self.0.join(".flea-test-sandbox").is_file()
        {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

const PATHS: [&str; 3] = [
    "/fixture/DSC_4821.jpg",
    "/fixture/DSC_4822.jpg",
    "/fixture/DSC_4930.jpg",
];

fn paths() -> Vec<String> {
    PATHS.iter().map(|path| path.to_string()).collect()
}

fn id_of(line: &str) -> u64 {
    let at = line.find("\"id\":").unwrap() + 5;
    line[at..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .unwrap()
}

fn remotes(send: &mut RcloneSend, fake: &Fake) {
    send.menu_opened(fake);
    let id = id_of(&fake.last());
    assert!(send.on_line(&format!(r#"{{"t":"rcloneremotes","id":{id},"visible":true,"remotes":["gdrive","b2-photos","nas-crypt"]}}"#)));
}

fn job(id: u64, fields: &str) -> String {
    format!(
        r#"{{"t":"job","job":{{"id":{id},"kind":"send","head":"Sending","undo":false,{fields}}},"unfinished":1,"newest":{id},"head":"Sending"}}"#
    )
}

/// Opens the card on gdrive with the board's remembered folder, sends, and has the backend start job 12.
fn started(send: &mut RcloneSend, fake: &Fake) -> u64 {
    remotes(send, fake);
    send.remember("gdrive", "Uploads");
    assert!(send.choose("gdrive", paths()));
    assert!(send.submit(fake));
    let request = id_of(&fake.last());
    assert!(send.on_line(&format!(
        r#"{{"t":"rclonestarted","requestId":{request},"id":12,"ok":true,"error":""}}"#
    )));
    request
}

#[path = "g2_sd2/flow.rs"]
mod flow;

#[test]
fn every_word_follows_the_language() {
    let root = Root::new();
    root.write(
        "catalog/de/flea.json",
        r#"{"send.rclone.row":"Mit rclone senden","send.rclone.title":{"one":"%1 Objekt an %2 senden","other":"%1 Objekte an %2 senden"}}"#,
    );
    let i18n = root.i18n("de");
    let (mut send, fake) = (RcloneSend::default(), Fake::default());
    remotes(&mut send, &fake);
    assert_eq!(
        send.menu_row(&i18n, &[]).unwrap().label,
        "Mit rclone senden"
    );
    assert!(send.choose("gdrive", paths()));
    let card = send.card(&i18n).unwrap();
    assert_eq!(
        (
            card.title.as_str(),
            card.cancel.as_str(),
            card.primary.as_str()
        ),
        ("3 Objekte an gdrive senden", "Abbrechen", "Send")
    );
}

#[test]
fn blocks_are_built_once_per_change_and_per_language_not_per_frame() {
    let root = Root::new();
    root.write("catalog/de/flea.json", r#"{"send.rclone.checking":{"one":"%1 Objekt auf %2 prüfen","other":"%1 Objekte auf %2 prüfen"}}"#);
    let (english, german) = (root.i18n("en"), root.i18n("de"));
    let (mut send, fake) = (RcloneSend::default(), Fake::default());
    let request = started(&mut send, &fake);
    assert!(send.on_line(&job(
        12,
        r#""state":"running","phase":"checking","check":"checksum","done":3,"total":3"#
    )));
    let first = send.blocks(&english);
    for _ in 0..60 {
        assert!(
            Rc::ptr_eq(&first, &send.blocks(&english)),
            "a sweep frame reuses the words"
        );
    }
    assert_eq!(send.builds.get(), 1);
    assert_eq!(
        send.blocks(&german)[0].1.head,
        "3 Objekte auf gdrive prüfen"
    );
    assert_eq!(send.builds.get(), 2, "a language switch rebuilds");
    assert!(send.cancel(request, &fake));
    assert_eq!(
        send.blocks(&german)[0].1.cancel,
        Some(("Cancelling".into(), false))
    );
    assert_eq!(send.builds.get(), 3, "a change rebuilds");
    send.catalog_changed();
    send.blocks(&german);
    assert_eq!(send.builds.get(), 4, "a catalog reload rebuilds");
}

#[test]
fn a_retry_in_flight_offers_no_button_and_a_lost_backend_stops_the_sweep() {
    let root = Root::new();
    let i18n = root.i18n("en");
    let (mut send, fake) = (RcloneSend::default(), Fake::default());
    let request = started(&mut send, &fake);
    assert!(send.on_line(&job(12, r#""state":"failed","phase":"sending","done":1,"total":3,"reason":"Could not send DSC_4822.jpg to gdrive.""#)));
    assert!(send.retry(request, &fake));
    let block = send.blocks(&i18n)[0].1.clone();
    assert_eq!(
        (block.failed, block.retry, block.dismiss, block.cancel),
        (true, None, None, None),
        "nothing to press until the backend answers"
    );
    assert!(!send.dismiss(request, &fake) && !send.retry(request, &fake));
    assert!(send.on_line(&job(
        12,
        r#""state":"running","phase":"checking","check":"checksum","done":3,"total":3"#
    )));
    let sweep = send.sweep(request).unwrap();
    assert!(send.backend_ended());
    assert!(
        send.sweep(request).is_none() && send.blocks(&i18n)[0].1.bar == Bar::None,
        "the dead backend's check stops sweeping"
    );
    assert!(
        !send.on_line(&job(
            12,
            r#""state":"running","phase":"checking","check":"checksum","done":3,"total":3"#
        )),
        "the next backend's job 12 is another job"
    );
    drop(sweep);
}

struct Bench {
    sweep: Rc<Sweep>,
}

impl Render for Bench {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        // The theme roles stand in: the sweep asserts motion, never inks.
        let theme = Theme::default();
        let bar = SweepBar {
            sweep: Rc::clone(&self.sweep),
            track: theme.background,
            segment: theme.foreground,
        };
        div().w(px(366.)).h(px(40.)).child(bar)
    }
}

#[gpui::test]
fn the_sweep_moves_every_frame_while_shown_and_the_board_pin_asks_for_none(
    cx: &mut TestAppContext,
) {
    let sweep = Rc::new(Sweep::default());
    let handle = cx.add_window(|_, _| Bench {
        sweep: Rc::clone(&sweep),
    });
    cx.run_until_parked();
    let first = sweep.drawn();
    assert_eq!(first, (0., 0.), "the segment enters from the left edge");
    for _ in 0..20 {
        cx.executor().advance_clock(Duration::from_millis(16));
        cx.update_window(handle.into(), |_, window, cx| {
            window.simulate_next_frame(cx)
        })
        .unwrap();
    }
    let (frames, requests) = (sweep.counters.frames.get(), sweep.counters.requests.get());
    assert!(
        frames >= 20 && requests == frames,
        "one request per frame while it sweeps: {frames} {requests}"
    );
    let (left, width) = sweep.drawn();
    assert!(
        width > 0. && left >= 0. && left + width <= 1. + 1e-6,
        "{left} {width}"
    );
    sweep.pin(Some(BOARD_AT));
    handle.update(cx, |_, _, cx| cx.notify()).unwrap();
    cx.update_window(handle.into(), |_, window, cx| {
        window.simulate_next_frame(cx)
    })
    .unwrap();
    let pinned = sweep.counters.requests.get();
    assert_eq!(sweep.drawn(), (BOARD_AT, 1. / 3.));
    cx.executor().advance_clock(Duration::from_millis(160));
    for _ in 0..5 {
        cx.update_window(handle.into(), |_, window, cx| {
            window.simulate_next_frame(cx)
        })
        .unwrap();
    }
    assert_eq!(
        sweep.counters.requests.get(),
        pinned,
        "a pinned bar asks for no frame"
    );
    assert_eq!(
        HEIGHT, 6.,
        "rclone-checking.html draws the bar at height:6px"
    );
}
