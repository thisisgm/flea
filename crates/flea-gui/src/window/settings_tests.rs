use crate::{backend::Backend, ipc::server::Event, state::Shell};
use flea::ipc::wire::{Reply, Request};
use gpui::{AppContext, TestAppContext};

#[path = "settings_cover_tests.rs"]
mod cover;
#[path = "m9_g2rows_02_tests.rs"]
mod m9_g2rows_02;
#[path = "m9_g2rows_02_plugins_tests.rs"]
mod m9_g2rows_02_plugins;

fn configured(backend: Backend, cx: &mut gpui::Context<Shell>) -> Shell {
    let mut shell = crate::state::tests::shell_with(backend, cx);
    let data = flea::uistate::from_file("{}");
    *shell.panes[0].as_ref().unwrap().nav.folders.borrow_mut() =
        crate::navigation::folder_state::Folders::from_settings(&data, Vec::new());
    shell.launch.settings.data = data;
    shell
}

fn read(
    shell: &mut Shell,
    name: &str,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<Shell>,
) -> Reply {
    let (answer, receive) = std::sync::mpsc::sync_channel(1);
    shell.ipc(
        Event::Call {
            request: Request::Call {
                target: "flea".into(),
                function: name.into(),
                arguments: vec![],
            },
            answer,
        },
        window,
        cx,
    );
    receive.try_recv().unwrap().reply
}

#[gpui::test]
fn u11_settings_open_and_escape_restore_the_listing(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let shell = configured(backend, cx);
        window.focus(&shell.focus, cx);
        shell
    });
    cx.simulate_keystrokes(",");
    shell.update_in(cx, |shell, window, cx| {
        assert_eq!(
            read(shell, "settingsOpen", window, cx),
            Reply::value("true")
        );
        assert_eq!(shell.panes[0].as_ref().unwrap().pane.cursor, 0);
    });
    cx.simulate_keystrokes("escape");
    shell.update_in(cx, |shell, window, cx| {
        assert_eq!(
            read(shell, "settingsOpen", window, cx),
            Reply::value("false")
        );
        assert!(shell.focus.is_focused(window));
    });
}

#[gpui::test]
fn u11_closed_settings_never_expose_a_control_rectangle(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        let shell = cx.new(|cx| configured(backend, cx));
        shell.update(cx, |shell, cx| {
            assert_eq!(
                read(shell, "settingsCardRect", window, cx),
                Reply::value("")
            );
            assert_eq!(
                read(shell, "keymapSheetOpen", window, cx),
                Reply::value("false")
            );
            assert_eq!(read(shell, "settingsModel", window, cx), Reply::value("[]"));
            assert_eq!(
                read(shell, "keymapSheetResults", window, cx),
                Reply::value("[]")
            );
            assert_eq!(read(shell, "thumbnailPolicyState", window, cx).code, 0);
            assert_eq!(
                read(shell, "settingsScrollState", window, cx),
                Reply::value("{}")
            );
            assert_eq!(read(shell, "settingsScroll", window, cx), Reply::value(""));
        });
    });
}

#[gpui::test]
fn u11_drawn_geometry_moves_with_the_real_window(cx: &mut TestAppContext) {
    use gpui::{px, size};
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let shell = configured(backend, cx);
        window.focus(&shell.focus, cx);
        shell
    });
    cx.simulate_resize(size(px(1100.), px(700.)));
    cx.simulate_keystrokes(",");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let first = shell.update_in(cx, |shell, window, cx| {
        read(shell, "settingsCardRect", window, cx).stdout
    });
    assert_eq!(first.trim(), "170 8 760 684");
    shell.update_in(cx, |shell, window, cx| {
        let state =
            flea::jsondoc::parse(read(shell, "settingsScrollState", window, cx).stdout.trim())
                .unwrap();
        let at = |path: &[&str]| {
            path.iter()
                .try_fold(&state, |value, key| value.get(key))
                .and_then(flea::jsondoc::Json::as_f64)
                .unwrap()
        };
        assert!(at(&["compactHeight"]) > 0.);
        assert!(at(&["pane", "height"]) > 0. && at(&["pane", "contentHeight"]) > 0.);
        assert_eq!(at(&["pane", "y"]), 0.);
        assert!(
            at(&["rail", "contentHeight"]) > 0.
                && at(&["rail", "contentHeight"]) <= at(&["rail", "height"])
        );
        assert_eq!(
            read(shell, "settingsScroll", window, cx).stdout.trim(),
            format!(
                "{}|{}",
                at(&["pane", "contentHeight"]).round(),
                at(&["pane", "height"]).round()
            )
        );
    });
    cx.simulate_resize(size(px(800.), px(500.)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    shell.update_in(cx, |shell, window, cx| {
        let second = read(shell, "settingsCardRect", window, cx).stdout;
        assert_eq!(second.trim(), "20 8 760 484");
        let view = shell.settings.panel.as_ref().unwrap().read(cx);
        assert!(!view.probe.centre("rail:display", "card").is_empty());
        assert_eq!(view.probe.centre("row:missing", "pane"), "");
        assert_ne!(first, second);
    });
}

// EY-207: the title's box is the drawn title, so its centre sits over the text at the card's left, not the card's middle.
#[gpui::test]
fn ey207_settings_title_centre_is_the_drawn_title_not_the_card_middle(cx: &mut TestAppContext) {
    use gpui::{px, size};
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let shell = configured(backend, cx);
        window.focus(&shell.focus, cx);
        shell
    });
    cx.simulate_keystrokes(",");
    cx.simulate_resize(size(px(800.), px(500.)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    shell.update_in(cx, |shell, window, cx| {
        let numbers = |reply: String| -> Vec<f32> {
            reply
                .split_whitespace()
                .map(|v| v.parse::<f32>().unwrap())
                .collect()
        };
        let card = numbers(read(shell, "settingsCardRect", window, cx).stdout);
        let title = numbers(read(shell, "settingsTitleCentre", window, cx).stdout);
        assert!(
            title[0] < card[0] + card[2] / 2.,
            "the drawn title sits left of the card's middle: {title:?} in {card:?}"
        );
        let view = shell.settings.panel.as_ref().unwrap().read(cx);
        let drawn = view.probe.bounds("title").expect("the title is drawn");
        assert!(
            f32::from(drawn.size.width) < card[2] / 2.,
            "the title box is the title's own width, not the head row: {drawn:?} in {card:?}"
        );
    });
}

// EY-150: after comma, Tab, Tab the reader names v0.3.12's focus (the panel's window-filling keys item) and each drawn row.
#[gpui::test]
fn ey150_cohesion_controls_reads_settings_focus_and_rows(cx: &mut TestAppContext) {
    use gpui::{px, size};
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let shell = configured(backend, cx);
        window.focus(&shell.focus, cx);
        shell
    });
    cx.simulate_keystrokes(", tab tab");
    cx.simulate_resize(size(px(800.), px(500.)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    shell.update_in(cx, |shell, window, cx| {
        let reply = read(shell, "cohesionControls", window, cx);
        assert!(reply.stderr.is_empty(), "{}", reply.stderr);
        let doc = flea::jsondoc::parse(&reply.stdout).expect("a JSON document");
        let focus = doc.get("focus").expect("a focus member");
        let field = |key: &str| focus.get(key).and_then(|v| v.as_f64());
        assert_eq!(
            focus.get("kind").and_then(|v| v.as_str()),
            Some("QQuickItem")
        );
        assert_eq!(focus.get("via").and_then(|v| v.as_str()), Some("item"));
        assert_eq!(focus.get("focused").and_then(|v| v.as_bool()), Some(true));
        let whole = [field("x"), field("y"), field("w"), field("h")];
        assert_eq!(whole, [Some(0.), Some(0.), Some(800.), Some(500.)]);
        let controls = doc.get("controls").and_then(|v| v.as_array()).unwrap();
        assert!(!controls.is_empty(), "the View pane's rows are drawn");
        let card = read(shell, "settingsCardRect", window, cx).stdout;
        let card: Vec<f64> = card
            .split_whitespace()
            .map(|v| v.parse().unwrap())
            .collect();
        for control in controls {
            assert_eq!(
                control.get("kind").and_then(|v| v.as_str()),
                Some("SettingsRow")
            );
            let x = control.get("x").and_then(|v| v.as_f64()).unwrap();
            assert!(x >= card[0] && x < card[0] + card[2], "{x} in {card:?}");
        }
    });
}

#[gpui::test]
fn u11_live_preset_and_density_have_real_consumers(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let shell = configured(backend, cx);
        window.focus(&shell.focus, cx);
        shell
    });
    cx.simulate_keystrokes(",");
    shell.update(cx, |shell, cx| {
        let view = shell.settings.panel.as_ref().unwrap();
        view.update(cx, |view, cx| {
            view.panel.show_section("keys");
            cx.notify();
        });
    });
    cx.simulate_keystrokes("l l l");
    shell.update_in(cx, |shell, window, cx| {
        assert_eq!(
            read(shell, "keymapPreset", window, cx),
            Reply::value("windows")
        );
        let view = shell.settings.panel.as_ref().unwrap();
        view.update(cx, |view, cx| {
            view.panel.show_section("view");
            view.panel.cursor = view
                .panel
                .rows
                .iter()
                .position(|r| r.id == "density")
                .unwrap();
            cx.notify();
        });
    });
    cx.simulate_keystrokes("l");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    shell.update_in(cx, |shell, window, cx| {
        assert_eq!(
            shell
                .launch
                .settings
                .data
                .get("density")
                .and_then(flea::jsondoc::Json::as_str),
            Some("normal")
        );
        assert_eq!(
            shell.panes[0]
                .as_ref()
                .unwrap()
                .list
                .borrow()
                .drawn
                .metrics
                .row,
            37.
        );
        assert_eq!(
            read(shell, "settingsOpen", window, cx),
            Reply::value("true")
        );
    });
    cx.simulate_keystrokes("escape");
    cx.simulate_keystrokes("ctrl-h");
    shell.update_in(cx, |shell, window, cx| {
        assert_eq!(read(shell, "showHidden", window, cx), Reply::value("true"));
        assert_eq!(shell.view, crate::window::startup::View::List);
    });
}

#[gpui::test]
fn u11_external_settings_preserve_each_windows_path_cursor_and_view(cx: &mut TestAppContext) {
    let mut a = cx.add_empty_window().clone();
    let mut b = cx.add_empty_window().clone();
    let (backend_a, _requests_a) = Backend::test_channel();
    let (backend_b, _requests_b) = Backend::test_channel();
    let left = a.update(|_, cx| cx.new(|cx| configured(backend_a, cx)));
    let right = b.update(|_, cx| cx.new(|cx| configured(backend_b, cx)));
    b.update(|_, cx| {
        right.update(cx, |shell, _| {
            shell.view = crate::window::startup::View::Grid;
            let host = shell.panes[0].as_mut().unwrap();
            host.path = "/other".into();
            host.pane = crate::pane::Pane::new("/other".into(), 620.);
            host.pane
                .receive(flea::protocol::Reply::Listed {
                    path: "/other".into(),
                    total: 10,
                })
                .unwrap();
            host.pane
                .receive(flea::protocol::Reply::Rows {
                    start: 0,
                    rows: vec![flea::protocol::RowFacts::default(); 10],
                    listing: 3,
                })
                .unwrap();
            host.pane.cursor = 4;
        })
    });
    for (window, entity) in [(&mut a, left), (&mut b, right.clone())] {
        window.update(|window, cx| {
            entity.update(cx, |shell, cx| {
                let document = flea::uistate::from_file(
                    r#"{"hidden":true,"density":"comfortable","view":"columns"}"#,
                );
                shell.apply_settings(document, window, cx);
                assert_eq!(
                    shell.panes[0]
                        .as_ref()
                        .unwrap()
                        .list
                        .borrow()
                        .file_metrics()
                        .row,
                    45.
                );
            })
        });
    }
    b.update(|_, cx| {
        right.read_with(cx, |shell, _| {
            assert_eq!(shell.view, crate::window::startup::View::Grid);
            assert_eq!(shell.panes[0].as_ref().unwrap().path, "/other");
            assert_eq!(shell.panes[0].as_ref().unwrap().pane.cursor, 4);
        })
    });
}
