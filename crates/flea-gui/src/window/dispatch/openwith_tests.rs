//! Enter on a file no application claims reaches flea --open, whose Open With ask opens the Open With card in the window (EY-84).
use super::test_shell::{land, open_root, row, sent, set_cursor, status};
use crate::backend::Event as BackendEvent;
use flea::protocol::RowFacts;
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::cell::RefCell;

thread_local! {
    static LAUNCHED: RefCell<Vec<Vec<String>>> = const { RefCell::new(Vec::new()) };
    static ANSWER: RefCell<(Option<i32>, String)> = const { RefCell::new((Some(0), String::new())) };
}

fn record(argv: &[String]) -> crate::g2::open_with_properties::lane::Exit {
    LAUNCHED.with(|launched| launched.borrow_mut().push(argv.to_vec()));
    ANSWER.with(|answer| answer.borrow().clone())
}

fn answering(status: Option<i32>, stdout: &str) {
    ANSWER.with(|answer| *answer.borrow_mut() = (status, stdout.into()));
}

fn launched() -> Vec<Vec<String>> {
    LAUNCHED.with(|launched| std::mem::take(&mut *launched.borrow_mut()))
}

fn rows() -> Vec<RowFacts> {
    vec![row("alpha", true), row("mystery.xyz", false)]
}

// The production Shell on /files through the real Root, with the test launcher on the opener.
fn shell(cx: &mut TestAppContext) -> (Entity<crate::state::Shell>, &mut VisualTestContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", rows(), 1);
    shell.update(cx, |shell, _| {
        for lane in &mut shell.opener {
            lane.launch = record;
        }
    });
    launched();
    sent();
    answering(Some(0), "");
    cx.update(|window, cx| shell.update(cx, |shell, cx| window.focus(&shell.focus, cx)));
    (shell, cx)
}

fn deliver(shell: &Entity<crate::state::Shell>, cx: &mut VisualTestContext, line: &str) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(0, BackendEvent::Producer(line.into()), window, cx)
        })
    });
    cx.run_until_parked();
}

fn card_title(shell: &Entity<crate::state::Shell>, cx: &mut VisualTestContext) -> Option<String> {
    let catalog = crate::g2::open_with_properties::tests::english();
    shell.read_with(cx, |shell, _| {
        shell
            .open_with
            .as_ref()
            .map(|open| open.card.card(&catalog).title)
    })
}

fn quit(shell: &Entity<crate::state::Shell>, cx: &mut VisualTestContext) {
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// The card opens on mystery.xyz, asks the backend for its snapshot, and its applications once that is held, under one id.
fn opened(cx: &mut TestAppContext) -> (Entity<crate::state::Shell>, &mut VisualTestContext, u64) {
    let (shell, cx) = shell(cx);
    set_cursor(&shell, cx, 1);
    answering(Some(5), "");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        launched()
            .iter()
            .map(|argv| argv[2].clone())
            .collect::<Vec<_>>(),
        ["/files/mystery.xyz"],
        "Enter hands the unclaimed file to flea --open"
    );
    let id_of = |lines: &[String], op: &str| {
        lines
            .iter()
            .filter_map(|line| flea::jsondoc::parse(line).ok())
            .find(|doc| doc.get("op").and_then(flea::jsondoc::Json::as_str) == Some(op))
            .and_then(|doc| doc.get("id").and_then(flea::jsondoc::Json::as_f64))
            .map(|id| id as u64)
    };
    let lines = sent();
    let snapshot = id_of(&lines, "snapshot").expect("the card snapshots its row first");
    // SW-03: the backend's one menu slot refuses a second queued request, so the catalogue waits for the snapshot.
    assert_eq!(
        id_of(&lines, "applications"),
        None,
        "the applications ask waits for the snapshot's answer: {lines:?}"
    );
    deliver(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","id":{snapshot},"op":"snapshot","ok":true,"kinds":[33188]}}"#
        ),
    );
    let lines = sent();
    assert_eq!(
        id_of(&lines, "applications"),
        Some(snapshot),
        "the applications ask carries the snapshot's id"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains(r#""installed":true"#)),
        "the card asks for the whole catalogue: {lines:?}"
    );
    (shell, cx, snapshot)
}

fn answer(shell: &Entity<crate::state::Shell>, cx: &mut VisualTestContext, id: u64) {
    deliver(
        shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","id":{id},"op":"applications","ok":true,"path":"/files/mystery.xyz","kind":"Data","mime":"application/x-mystery","applications":[{{"id":"nvim.desktop","label":"Neovim","icon":"nvim","default":true}}],"installed":[{{"id":"btop.desktop","label":"btop++","icon":"btop","default":false}}]}}"#
        ),
    );
}

#[gpui::test]
fn enter_on_an_unclaimed_file_opens_the_open_with_card(cx: &mut TestAppContext) {
    let (shell, cx, id) = opened(cx);
    assert!(cx.debug_bounds("op3-card").is_none());
    assert!(cx.debug_bounds("op3-openwith-scrim").is_none());
    answer(&shell, cx, id);
    assert_eq!(
        card_title(&shell, cx).as_deref(),
        Some("Open mystery.xyz With")
    );
    assert!(
        cx.debug_bounds("op3-card").is_some(),
        "the Open With card is drawn in the window"
    );
    let catalog = crate::g2::open_with_properties::tests::english();
    let labels = shell.read_with(cx, |shell, _| {
        shell.open_with.as_ref().map(|open| {
            open.card
                .card(&catalog)
                .body
                .iter()
                .filter_map(|block| match block {
                    crate::g2::open_with_properties::shell::Block::Apps(rows, _) => Some(
                        rows.iter()
                            .filter_map(|row| match row {
                                crate::g2::open_with_properties::shell::AppRow::App {
                                    label,
                                    ..
                                } => Some(label.clone()),
                                _ => None,
                            })
                            .collect::<Vec<_>>(),
                    ),
                    _ => None,
                })
                .next()
        })
    });
    assert_eq!(
        labels,
        Some(Some(vec!["Neovim".into(), "btop++".into()])),
        "the reply fills the card's list"
    );
    quit(&shell, cx);
}

#[gpui::test]
fn the_open_with_card_opens_the_cursor_app_and_closes_on_the_backends_yes(cx: &mut TestAppContext) {
    let (shell, cx, id) = opened(cx);
    answer(&shell, cx, id);
    sent();
    // Search, list, the always box, Cancel, Open: the board's Tab walk, with Space ticking the box.
    cx.simulate_keystrokes("tab tab space tab tab enter");
    cx.run_until_parked();
    assert_eq!(
        sent(),
        [format!(
            r#"{{"c":"menuaction","op":"openWith","id":{id},"application":"nvim.desktop","always":true}}"#
        )],
        "Enter on Open asks for the cursor application with the box ticked"
    );
    assert_eq!(
        card_title(&shell, cx).as_deref(),
        Some("Open mystery.xyz With"),
        "the card waits for the backend"
    );
    deliver(
        &shell,
        cx,
        &format!(r#"{{"t":"menuaction","id":{id},"op":"openWith","ok":true}}"#),
    );
    assert_eq!(
        card_title(&shell, cx),
        None,
        "the backend's yes closes the card"
    );
    quit(&shell, cx);
}

#[gpui::test]
fn a_path_no_longer_listed_says_so_and_opens_nothing(cx: &mut TestAppContext) {
    let (shell, cx) = shell(cx);
    // v0.3.12 Pane.qml showOpenWith: a path its listing no longer names is refused with this sentence.
    shell.update(cx, |shell, cx| {
        shell.open_open_with(0, "/files/gone.xyz".into(), cx)
    });
    cx.run_until_parked();
    assert_eq!(
        card_title(&shell, cx),
        None,
        "no card opens for a row that is gone"
    );
    assert_eq!(status(&shell, cx), "That file is no longer listed.");
    quit(&shell, cx);
}

#[gpui::test]
fn escape_on_the_open_with_card_closes_it_and_expires_its_snapshot(cx: &mut TestAppContext) {
    let (shell, cx, id) = opened(cx);
    sent();
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(card_title(&shell, cx), None, "Escape closes the card");
    assert_eq!(
        sent(),
        [format!(r#"{{"c":"menuaction","op":"close","id":{id}}}"#)],
        "Escape sends the close the backend reads"
    );
    quit(&shell, cx);
}

// A second ask while a card shows replaces it, and the replaced Open With card expires its snapshot first.
#[gpui::test]
fn a_second_ask_closes_the_card_that_shows(cx: &mut TestAppContext) {
    let (shell, cx, first) = opened(cx);
    sent();
    cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            shell.open_open_with(0, "/files/mystery.xyz".into(), cx)
        })
    });
    cx.run_until_parked();
    let lines = sent();
    assert_eq!(
        lines.first().cloned(),
        Some(format!(r#"{{"c":"menuaction","op":"close","id":{first}}}"#)),
        "the first card's snapshot expires before the second opens: {lines:?}"
    );
    let second = shell.read_with(cx, |shell, _| shell.open_with.as_ref().map(|open| open.id));
    assert!(
        second.is_some_and(|id| id != first),
        "the second ask holds its own id"
    );
    let ask = r#"{"ask":"run","kind":"script","size":21,"handlerId":"","handlerName":"","dev":"8","ino":"9","mtimeS":11,"mtimeNs":12,"target":"/files/mystery.xyz"}"#;
    cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            shell.open_run_card(0, "/files/mystery.xyz".into(), ask.into(), false, cx)
        })
    });
    cx.run_until_parked();
    assert_eq!(
        sent(),
        [format!(
            r#"{{"c":"menuaction","op":"close","id":{}}}"#,
            second.unwrap()
        )],
        "a run ask closes the Open With card"
    );
    assert_eq!(card_title(&shell, cx), None);
    assert!(
        shell.read_with(cx, |shell, _| shell.run_card.is_some()),
        "the run card opens in its place"
    );
    cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            shell.open_open_with(0, "/files/mystery.xyz".into(), cx)
        })
    });
    cx.run_until_parked();
    assert!(
        shell.read_with(cx, |shell, _| shell.run_card.is_none()),
        "an Open With ask closes the run card"
    );
    quit(&shell, cx);
}

// v0.3.12 DialogScrim: a press beside the card cancels it, as Escape does, and expires the snapshot.
#[gpui::test]
fn a_press_on_the_scrim_cancels_the_open_with_card(cx: &mut TestAppContext) {
    let (shell, cx, id) = opened(cx);
    assert!(cx.debug_bounds("op3-card").is_none());
    assert!(cx.debug_bounds("op3-openwith-scrim").is_none());
    answer(&shell, cx, id);
    sent();
    let scrim = cx
        .debug_bounds("op3-openwith-scrim")
        .expect("the scrim is drawn");
    let card = cx.debug_bounds("op3-card").expect("the card is drawn");
    let at = scrim.origin + gpui::point(gpui::px(4.), gpui::px(4.));
    assert!(!card.contains(&at), "the press lands beside the card");
    // A press on the card's own body, away from its buttons, keeps the card.
    let inside = card.origin + gpui::point(gpui::px(4.), gpui::px(4.));
    cx.simulate_mouse_move(inside, None, gpui::Modifiers::none());
    cx.simulate_mouse_down(inside, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(inside, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        card_title(&shell, cx).as_deref(),
        Some("Open mystery.xyz With"),
        "a press on the card is not a press on the scrim"
    );
    cx.simulate_mouse_move(at, None, gpui::Modifiers::none());
    cx.simulate_mouse_down(at, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(at, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(card_title(&shell, cx), None, "the scrim press cancels");
    assert_eq!(
        sent(),
        [format!(r#"{{"c":"menuaction","op":"close","id":{id}}}"#)]
    );
    quit(&shell, cx);
}

// A snapshot the backend never took opens no card, since its applications could never arrive.
#[gpui::test]
fn an_ask_on_a_stopped_backend_opens_no_card(cx: &mut TestAppContext) {
    let (shell, cx) = shell(cx);
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().backend = None
    });
    cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            shell.open_open_with(0, "/files/mystery.xyz".into(), cx)
        })
    });
    cx.run_until_parked();
    assert_eq!(
        card_title(&shell, cx),
        None,
        "no card waits on a backend that is gone"
    );
    quit(&shell, cx);
}

// The snapshot names listing rows, so a held window that starts at row 350 sends the asked row's own listing index.
#[gpui::test]
fn the_snapshot_names_the_listing_row_past_the_held_windows_start(cx: &mut TestAppContext) {
    let (shell, cx) = shell(cx);
    shell.update(cx, |shell, _| {
        let pane = &mut shell.panes[0].as_mut().unwrap().pane;
        let rows = pane.held.rows.clone();
        pane.held = std::rc::Rc::new(crate::list::pane::Held {
            listing: pane.held.listing,
            total: 400,
            start: 350,
            rows,
        });
    });
    sent();
    cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            shell.open_open_with(0, "/files/mystery.xyz".into(), cx)
        })
    });
    cx.run_until_parked();
    let snapshot = sent()
        .into_iter()
        .find(|line| line.contains(r#""op":"snapshot""#))
        .expect("the card snapshots its row");
    assert!(
        snapshot.contains(r#""rows":[351],"cursor":351"#),
        "the snapshot names listing row 351: {snapshot}"
    );
    quit(&shell, cx);
}

// EY-357: each application row of the card draws that application's own icon, as boards/openwith-list.png and v0.3.12 do.
#[gpui::test]
fn the_open_with_card_draws_each_applications_own_icon(cx: &mut TestAppContext) {
    let base =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/FIX-MENU-ROWS");
    std::fs::create_dir_all(&base).unwrap();
    let base = base.canonicalize().unwrap();
    let made = std::process::Command::new("mktemp")
        .arg("-d")
        .arg(base.join("openwith-icons.XXXXXXXX"))
        .output()
        .unwrap();
    assert!(made.status.success());
    let root = std::path::PathBuf::from(String::from_utf8(made.stdout).unwrap().trim());
    assert!(root.is_absolute() && root.starts_with(&base));
    std::fs::write(root.join(".flea-test-sandbox"), "fixture\n").unwrap();
    let icons: Vec<String> = [9, 18]
        .iter()
        .map(|width| {
            let path = root.join(format!("app-{width}.svg"));
            std::fs::write(&path, format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18"><rect width="{width}" height="18" fill="red"/></svg>"#)).unwrap();
            path.to_string_lossy().into_owned()
        })
        .collect();
    let (shell, cx, id) = opened(cx);
    deliver(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","id":{id},"op":"applications","ok":true,"path":"/files/mystery.xyz","kind":"Data","mime":"application/x-mystery","applications":[],"installed":[{{"id":"a.desktop","label":"Aether","icon":"{}","default":false}},{{"id":"b.desktop","label":"btop++","icon":"{}","default":false}}]}}"#,
            icons[0], icons[1]
        ),
    );
    let mut painted = Vec::new();
    for _ in 0..12 {
        cx.run_until_parked();
        crate::g2::context_menus::app_icons::PAINTED
            .with(|held| *held.borrow_mut() = Some(Vec::new()));
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        painted = crate::g2::context_menus::app_icons::PAINTED
            .with(|held| held.borrow_mut().take().unwrap_or_default());
        if icons.iter().all(|icon| painted.contains(icon)) {
            break;
        }
    }
    for icon in &icons {
        assert!(
            painted.contains(icon),
            "the card paints each application's icon, not an empty slot: {painted:?}"
        );
    }
    quit(&shell, cx);
}

#[path = "openwith_ime_tests.rs"]
mod ime;
