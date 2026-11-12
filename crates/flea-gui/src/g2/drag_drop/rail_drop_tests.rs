//! EY-221: a row dragged from the listing onto a rail place, through the real pointer path into the mounted rail.
use super::mounted_tests::{centre, drain, frame, open, Root};
use crate::{list::bench, state::Shell};
use flea::ui_logic_rail_places::{
    order::{Heading, RailSection},
    rail::{Group, Kind, RailEntry},
};
use gpui::{
    point, px, Modifiers, MouseButton, Pixels, Point, TestAppContext, VisualTestContext,
    WindowHandle,
};

fn place(label: &str, path: &str, group: Group, kind: Kind) -> RailEntry {
    RailEntry {
        group,
        kind,
        label: label.into(),
        path: path.into(),
        glyph: "house",
        ..RailEntry::default()
    }
}

// The window's own rail, with a Home place in the test root and the Trash row.
pub(super) fn rail(cx: &mut TestAppContext, handle: WindowHandle<Shell>, home: &str) {
    handle
        .update(cx, |shell, _, cx| shell.mount_rail(cx))
        .unwrap();
    frame(cx, handle);
    cx.run_until_parked();
    let home = home.to_owned();
    handle
        .update(cx, |shell, _, cx| {
            let places = shell.rail.as_ref().unwrap().places.clone();
            places.update(cx, |producer, cx| {
                producer.replace_sections(
                    vec![RailSection {
                        heading: Heading::Key("places.heading.places"),
                        rows: vec![
                            place("Home", &home, Group::Home, Kind::Home),
                            place("Trash", "", Group::Trash, Kind::Trash),
                        ],
                    }],
                    cx,
                )
            });
        })
        .unwrap();
    frame(cx, handle);
}

// The centre of a rail row as the rows last painted it, in window coordinates.
pub(super) fn rail_centre(
    cx: &mut TestAppContext,
    handle: WindowHandle<Shell>,
    index: usize,
) -> Point<Pixels> {
    handle
        .update(cx, |shell, _, cx| {
            let producer = shell.rail.as_ref().unwrap().places.read(cx);
            let laid = producer.laid.borrow();
            let row = laid.row(index).expect("the rail painted the row");
            point(
                px(row.rect.x + row.rect.width / 2.),
                px(row.rect.y + row.rect.height / 2.),
            )
        })
        .unwrap()
}

// Press on a listing row and carry it over a rail row, still held.
pub(super) fn hold(
    vcx: &mut VisualTestContext,
    from: Point<Pixels>,
    to: Point<Pixels>,
    keys: Modifiers,
) {
    vcx.simulate_mouse_move(from, None, Modifiers::default());
    vcx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_move(
        point(from.x + px(3.), from.y - px(4.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    vcx.simulate_mouse_move(point(to.x + px(1.), to.y), Some(MouseButton::Left), keys);
    vcx.simulate_mouse_move(to, Some(MouseButton::Left), keys);
    vcx.run_until_parked();
    vcx.simulate_mouse_move(to, Some(MouseButton::Left), keys);
    vcx.run_until_parked();
}

// The held pointer moves in place with new keys, as a key pressed mid-drag reaches the next drag move.
pub(super) fn hold_keys(vcx: &mut VisualTestContext, at: Point<Pixels>, keys: Modifiers) {
    vcx.simulate_mouse_move(point(at.x + px(1.), at.y), Some(MouseButton::Left), keys);
    vcx.simulate_mouse_move(at, Some(MouseButton::Left), keys);
    vcx.run_until_parked();
}

fn cue(
    vcx: &mut VisualTestContext,
    handle: WindowHandle<Shell>,
) -> (Option<(usize, String)>, Vec<String>) {
    handle
        .update(vcx, |shell, _, cx| {
            let producer = shell.rail.as_ref().unwrap().places.read(cx);
            let target = producer
                .model
                .borrow()
                .drop_target
                .clone()
                .map(|(row, label)| (row, label.to_string()));
            let lines = shell.status_messages(cx, |messages, _| {
                messages
                    .activities()
                    .iter()
                    .map(|activity| activity.text.clone())
                    .collect()
            });
            (target, lines)
        })
        .unwrap()
}

// op1 asks the backend for collisions first; an empty answer releases the transfer line itself.
fn transfer(
    vcx: &mut VisualTestContext,
    handle: WindowHandle<Shell>,
    lines: &mut impl FnMut() -> Option<String>,
) -> String {
    let sent = drain(lines);
    assert!(
        sent.iter().any(|line| line.contains(r#""c":"collisions""#)),
        "no collisions probe among {sent:?}"
    );
    handle
        .update(vcx, |shell, window, cx| {
            let line = r#"{"t":"collisions","id":1,"total":0,"names":[],"mergeable":false}"#;
            shell.receive(
                0,
                crate::backend::Event::Operations(line.into()),
                window,
                cx,
            );
        })
        .unwrap();
    vcx.run_until_parked();
    drain(lines)
        .into_iter()
        .chain(sent)
        .find(|line| line.contains(r#""c":"transfer""#))
        .expect("a transfer after the empty collisions answer")
}

pub(super) fn home(root: &Root) -> String {
    let home = format!("{}/home", root.src().trim_end_matches("/src"));
    std::fs::create_dir_all(&home).unwrap();
    home
}

#[test]
fn ey221_raildrop_held_over_home_lights_move_here_and_says_the_line() {
    let root = Root::new("railheld");
    let home = home(&root);
    let mut cx = bench::real_cx();
    let (handle, _lines) = open(&mut cx, root.src());
    frame(&mut cx, handle);
    rail(&mut cx, handle, &home);
    let file = centre(&mut cx, handle, 1);
    let to = rail_centre(&mut cx, handle, 0);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    hold(&mut vcx, file, to, Modifiers::default());
    let (target, lines) = cue(&mut vcx, handle);
    assert_eq!(target, Some((0, "move here".to_owned())));
    assert!(
        lines.contains(&"Move 1 item to Home · ctrl copies, shift moves".to_owned()),
        "{lines:?}"
    );
    vcx.simulate_mouse_move(file, Some(MouseButton::Left), Modifiers::default());
    vcx.run_until_parked();
    let (target, lines) = cue(&mut vcx, handle);
    assert_eq!(target, None, "leaving the rail drops the cue");
    // FileDrag.leaveTarget: an own drag off every target keeps its line with no folder name.
    assert_eq!(
        lines,
        vec!["Move 1 item to a folder · ctrl copies, shift moves".to_owned()]
    );
}

#[test]
fn ey221_raildrop_release_on_home_moves_into_it() {
    let root = Root::new("railmove");
    let home = home(&root);
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open(&mut cx, root.src());
    frame(&mut cx, handle);
    rail(&mut cx, handle, &home);
    let file = centre(&mut cx, handle, 1);
    let to = rail_centre(&mut cx, handle, 0);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    hold(&mut vcx, file, to, Modifiers::default());
    vcx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    // The move rides op1's transfer, the request the backend journals for z.
    let transfer = transfer(&mut vcx, handle, &mut lines);
    assert!(transfer.contains(r#""op":"move""#), "{transfer}");
    assert!(
        transfer.contains(&format!("{}/b.txt", root.src())),
        "{transfer}"
    );
    assert!(
        transfer.contains(&format!(r#""dest":"{home}""#)),
        "{transfer}"
    );
    let (target, activity) = cue(&mut vcx, handle);
    assert_eq!(target, None, "the release clears the cue");
    assert!(activity.is_empty(), "{activity:?}");
}

#[test]
fn fx462_a_cached_unavailable_place_accepts_a_drop_after_its_target_returns() {
    let root = Root::new("returned-rail");
    let home = home(&root);
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open(&mut cx, root.src());
    frame(&mut cx, handle);
    rail(&mut cx, handle, &home);
    handle
        .update(&mut cx, |shell, _, cx| {
            shell
                .rail
                .as_ref()
                .unwrap()
                .places
                .update(cx, |producer, cx| {
                    let mut sections = producer.sections.clone();
                    sections[0].rows[0].error =
                        Some(flea::ui_logic_rail_places::rail::FavouriteError::Status(
                            "unavailable (NotFound)".into(),
                        ));
                    producer.replace_sections(sections, cx);
                });
        })
        .unwrap();
    frame(&mut cx, handle);
    let file = centre(&mut cx, handle, 1);
    let to = rail_centre(&mut cx, handle, 0);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    hold(&mut vcx, file, to, Modifiers::default());
    vcx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    let transfer = transfer(&mut vcx, handle, &mut lines);
    assert!(
        transfer.contains(&format!(r#""dest":"{home}""#)),
        "{transfer}"
    );
}

#[test]
fn ey221_raildrop_ctrl_at_the_release_copies() {
    let root = Root::new("railcopy");
    let home = home(&root);
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open(&mut cx, root.src());
    frame(&mut cx, handle);
    rail(&mut cx, handle, &home);
    let file = centre(&mut cx, handle, 1);
    let to = rail_centre(&mut cx, handle, 0);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    let ctrl = Modifiers {
        control: true,
        ..Modifiers::default()
    };
    hold(&mut vcx, file, to, ctrl);
    let (target, lines_held) = cue(&mut vcx, handle);
    assert_eq!(target, Some((0, "copy here".to_owned())));
    assert!(
        lines_held.contains(&"Copy 1 item to Home".to_owned()),
        "{lines_held:?}"
    );
    vcx.simulate_mouse_up(to, MouseButton::Left, ctrl);
    vcx.run_until_parked();
    let transfer = transfer(&mut vcx, handle, &mut lines);
    assert!(transfer.contains(r#""op":"copy""#), "{transfer}");
    assert!(
        transfer.contains(&format!(r#""dest":"{home}""#)),
        "{transfer}"
    );
}

#[test]
fn ey221_raildrop_release_on_trash_sends_the_trash_request() {
    let root = Root::new("railtrash");
    let home = home(&root);
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open(&mut cx, root.src());
    frame(&mut cx, handle);
    rail(&mut cx, handle, &home);
    let file = centre(&mut cx, handle, 1);
    let to = rail_centre(&mut cx, handle, 1);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    hold(&mut vcx, file, to, Modifiers::default());
    let (target, held) = cue(&mut vcx, handle);
    assert_eq!(target, Some((1, "move here".to_owned())));
    assert!(
        held.contains(&"Move 1 item to Trash".to_owned()),
        "{held:?}"
    );
    drain(&mut lines);
    vcx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    let sent = drain(&mut lines);
    // dd's own request by path, which the backend journals so z restores it.
    let expected = format!(r#"{{"c":"trash","paths":["{}/b.txt"]}}"#, root.src());
    assert!(sent.contains(&expected), "{sent:?}");
    assert!(
        sent.iter().all(|line| !line.contains("transfer")),
        "{sent:?}"
    );
}

#[test]
fn pb_drag_3_raildrop_entries_changing_under_a_held_drag_re_aim_it() {
    let root = Root::new("railreaim");
    let home = home(&root);
    let docs = format!("{}/docs", root.src().trim_end_matches("/src"));
    std::fs::create_dir_all(&docs).unwrap();
    let mut cx = bench::real_cx();
    let (handle, _lines) = open(&mut cx, root.src());
    frame(&mut cx, handle);
    rail(&mut cx, handle, &home);
    let file = centre(&mut cx, handle, 1);
    let to = rail_centre(&mut cx, handle, 0);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    hold(&mut vcx, file, to, Modifiers::default());
    assert_eq!(cue(&mut vcx, handle).0, Some((0, "move here".to_owned())));
    // A place lands above Home while the drag rests: the row under the pointer is now Docs.
    handle
        .update(&mut vcx, |shell, _, cx| {
            let places = shell.rail.as_ref().unwrap().places.clone();
            places.update(cx, |producer, cx| {
                producer.replace_sections(
                    vec![RailSection {
                        heading: Heading::Key("places.heading.places"),
                        rows: vec![
                            place("Docs", &docs, Group::Favourite, Kind::Favourite),
                            place("Home", &home, Group::Home, Kind::Home),
                            place("Trash", "", Group::Trash, Kind::Trash),
                        ],
                    }],
                    cx,
                )
            });
        })
        .unwrap();
    frame(&mut vcx, handle);
    vcx.run_until_parked();
    frame(&mut vcx, handle);
    let (_, lines) = cue(&mut vcx, handle);
    assert!(
        lines.contains(&"Move 1 item to Docs · ctrl copies, shift moves".to_owned()),
        "{lines:?}"
    );
}
