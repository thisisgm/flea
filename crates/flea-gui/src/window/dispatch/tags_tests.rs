//! Tag keys run through the mounted root, menu validation and the backend's ordered pipe.
use super::menus_tests::{answer, home, reply, settle, validated};
use super::test_shell::{land, land_on, open_root, open_root_with, sent, set_cursor};
use crate::window::u07_tests::read;
use flea::jsondoc::{self, Json};
use gpui::{Entity, TestAppContext, VisualTestContext};

const FILE: u32 = 0o100644;

pub(super) fn request(command: &str) -> Json {
    sent()
        .into_iter()
        .filter_map(|line| jsondoc::parse(&line).ok())
        .find(|line| line.get("c").and_then(Json::as_str) == Some(command))
        .unwrap_or_else(|| panic!("the mounted window sent {command}"))
}

fn toggle_digit(cx: &mut TestAppContext, digit: usize) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    set_cursor(&shell, cx, 2);
    cx.simulate_keystrokes("m");
    answer(&shell, cx, &[FILE]);
    sent();
    cx.simulate_keystrokes(&digit.to_string());
    validated(
        &shell,
        cx,
        &format!("tags:tag{digit}"),
        &["/files/notes.txt"],
    );
    let change = request("settags");
    let name = flea::ui_logic_pl4::tags::NAMES[digit - 1];
    assert_eq!(change.get("tag").and_then(Json::as_str), Some(name));
    assert_eq!(change.get("op").and_then(Json::as_str), Some("toggle"));
    assert_eq!(change.get("listing"), Some(&Json::Num("1".into())));
    assert_eq!(
        change.get("rows").and_then(Json::as_array),
        Some(&[Json::Num("2".into())][..])
    );
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "true");
    cx.simulate_keystrokes("escape");
    cx.simulate_keystrokes("escape");
    complete(&shell, cx, r#"{"t":"settagged","ok":1,"op":"add"}"#);
    assert_eq!(
        status(&shell, cx),
        format!("Tagged 1 item {name} \u{b7} z undoes")
    );
    cx.simulate_keystrokes("z");
    settle(cx);
    request("undo");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_tag_digit_1(cx: &mut TestAppContext) {
    toggle_digit(cx, 1);
}
#[gpui::test]
fn pb_tag_digit_2(cx: &mut TestAppContext) {
    toggle_digit(cx, 2);
}
#[gpui::test]
fn pb_tag_digit_3(cx: &mut TestAppContext) {
    toggle_digit(cx, 3);
}
#[gpui::test]
fn pb_tag_digit_4(cx: &mut TestAppContext) {
    toggle_digit(cx, 4);
}
#[gpui::test]
fn pb_tag_digit_5(cx: &mut TestAppContext) {
    toggle_digit(cx, 5);
}
#[gpui::test]
fn pb_tag_digit_6(cx: &mut TestAppContext) {
    toggle_digit(cx, 6);
}
#[gpui::test]
fn pb_tag_digit_7(cx: &mut TestAppContext) {
    toggle_digit(cx, 7);
}

#[gpui::test]
fn pb_tag_t_opens_the_selection_flyout(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    set_cursor(&shell, cx, 2);
    cx.simulate_keystrokes("T");
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.menu.flyout),
        Some("tags")
    );
    super::menus_tests::tags_answer(&shell, cx, &[FILE]);
    assert_eq!(
        read(&shell, cx, "contextMenuSubmenuEntries", &[]),
        "Red|Orange|Yellow|Green|Blue|Purple|Gray"
    );
    cx.simulate_keystrokes("1");
    validated(&shell, cx, "tags:tag1", &["/files/notes.txt"]);
    assert_eq!(
        request("settags").get("tag").and_then(Json::as_str),
        Some("Red")
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

pub(super) fn states(
    shell: &Entity<crate::state::Shell>,
    cx: &mut VisualTestContext,
    applied: &str,
    mixed: &str,
) {
    let ask = request("tagstates");
    let id = ask.get("id").unwrap();
    let Json::Num(id) = id else {
        panic!("numeric tag read id")
    };
    reply(
        shell,
        cx,
        &format!(
            r#"{{"t":"tagstates","id":{id},"listing":1,"applied":[{applied}],"mixed":[{mixed}]}}"#
        ),
    );
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        window.simulate_next_frame(cx);
    });
    settle(cx);
}

pub(super) fn tagged_rows() -> Vec<flea::protocol::RowFacts> {
    let flea::protocol::Reply::Rows { rows, .. } = flea::protocol::parse_reply(r#"{"t":"rows","listing":1,"start":0,"rows":[{"n":"notes.txt","d":false,"s":99,"m":0,"p":33188,"i":"text-plain","tg":["Red","Green","Blue"]}]}"#).unwrap() else { panic!("rows") };
    rows
}

#[gpui::test]
fn pb_tag_selection_reads_offscreen_rows_and_clear_all(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    let count = shell.read_with(cx, |shell, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .pane
            .swap
            .pending
            .as_ref()
            .unwrap()
            .first
    });
    let mut rows = home();
    rows.extend(
        (rows.len()..count).map(|at| super::menus_tests::entry(&format!("file-{at}.txt"), FILE)),
    );
    land_on(&shell, cx, 0, "/files", rows, 500, 1);
    cx.update(|_, cx| {
        shell.update(cx, |shell, _| {
            let host = shell.panes[0].as_mut().unwrap();
            host.pane.cursor = 2;
            host.selected = vec![2, 400];
        })
    });
    cx.simulate_keystrokes("T");
    answer(&shell, cx, &[FILE, FILE]);
    states(&shell, cx, "\"Red\"", "\"Blue\"");
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "true");
    let model = jsondoc::parse(&read(&shell, cx, "contextMenuModel", &[])).unwrap();
    let tags = model
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row.get("action").and_then(Json::as_str) == Some("tags"))
        .unwrap();
    let rows = tags.get("submenu").and_then(Json::as_array).unwrap();
    assert_eq!(
        rows[0]
            .get("tagRow")
            .and_then(|r| r.get("state"))
            .and_then(Json::as_str),
        Some("applied")
    );
    assert_eq!(
        rows[4]
            .get("tagRow")
            .and_then(|r| r.get("state"))
            .and_then(Json::as_str),
        Some("mixed")
    );
    assert!(read(&shell, cx, "contextMenuSubmenuEntries", &[]).ends_with("|-|Remove All Tags"));
    for _ in 0..7 {
        cx.simulate_keystrokes("down");
    }
    cx.simulate_keystrokes("enter");
    validated(
        &shell,
        cx,
        "tags:__clear__",
        &["/files/notes.txt", "/files/offscreen.txt"],
    );
    let change = request("settags");
    assert_eq!(change.get("op").and_then(Json::as_str), Some("clear"));
    assert_eq!(change.get("tag").and_then(Json::as_str), Some(""));
    assert_eq!(
        change.get("rows").and_then(Json::as_array).unwrap().len(),
        2
    );
    complete(&shell, cx, r#"{"t":"settagged","ok":2,"op":"clear"}"#);
    assert_eq!(
        status(&shell, cx),
        "Removed all tags from 2 items \u{b7} z undoes"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_tag_rows_feed_the_live_rail(cx: &mut TestAppContext) {
    let (shell, cx) = open_root_with(
        cx,
        r#""keys":"default","view":"list","places":{"rail":"shown","showTags":true}"#,
        None,
    );
    shell.update(cx, |shell, cx| {
        shell.launch.settings.data = jsondoc::parse(
            r#"{"keys":"default","view":"list","places":{"rail":"shown","showTags":true}}"#,
        )
        .unwrap();
        let rail = shell.rail.as_ref().unwrap();
        rail.places.update(cx, |producer, cx| {
            producer.adopt_state(&shell.launch.settings.data, cx);
            producer.fixture_home("/files", "", cx);
        });
    });
    land(&shell, cx, "/files", tagged_rows(), 1);
    let labels = read(&shell, cx, "railLabels", &[]);
    for name in ["Red", "Green", "Blue"] {
        assert!(
            labels.split('|').any(|label| label == name),
            "{name} absent from {labels}"
        );
    }
    cx.simulate_keystrokes("backspace");
    land(&shell, cx, "/", home(), 2);
    assert!(
        read(&shell, cx, "railLabels", &[])
            .split('|')
            .any(|label| label == "Red"),
        "seen tags survive leaving their folder"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_tag_columns_paint_solid_swatches_after_the_name(cx: &mut TestAppContext) {
    let (shell, cx) = open_root_with(cx, r#""keys":"default","view":"columns""#, None);
    land(&shell, cx, "/files", tagged_rows(), 1);
    for text in [
        "background='#101315'\nforeground='#c7b59a'\nred='#d65a4f'\ngreen='#79ab55'\nblue='#509467'",
        "background='#eff1f5'\nforeground='#4c4f69'\nred='#d20f39'\ngreen='#40a02b'\nblue='#1e66f5'",
        "background='#181818'\nforeground='#d8d8d8'\nred='#999999'\ngreen='#999999'\nblue='#999999'",
        "background='#181818'\nforeground='#d8d8d8'\nred='invalid'",
    ] {
        cx.update(|_, cx| cx.set_global(crate::palette::Theme::from_text(text)));
        let (mut squares, scale) = cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
            (window.painted_quads(), window.scale_factor())
        });
        squares.retain(|quad| {
            (quad.bounds.size.width.0 / scale - 14.).abs() < 0.01 &&
            (quad.bounds.size.height.0 / scale - 14.).abs() < 0.01 &&
            quad.background.as_solid().is_some_and(|ink| ink.a == 1.)
        });
        squares.sort_by(|a, b| a.bounds.origin.x.0.total_cmp(&b.bounds.origin.x.0));
        assert_eq!(squares.len(), 3, "each tag paints a solid square on {text}");
        let (expected_x, expected_y, right) = shell.read_with(cx, |shell, _| {
            let painter = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap().painters.get(crate::columns::geometry::Slot::Active).unwrap().borrow();
            let frame = painter.drawn.as_ref().unwrap();
            let name = painter.prepared(0).unwrap().name(false).line().width;
            (f32::from(frame.bounds.left()) + 14. + 23. + 9. + f32::from(name) + 9.,
             f32::from(frame.bounds.top()) + (crate::list::pane::ROW_HEIGHT - 14.) / 2.,
             f32::from(frame.bounds.right()))
        });
        for (at, square) in squares.iter().enumerate() {
            let snapped_x = ((expected_x + at as f32 * 17.) * scale).round() / scale;
            let snapped_y = (expected_y * scale).round() / scale;
            assert!((square.bounds.origin.x.0 / scale - snapped_x).abs() < 0.01);
            assert!((square.bounds.origin.y.0 / scale - snapped_y).abs() < 0.01);
            assert!(square.bounds.origin.x.0 / scale + 14. < right);
        }
    }
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

/// Completes a metadata write and answers the held-window reread on the real acceptance path.
pub(super) fn complete(
    shell: &Entity<crate::state::Shell>,
    cx: &mut VisualTestContext,
    line: &str,
) {
    let (backend, mut windows) = crate::backend::Backend::test_channel();
    let (original, held) = shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        (
            host.backend.replace(backend),
            std::rc::Rc::clone(&host.pane.held),
        )
    });
    reply(shell, cx, line);
    let (start, count) = windows().expect("completion asks for the held window");
    assert_eq!(start, held.start);
    assert!(count > 0);
    let rows = held.rows.iter().take(count).cloned().collect();
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().backend = original
    });
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(
                0,
                crate::backend::Event::Reply(flea::protocol::Reply::Rows {
                    start: held.start,
                    listing: held.listing,
                    rows,
                }),
                window,
                cx,
            );
        })
    });
    settle(cx);
    assert!(shell.read_with(cx, |shell, _| shell.panes[0]
        .as_ref()
        .unwrap()
        .pane
        .requests
        .outstanding
        .is_none()));
}

/// Reads the same completion notice the mounted footer draws after a held-row reply.
pub(super) fn status(shell: &Entity<crate::state::Shell>, cx: &mut VisualTestContext) -> String {
    format!(
        "{}{}",
        read(shell, cx, "statusPrimary", &[]),
        read(shell, cx, "statusSecondary", &[])
    )
}
