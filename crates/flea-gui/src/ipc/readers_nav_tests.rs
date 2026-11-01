//! U-READERS-NAV: the readers a Boat scene needs to aim and judge a permanent delete, a crumb drop and a drag in flight.
use crate::window::dispatch::test_shell::{land, open_root};
use crate::{ipc::server::Event, state::Shell};
use flea::{
    ipc::wire::{Reply, Request},
    jsondoc::{self, Json},
    protocol::RowFacts,
    ui_logic_op1::{
        confirm::Source,
        trash::{Review, Scope},
    },
};
use gpui::{Entity, TestAppContext, VisualTestContext};

fn reply(shell: &Entity<Shell>, cx: &mut VisualTestContext, name: &str, args: &[&str]) -> Reply {
    cx.run_until_parked();
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let (answer, receive) = std::sync::mpsc::sync_channel(1);
            shell.ipc(
                Event::Call {
                    request: Request::Call {
                        target: "flea".into(),
                        function: name.into(),
                        arguments: args.iter().map(|arg| (*arg).into()).collect(),
                    },
                    answer,
                },
                window,
                cx,
            );
            receive
                .try_recv()
                .expect("the mounted shell answered")
                .reply
        })
    })
}

fn read(shell: &Entity<Shell>, cx: &mut VisualTestContext, name: &str, args: &[&str]) -> String {
    let reply = reply(shell, cx, name, args);
    assert_eq!(reply.code, 0, "{name}: {reply:?}");
    reply.stdout.trim_end().to_owned()
}

fn json(shell: &Entity<Shell>, cx: &mut VisualTestContext, name: &str) -> Json {
    jsondoc::parse(&read(shell, cx, name, &[])).unwrap()
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

fn text<'a>(doc: &'a Json, path: &[&str]) -> Option<&'a str> {
    path.iter()
        .try_fold(doc, |doc, key| doc.get(key))
        .and_then(Json::as_str)
}

fn row(name: &str, dir: bool) -> RowFacts {
    RowFacts {
        name: name.into(),
        dir,
        ..RowFacts::default()
    }
}

// EY-387: Shift+Delete's review card answers opened with v0.3.12's action, facts and confirmation, not a closed card.
#[gpui::test]
fn readers_nav_menu_dialog_state_answers_the_permanent_delete_card(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", vec![row("notes.txt", false)], 1);
    assert_eq!(
        read(&shell, cx, "menuDialogState", &[]),
        "{\"opened\":false}"
    );
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let review = Review {
                count: Some(1),
                bytes: 99,
                token: "1".into(),
                ..Default::default()
            };
            shell.confirm_operation(0, review, Scope::Items, Source::Menu(1), window, cx);
        })
    });
    draw(cx);
    let state = json(&shell, cx, "menuDialogState");
    assert_eq!(state.get("opened"), Some(&Json::Bool(true)), "{state:?}");
    assert_eq!(text(&state, &["action"]), Some("deletePermanently"));
    assert_eq!(state.get("busy"), Some(&Json::Bool(false)));
    let facts = state.get("facts").unwrap();
    assert_eq!(facts.get("count").and_then(Json::as_f64), Some(1.));
    assert_eq!(facts.get("bytes").and_then(Json::as_f64), Some(99.));
    assert_eq!(facts.get("token").and_then(Json::as_f64), Some(1.));
    assert_eq!(text(&state, &["facts", "op"]), Some("prepareDelete"));
    let confirmation = state.get("confirmation").unwrap();
    assert_eq!(confirmation.get("opened"), Some(&Json::Bool(true)));
    assert_eq!(
        text(&state, &["confirmation", "title"]),
        Some("Delete 1 item permanently?")
    );
    assert_eq!(
        confirmation.get("destructiveFocus"),
        Some(&Json::Bool(false))
    );
    for control in ["cancel", "danger"] {
        let centre = text(&state, &["confirmation", control, "centre"]).unwrap();
        assert_eq!(
            centre.split_whitespace().count(),
            2,
            "{control}: {centre:?}"
        );
        assert_eq!(
            confirmation.get(control).and_then(|c| c.get("visible")),
            Some(&Json::Bool(true)),
            "{control}"
        );
    }
    assert_eq!(
        text(&state, &["confirmation", "cancel", "name"]),
        Some("Cancel")
    );
    assert_eq!(
        text(&state, &["confirmation", "danger", "name"]),
        Some("Delete")
    );
    assert_eq!(
        text(&state, &["rect"]).unwrap().split_whitespace().count(),
        4
    );
    // The Trash view's own card is trashState's, so the menu reader stays closed for it.
    cx.update(|_, cx| {
        let overlay = shell.read(cx).overlay.clone().expect("the card's overlay");
        overlay.update(cx, |overlay, cx| {
            overlay.model.present_external_delete(
                Review {
                    count: Some(2),
                    token: "9".into(),
                    ..Default::default()
                },
                Scope::TrashItems,
                9,
            );
            cx.notify();
        });
    });
    draw(cx);
    assert_eq!(
        read(&shell, cx, "menuDialogState", &[]),
        "{\"opened\":false}"
    );
}

// sa1030.drag.crumb-drop: a scene aims at the crumb drawn as a folder's name, wherever the fixture root puts it.
#[gpui::test]
fn readers_nav_crumb_centre_of_answers_a_crumb_by_its_label(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        let now = host.pane.now();
        assert!(host.pane.begin(
            "/files/A/full".into(),
            crate::pane::swap::Options {
                supersede: true,
                ..Default::default()
            },
            now
        ));
    });
    land(&shell, cx, "/files/A/full", vec![row("f1.txt", false)], 2);
    assert_eq!(read(&shell, cx, "crumbText", &[]), "/files/A/full");
    let count: usize = read(&shell, cx, "crumbCount", &[]).parse().unwrap();
    let named = read(&shell, cx, "crumbCentreOf", &["A"]);
    assert!(!named.is_empty(), "the A crumb is drawn");
    assert_eq!(
        named,
        read(&shell, cx, "crumbCentre", &[&(count - 2).to_string()])
    );
    assert_eq!(
        read(&shell, cx, "crumbCentreOf", &["full"]),
        read(&shell, cx, "crumbCentre", &[&(count - 1).to_string()])
    );
    assert_eq!(
        read(&shell, cx, "crumbCentreOf", &["/"]),
        read(&shell, cx, "crumbCentre", &["0"])
    );
    assert_eq!(read(&shell, cx, "crumbCentreOf", &["missing"]), "");
    assert_eq!(read(&shell, cx, "crumbCentreOf", &["A/"]), named);
}

// sa1030.drag.into-self-refused: a drag in flight reads as active with its rows and the folder a drop there lands in.
#[gpui::test]
fn readers_nav_drag_state_answers_the_drag_in_flight(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(
        &shell,
        cx,
        "/files",
        vec![row("full", true), row("notes.txt", false)],
        1,
    );
    assert_eq!(
        json(&shell, cx, "dragState"),
        jsondoc::parse("{\"active\":false}").unwrap()
    );
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().selected = vec![0]
    });
    let rect: Vec<f32> = read(&shell, cx, "rowNameRect", &["0"])
        .split_whitespace()
        .map(|value| value.parse().unwrap())
        .collect();
    let from = gpui::point(gpui::px(rect[0] + 5.), gpui::px(rect[1] + rect[3] / 2.));
    cx.simulate_mouse_move(from, None, gpui::Modifiers::default());
    cx.simulate_mouse_down(from, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.update(|window, cx| {
        shell.update(
            cx,
            |shell, cx| assert!(shell.begin_file_drag(0, window, cx)),
        )
    });
    let state = json(&shell, cx, "dragState");
    assert_eq!(state.get("active"), Some(&Json::Bool(true)), "{state:?}");
    assert_eq!(state.get("count").and_then(Json::as_f64), Some(1.));
    assert_eq!(state.get("side").and_then(Json::as_f64), Some(0.));
    assert_eq!(
        state.get("paths"),
        Some(&Json::Arr(vec![Json::Str("/files/full".into())]))
    );
    assert_eq!(state.get("travelled"), Some(&Json::Bool(false)));
    // Over its own row the carried folder is skipped, so the drop would land in the folder it is already in.
    assert_eq!(text(&state, &["target"]), Some("/files"));
    assert_eq!(state.get("overSide").and_then(Json::as_f64), Some(0.));
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.forget_file_drags(window, cx)));
    assert_eq!(
        json(&shell, cx, "dragState"),
        jsondoc::parse("{\"active\":false}").unwrap()
    );
    cx.simulate_mouse_up(from, gpui::MouseButton::Left, gpui::Modifiers::default());
}
