//! U08 regression proof through the composed window, native input and IPC dispatch.
use super::*;
use crate::{backend::Backend, ipc::server, state::tests::shell_with};
use flea::{
    ipc::wire,
    jsondoc,
    ui_logic_pl4::{
        order::{Heading, RailSection},
        rail::{Group, Kind, RailEntry},
    },
};
use gpui::{point, px, Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext};
use std::{cell::RefCell, rc::Rc};

fn open(hidden: bool, cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext) {
    let (backend, receiver) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let mut shell = shell_with(backend, cx);
        window.focus(&shell.focus, cx);
        cx.on_release(move |_, _| drop(receiver)).detach();
        if hidden {
            shell.launch.settings.data = jsondoc::parse(r#"{"places":{"rail":"hidden"}}"#).unwrap();
        }
        let join = mount(
            shell.locale.catalog.clone(),
            &shell.launch.settings.data,
            shell.focus_side,
            cx,
        );
        cx.subscribe(&join.host, |shell, _, event, cx| {
            shell.rail_host(*event, cx)
        })
        .detach();
        shell.slots.rail = Some(AnyView::from(join.host.clone()));
        shell.rail = Some(join);
        shell
    });
    cx.run_until_parked();
    // Row fixtures start empty with the initial cursor, independent of the host's HOME entries.
    producer(&shell, cx).update(cx, |producer, cx| {
        producer.replace_sections(Vec::new(), cx);
        producer.key_action("cursorFirst", cx);
    });
    (shell, cx)
}

fn frame(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

fn producer(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Entity<Producer> {
    shell.read_with(cx, |shell, _| shell.rail.as_ref().unwrap().places.clone())
}

fn rows(shell: &Entity<Shell>, entries: Vec<RailEntry>, cx: &mut VisualTestContext) {
    producer(shell, cx).update(cx, |producer, cx| {
        producer.replace_sections(
            vec![RailSection {
                heading: Heading::Key("places.heading.places"),
                rows: entries,
            }],
            cx,
        )
    });
    frame(cx);
}

fn row(label: &str, path: &str) -> RailEntry {
    RailEntry {
        label: label.into(),
        path: path.into(),
        glyph: "house",
        ..RailEntry::default()
    }
}

fn ipc(
    shell: &Entity<Shell>,
    name: &str,
    args: &[&str],
    cx: &mut VisualTestContext,
) -> wire::Reply {
    let (answer, receiver) = std::sync::mpsc::sync_channel(1);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.ipc(
                server::Event::Call {
                    request: wire::Request::Call {
                        target: "flea".into(),
                        function: name.into(),
                        arguments: args.iter().map(|v| (*v).into()).collect(),
                    },
                    answer,
                },
                window,
                cx,
            )
        })
    });
    receiver.try_recv().expect("a synchronous IPC answer").reply
}

#[gpui::test]
fn u08_composition_paints_rows_and_readers_report_the_drawn_centres(cx: &mut TestAppContext) {
    let (shell, cx) = open(false, cx);
    rows(&shell, vec![row("A", "/a"), row("B", "/b")], cx);
    assert_eq!(ipc(&shell, "railCount", &[], cx).stdout.trim(), "2");
    assert_eq!(ipc(&shell, "railLabels", &[], cx).stdout.trim(), "A|B");
    let producer = producer(&shell, cx);
    let centre = producer.read_with(cx, |producer, _| {
        let laid = producer.laid.borrow();
        let row = laid
            .row(1)
            .expect("composition painted the second rail row");
        assert_eq!(row.rect.height, 29.);
        assert_eq!(row.label_rect.x, 38.);
        point(
            px(row.rect.x + row.rect.width / 2.),
            px(row.rect.y + row.rect.height / 2.),
        )
    });
    let reply = ipc(&shell, "railRowCentre", &["1"], cx);
    let values: Vec<f32> = reply
        .stdout
        .split_whitespace()
        .map(|v| v.parse().unwrap())
        .collect();
    assert_eq!(values.len(), 2);
    assert!((values[0] - f32::from(centre.x)).abs() <= 1.);
    assert!((values[1] - f32::from(centre.y)).abs() <= 1.);
    cx.simulate_mouse_move(centre, None, Modifiers::none());
    cx.simulate_mouse_down(centre, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(ipc(&shell, "railCursor", &[], cx).stdout.trim(), "1");
    assert_eq!(ipc(&shell, "focusView", &[], cx).stdout.trim(), "list");
    assert_eq!(ipc(&shell, "railRowCentre", &["99"], cx).stdout.trim(), "");
    let idle = ipc(&shell, "recentMode", &[], cx);
    assert_eq!(
        idle.code, 0,
        "the Recent reader answers on the running window"
    );
    assert_eq!(
        idle.stdout.trim(),
        "",
        "a pane outside Recent reads an empty mode"
    );
}

#[gpui::test]
fn u08_hidden_rail_has_no_directory_reads_focus_or_stale_geometry(cx: &mut TestAppContext) {
    let (shell, cx) = open(true, cx);
    frame(cx);
    shell.update(cx, |shell, cx| shell.rail_focus(true, cx));
    shell.read_with(cx, |shell, cx| {
        let join = shell.rail.as_ref().unwrap();
        assert_eq!(join.host.read(cx).constructed, 0);
        assert_eq!(join.places.read(cx).directory_reads, 0);
        assert!(!join.places.read(cx).model.borrow().focused);
        assert!(!join.places.read(cx).polling);
    });
    assert_eq!(ipc(&shell, "railCount", &[], cx).stdout.trim(), "0");
    assert_eq!(ipc(&shell, "railEntries", &[], cx).stdout.trim(), "[]");
    assert_eq!(ipc(&shell, "railRowCentre", &["0"], cx).stdout.trim(), "");
    let state = jsondoc::parse(&ipc(&shell, "railState", &[], cx).stdout).unwrap();
    assert_eq!(state.get("inset").and_then(jsondoc::Json::as_f64), Some(0.));
}

#[gpui::test]
fn u08_keyboard_moves_the_rail_cursor_and_hiding_returns_focus(cx: &mut TestAppContext) {
    let (shell, cx) = open(false, cx);
    rows(&shell, vec![row("A", "/a"), row("B", "/b")], cx);
    shell.update(cx, |shell, cx| shell.rail_focus(true, cx));
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    assert_eq!(ipc(&shell, "railCursor", &[], cx).stdout.trim(), "1");
    cx.simulate_keystrokes("up");
    cx.run_until_parked();
    assert_eq!(ipc(&shell, "railCursor", &[], cx).stdout.trim(), "0");
    shell.update(cx, |shell, cx| {
        shell.rail_state(
            &jsondoc::parse(r#"{"places":{"rail":"hidden"}}"#).unwrap(),
            cx,
        )
    });
    frame(cx);
    assert_eq!(ipc(&shell, "railCount", &[], cx).stdout.trim(), "0");
    assert_eq!(
        ipc(&shell, "railRenameFieldShown", &[], cx).stdout.trim(),
        "false"
    );
    assert!(!producer(&shell, cx).read_with(cx, |producer, _| producer.model.borrow().focused));
}

// EY-347: PaneRail.qml focusPress leaves the keyboard on the list after a docked rail press, so Ctrl+2 still switches the view.
#[gpui::test]
fn ey347_docked_rail_press_keeps_the_keyboard_on_the_list(cx: &mut TestAppContext) {
    let (shell, cx) = open(false, cx);
    rows(&shell, vec![row("A", "/a"), row("B", "/b")], cx);
    let centre = producer(&shell, cx).read_with(cx, |producer, _| {
        let laid = producer.laid.borrow();
        let row = laid
            .row(1)
            .expect("composition painted the second rail row");
        point(
            px(row.rect.x + row.rect.width / 2.),
            px(row.rect.y + row.rect.height / 2.),
        )
    });
    cx.simulate_mouse_move(centre, None, Modifiers::none());
    cx.simulate_mouse_down(centre, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(centre, MouseButton::Left, Modifiers::none());
    frame(cx);
    assert_eq!(ipc(&shell, "railCursor", &[], cx).stdout.trim(), "1");
    assert_eq!(ipc(&shell, "focusView", &[], cx).stdout.trim(), "list");
    cx.update(|window, cx| {
        let shell = shell.read(cx);
        assert!(shell.focus.is_focused(window), "the list kept the keyboard");
        assert!(!shell.dual_focus.rail.contains_focused(window, cx));
        assert_eq!(shell.keyboard.context, flea::keys::Context::Listing);
    });
}

// EY-205: PaneRail.qml revealed is the pointer or keyboard reveal in every mode, so a docked rail the keyboard holds reads true.
#[gpui::test]
fn ey205_docked_rail_reports_revealed_while_the_keyboard_holds_it(cx: &mut TestAppContext) {
    let (shell, cx) = open(false, cx);
    rows(&shell, vec![row("A", "/a"), row("B", "/b")], cx);
    let revealed = |shell: &Entity<Shell>, cx: &mut VisualTestContext| {
        let state = jsondoc::parse(&ipc(shell, "railState", &[], cx).stdout).unwrap();
        matches!(state.get("revealed"), Some(jsondoc::Json::Bool(true)))
    };
    assert!(!revealed(&shell, cx), "nothing holds the docked rail yet");
    shell.update(cx, |shell, cx| shell.rail_focus(true, cx));
    frame(cx);
    assert!(
        revealed(&shell, cx),
        "the keyboard on a docked rail reveals it, as Ipc.qml's railRevealed does"
    );
    // Tab out hands the window back to the listing, as rail.rs does, or the window keeps the rail held.
    shell.update_in(cx, |shell, window, cx| {
        shell.rail_focus(false, cx);
        window.focus(&shell.focus, cx);
    });
    frame(cx);
    assert!(
        !revealed(&shell, cx),
        "Tab out withdraws the reveal at once"
    );
}

#[gpui::test]
fn u08_trash_count_handoff_changes_the_live_draw_and_respects_the_setting(cx: &mut TestAppContext) {
    let (shell, cx) = open(false, cx);
    shell.update(cx, |shell, cx| {
        shell.rail_state(
            &jsondoc::parse(r#"{"places":{"trashCount":true}}"#).unwrap(),
            cx,
        )
    });
    rows(&shell, vec![flea::ui_logic_pl4::order::trash_entry()], cx);
    shell.update(cx, |shell, cx| shell.rail_trash_count(12, cx));
    frame(cx);
    let detail = jsondoc::parse(&ipc(&shell, "railDetails", &[], cx).stdout).unwrap();
    let rows = detail
        .get("rows")
        .and_then(jsondoc::Json::as_array)
        .unwrap();
    assert_eq!(
        rows[0].get("detail").and_then(jsondoc::Json::as_str),
        Some("12")
    );
    producer(&shell, cx).read_with(cx, |producer, _| {
        let laid = producer.laid.borrow();
        let row = laid.row(0).unwrap();
        assert_eq!(row.indicator_rect.width, 0.);
        assert!(row.detail_rect.width > 0.);
        assert_eq!(row.detail_rect.x + row.detail_rect.width, laid.width - 14.);
    });
    shell.update(cx, |shell, cx| shell.rail_trash_count(0, cx));
    frame(cx);
    assert_eq!(
        producer(&shell, cx).read_with(cx, |producer, _| producer
            .laid
            .borrow()
            .row(0)
            .unwrap()
            .detail
            .clone()),
        ""
    );
}

#[gpui::test]
fn u08_fix2_pending_keyboard_reveal_survives_content_refresh(cx: &mut TestAppContext) {
    let (shell, cx) = open(false, cx);
    rows(
        &shell,
        (0..100)
            .map(|i| row(&format!("Place {i}"), &format!("/place/{i}")))
            .collect(),
        cx,
    );
    let producer = producer(&shell, cx);
    producer.update(cx, |p, cx| {
        p.key_action("cursorLast", cx);
        assert!(p.model.borrow().reveal_cursor);
        let mut sections = p.sections.clone();
        sections[0].rows[0].label = "Updated place".into();
        p.replace_sections(sections, cx);
        assert_eq!(p.cursor(), Some(99));
        assert!(
            p.model.borrow().reveal_cursor,
            "a refresh must keep an already pending keyboard reveal"
        );
    });
    frame(cx);
    assert!(!ipc(&shell, "railRowCentre", &["99"], cx)
        .stdout
        .trim()
        .is_empty());
}

#[path = "rail_service_tests.rs"]
mod service;

#[path = "rail_viewport_tests.rs"]
mod viewport;

#[path = "rail_reorder_tests.rs"]
mod reorder;

#[path = "rail_overflow_tests.rs"]
mod overflow;

#[rustfmt::skip]
mod r1_probes {
use super::*;

#[gpui::test]
fn review_u08_r1_cursor_keeps_place_identity_after_source_replacement(cx: &mut TestAppContext) {
    let (shell, cx) = open(false, cx);
    rows(&shell, vec![row("A", "/a"), row("B", "/b")], cx);
    shell.update(cx, |shell, cx| shell.rail_focus(true, cx));
    cx.simulate_keystrokes("down");
    frame(cx);
    assert_eq!(ipc(&shell, "railLabel", &["1"], cx).stdout.trim(), "B");
    rows(
        &shell,
        vec![row("X", "/x"), row("A", "/a"), row("B", "/b")],
        cx,
    );
    let cursor = ipc(&shell, "railCursor", &[], cx).stdout.trim().to_owned();
    assert_eq!(
        ipc(&shell, "railLabel", &[&cursor], cx).stdout.trim(),
        "B",
        "an insertion above B must not retarget the keyboard to A"
    );
}

#[gpui::test]
fn review_u08_r1_editor_click_never_activates_its_share(cx: &mut TestAppContext) {
    let (shell, cx) = open(false, cx);
    let share = RailEntry {
        group: Group::Network,
        kind: Kind::Share,
        label: "NAS".into(),
        uri: "smb://nas/share".into(),
        glyph: "server",
        ..RailEntry::default()
    };
    rows(&shell, vec![share], cx);
    let heard = Rc::new(RefCell::new(Vec::<Request>::new()));
    let sink = heard.clone();
    cx.update(|_, cx| {
        cx.subscribe(&shell, move |_, request: &Request, _| {
            sink.borrow_mut().push(request.clone())
        })
        .detach()
    });
    shell.update(cx, |shell, cx| shell.rail_focus(true, cx));
    cx.simulate_keystrokes("f2");
    frame(cx);
    let centre = producer(&shell, cx).read_with(cx, |producer, _| {
        let laid = producer.laid.borrow();
        let editor = laid.row(0).unwrap().editor.unwrap();
        point(px(editor.x + editor.width / 2.), px(editor.y + editor.height / 2.))
    });
    heard.borrow_mut().clear();
    cx.simulate_mouse_move(centre, None, Modifiers::none());
    cx.simulate_mouse_down(centre, MouseButton::Left, Modifiers::none());
    frame(cx);
    assert!(
        !heard.borrow().iter().any(|request| matches!(request.event, Event::Activate(_))),
        "clicking the rename field emitted remote activation: {:?}",
        heard.borrow()
    );
}

#[gpui::test]
fn review_u08_r1_recent_completion_keeps_origin_pane(cx: &mut TestAppContext) {
    let (shell, cx) = open(false, cx);
    shell.update(cx, |shell, cx| {
        shell.view = crate::window::startup::View::Dual;
        shell.panes[1] = Some(crate::window::pane::PaneHost::new(
            "/right".into(), None, 620., &shell.locale,
        ));
        let producer = shell.rail.as_ref().unwrap().places.clone();
        producer.update(cx, |producer, cx| {
            producer.read_recent(
                concat!(env!("CARGO_MANIFEST_DIR"), "/src/g2/pl4/recent-fixture.xbel").into(),
                cx,
            )
        });
        shell.focus_pane(1, cx);
    });
    cx.run_until_parked();
    frame(cx);
    let (left, right) = shell.read_with(cx, |shell, _| {
        (
            shell.panes[0].as_ref().unwrap().recent.on,
            shell.panes[1].as_ref().unwrap().recent.on,
        )
    });
    assert!(left, "Recent started on the left and must complete there");
    assert!(!right, "Recent started on the left but was applied to the right");
}

#[gpui::test]
fn review_u08_r1_current_place_keeps_deepest_ancestor_highlight(cx: &mut TestAppContext) {
    let (shell, cx) = open(false, cx);
    rows(&shell, vec![row("Home", "/home/test"), row("Documents", "/home/test/Documents")], cx);
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().path = "/home/test/Documents/child".into();
    });
    frame(cx);
    let lit = producer(&shell, cx).read_with(cx, |producer, _| producer.model.borrow().lit);
    assert_eq!(lit, Some(1), "the deepest containing place must remain lit inside a child directory");
}
}
#[path = "fixreaders_rail_tests.rs"]
mod fixreaders;
#[path = "tag_places_tests.rs"]
mod tag_places_tests;
