//! Network and phone text gates and terminal actions through the production window.
use super::test_shell::{land, open_root, open_root_with, row};
use crate::backend::Event;
use crate::window::selection_preview::{docs::Doc, Mounted};
use gpui::TestAppContext;
use std::cell::RefCell;

thread_local! { static LAUNCHED: RefCell<Vec<Vec<String>>> = const { RefCell::new(Vec::new()) }; }

#[gpui::test]
fn pb_network_2_control_t_opens_the_shown_directory(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", vec![row("child", true)], 1);
    LAUNCHED.with(|calls| calls.borrow_mut().clear());
    shell.update(cx, |shell, _| {
        shell.opener[0].launch = |argv| {
            LAUNCHED.with(|calls| calls.borrow_mut().push(argv.to_vec()));
            (Some(0), String::new())
        }
    });
    cx.simulate_keystrokes("ctrl-t");
    cx.run_until_parked();
    let calls = LAUNCHED.with(|calls| calls.borrow().clone());
    assert_eq!(calls.len(), 1, "the terminal key must run its launcher");
    assert_eq!(&calls[0][1..], &["--terminal", "/files"]);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_network_2_quick_look_rises_and_keeps_its_close_frame(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", vec![row("opaque.bin", false)], 1);
    cx.update(|_, cx| cx.set_reduce_motion(false));
    cx.simulate_keystrokes("space");
    super::preview_tests::draw(cx);
    let top = |cx: &mut gpui::VisualTestContext| {
        f32::from(
            cx.debug_bounds("pv1-card")
                .expect("the Quick Look card is drawn")
                .origin
                .y,
        )
    };
    let first = top(cx);
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(180));
    super::preview_tests::draw(cx);
    let rest = top(cx);
    assert!((first - rest - 10.).abs() < 0.01);
    let events = std::rc::Rc::new(RefCell::new(Vec::new()));
    let seen = events.clone();
    let subscription = cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            let view = shell.quick_look.as_ref().unwrap().view.clone();
            cx.subscribe(&view, move |_, _, event: &crate::g2::pv1::Event, _| {
                seen.borrow_mut().push(*event)
            })
        })
    });
    cx.simulate_keystrokes("escape");
    super::preview_tests::draw(cx);
    assert_eq!(top(cx), rest);
    let button = cx
        .debug_bounds("pv1-open-with")
        .expect("the retained card still draws its button");
    cx.simulate_click(button.center(), gpui::Modifiers::none());
    assert!(
        !events.borrow().contains(&crate::g2::pv1::Event::OpenWith),
        "a closing card accepts no control action"
    );
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(140));
    cx.run_until_parked();
    assert!(
        shell.read_with(cx, |shell, cx| shell
            .quick_look
            .as_ref()
            .unwrap()
            .view
            .read(cx)
            .closing
            .is_none()),
        "the fade releases its retained subject even without another presented frame"
    );
    super::preview_tests::draw(cx);
    assert!(cx.debug_bounds("pv1-card").is_none());
    drop(subscription);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

fn text_gate(cx: &mut TestAppContext, class: &str, limit: u64) {
    let (shell, cx) = open_root_with(
        cx,
        r#""keys":"default","view":"columns","preview":{"column":true,"thumbNetwork":true,"thumbPhone":true}"#,
        None,
    );
    land(&shell, cx, "/files", Vec::new(), 1);
    let mut file = row("large.txt", false);
    file.mode = 0o100644;
    file.icon = "text-plain".into();
    file.size = 400_000;
    land(&shell, cx, "/files", vec![file], 2);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(
                0,
                Event::ListDetails(crate::list::sizes::Reply::Filesystem {
                    path: "/files".into(),
                    class: class.into(),
                }),
                window,
                cx,
            )
        })
    });
    super::menus_tests::settle(cx);
    let gate = shell.read_with(cx, |shell, cx| {
        let preview = shell.selection_preview(0).expect("the column is mounted");
        let preview = preview.read(cx);
        let frame = preview
            .pending
            .as_ref()
            .or(preview.drawn.as_ref())
            .expect("the selected text has a frame");
        let Mounted::Doc(Doc::Code(code)) = &frame.mounted else {
            panic!("text mounts its line view")
        };
        let code = code.read(cx);
        (code.max_bytes, code.refusal().is_some())
    });
    assert_eq!(gate, (limit, class != "local"), "the {class} text gate");
    if class == "local" {
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                shell.receive(
                    0,
                    Event::ListDetails(crate::list::sizes::Reply::Filesystem {
                        path: "/files".into(),
                        class: "network".into(),
                    }),
                    window,
                    cx,
                )
            })
        });
        super::menus_tests::settle(cx);
        let limit = shell.read_with(cx, |shell, cx| {
            let preview = shell.selection_preview(0).unwrap();
            let preview = preview.read(cx);
            let frame = preview.pending.as_ref().or(preview.drawn.as_ref()).unwrap();
            let Mounted::Doc(Doc::Code(code)) = &frame.mounted else {
                panic!("the text frame")
            };
            code.read(cx).max_bytes
        });
        assert_eq!(
            limit, 262_144,
            "reclassification also replaces the current text policy"
        );
    }
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_network_2_network_text_uses_the_storage_gate(cx: &mut TestAppContext) {
    text_gate(cx, "network", 262_144);
}

#[gpui::test]
fn pb_network_2_phone_text_uses_the_storage_gate(cx: &mut TestAppContext) {
    text_gate(cx, "phone", 262_144);
}

#[gpui::test]
fn pb_network_2_local_text_keeps_the_local_gate(cx: &mut TestAppContext) {
    text_gate(cx, "local", 1_048_576);
}

#[gpui::test]
fn pb_network_2_network_thumbnail_menu_changes_the_class_setting(cx: &mut TestAppContext) {
    use super::menus_tests::{actions, choose, right_click, settle};
    let (shell, cx) = open_root_with(
        cx,
        r#""keys":"default","view":"list","menu":{"hidden":[]}"#,
        None,
    );
    land(&shell, cx, "/files", Vec::new(), 1);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(
                0,
                Event::ListDetails(crate::list::sizes::Reply::Filesystem {
                    path: "/files".into(),
                    class: "network".into(),
                }),
                window,
                cx,
            )
        })
    });
    settle(cx);
    // Below the empty listing the right click lands on the background menu, which holds the view rows.
    right_click(cx, 600., 500.);
    settle(cx);
    assert!(
        actions(&shell, cx).contains(&"extThumbs".to_owned()),
        "a network folder offers its class switch: {:?}, class {:?}",
        actions(&shell, cx),
        shell.read_with(cx, |shell, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .sizes
            .class()
            .map(str::to_owned))
    );
    choose(&shell, cx, "extThumbs");
    let on = shell.read_with(cx, |shell, _| {
        shell
            .launch
            .settings
            .data
            .get("preview")
            .and_then(|preview| preview.get("thumbNetwork"))
            .and_then(flea::jsondoc::Json::as_bool)
    });
    assert_eq!(
        on,
        Some(true),
        "the menu enables thumbnails for the storage class"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_network_2_remote_code_sends_bounded_text_instead_of_its_path(cx: &mut TestAppContext) {
    use super::test_shell::sent;
    use flea::jsondoc::{self, Json};
    use std::{
        path::PathBuf,
        time::{Duration, Instant},
    };
    cx.executor().allow_parking();
    let scratch =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/PB-NETWORK-2");
    std::fs::create_dir_all(&scratch).unwrap();
    let output = std::process::Command::new("mktemp")
        .args(["-d", "--"])
        .arg(scratch.join("text.XXXXXXXX"))
        .output()
        .expect("a private test directory");
    assert!(output.status.success());
    let root = std::env::current_dir()
        .unwrap()
        .join(String::from_utf8(output.stdout).unwrap().trim());
    struct Sandbox(PathBuf);
    impl Drop for Sandbox {
        fn drop(&mut self) {
            assert!(
                self.0.is_absolute()
                    && self.0.parent().is_some()
                    && self.0.join(".flea-test-sandbox").is_file()
            );
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    std::fs::write(root.join(".flea-test-sandbox"), b"").unwrap();
    let sandbox = Sandbox(std::fs::canonicalize(root).unwrap());
    let body = "fn main() {}\n";
    std::fs::write(sandbox.0.join("main.rs"), body).unwrap();
    let path = sandbox.0.to_str().unwrap();
    let (shell, cx) = open_root_with(
        cx,
        r#""keys":"default","view":"columns","preview":{"column":true,"thumbNetwork":true}"#,
        None,
    );
    let mut file = row("main.rs", false);
    file.mode = 0o100644;
    file.icon = "text-x-rust".into();
    file.size = body.len() as u64;
    land(&shell, cx, "/files", Vec::new(), 1);
    cx.simulate_keystrokes("ctrl-l");
    cx.simulate_input(path);
    cx.simulate_keystrokes("enter");
    land(&shell, cx, path, vec![file], 2);
    sent();
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(
                0,
                Event::ListDetails(crate::list::sizes::Reply::Filesystem {
                    path: path.into(),
                    class: "network".into(),
                }),
                window,
                cx,
            )
        })
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut code = None;
    while code.is_none() {
        super::menus_tests::settle(cx);
        code = sent()
            .iter()
            .filter_map(|line| jsondoc::parse(line).ok())
            .find(|doc| doc.get("c").and_then(Json::as_str) == Some("code"));
        assert!(
            code.is_some() || Instant::now() < deadline,
            "the remote read completes"
        );
        if code.is_none() {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    let code = code.unwrap();
    assert_eq!(code.get("text").and_then(Json::as_str), Some(body));
    assert!(
        code.get("path").is_none(),
        "the code backend receives bytes, never the remote path"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
