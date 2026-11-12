//! Controller native findings: walked folder bytes and selection from the same painted frame.
use super::{ask, frame, open};

#[path = "fx400_tests.rs"]
mod fx400;

#[path = "s4_size_tests.rs"]
mod s4_size_tests;

#[path = "join_cursor_tests.rs"]
mod cursor;
#[path = "join_size_key_tests.rs"]
mod size_keys;
#[path = "join_tap_tests.rs"]
mod tap;
use crate::{
    backend::{Backend, Event},
    list::{bench, pane::Held},
    palette::Theme,
};
use flea::jsondoc::{self, Json};
use gpui::{TestAppContext, WindowHandle};
use std::{
    process::Command,
    rc::Rc,
    time::{Duration, Instant},
};

fn folders(cx: &mut TestAppContext, class: &str) -> WindowHandle<crate::state::Shell> {
    let handle = open(cx);
    let script = r#"import json,sys
for line in sys.stdin:
 request=json.loads(line)
 if request['c']=='noop': continue
 if request['c']=='quit': break
 print(json.dumps(dict(t='paths',paths=[line])),flush=True)
 if request['c']=='fsinfo':
  print(json.dumps(dict(t='fsinfo',path='/files',**{'class':sys.argv[1]})),flush=True)
 if request['c']=='dirsize':
  for row in request['rows']:
   print(json.dumps(dict(t='dirsized',row=row,bytes=243+row,partial=row==1)),flush=True)
"#;
    let mut command = Command::new("python3");
    command.args(["-u", "-c", script, class]);
    let backend = Backend::start_with(command, r#"{"c":"noop"}"#.into()).unwrap();
    handle
        .update(cx, |shell, _, _| {
            // The backend receipts every command, and these tests count folder walks only.
            shell.launch.settings.data =
                jsondoc::parse(r#"{"preview":{"thumbnails":"off"}}"#).unwrap();
            let host = shell.panes[0].as_mut().unwrap();
            host.pane.held = Rc::new(Held {
                listing: 2,
                total: 2000,
                start: 0,
                rows: bench::fixture("2k"),
            });
            host.backend = Some(backend);
        })
        .unwrap();
    handle
}

fn next(cx: &mut TestAppContext, handle: WindowHandle<crate::state::Shell>) -> Event {
    let replies = handle
        .update(cx, |shell, _, _| {
            shell.panes[0]
                .as_ref()
                .unwrap()
                .backend
                .as_ref()
                .unwrap()
                .replies
                .clone()
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Ok(event) = replies.try_recv() {
            return event;
        }
        assert!(
            Instant::now() < deadline,
            "the live list never sent its visible-folder request"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn request(cx: &mut TestAppContext, handle: WindowHandle<crate::state::Shell>) -> Json {
    let event = next(cx, handle);
    let Event::Paths(lines) = event else {
        panic!("missing command receipt: {event:?}")
    };
    jsondoc::parse(&lines[0]).unwrap()
}

fn receive(cx: &mut TestAppContext, handle: WindowHandle<crate::state::Shell>, event: Event) {
    handle
        .update(cx, |shell, window, cx| shell.receive(0, event, window, cx))
        .unwrap();
}

fn advance(cx: &mut TestAppContext, ms: u64) {
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(ms));
    cx.run_until_parked();
}

fn settle(cx: &mut TestAppContext) {
    advance(cx, 120);
}

#[track_caller]
fn quiet(cx: &mut TestAppContext, handle: WindowHandle<crate::state::Shell>) {
    let replies = handle
        .update(cx, |shell, _, _| {
            shell.panes[0]
                .as_ref()
                .unwrap()
                .backend
                .as_ref()
                .unwrap()
                .replies
                .clone()
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_millis(40);
    while Instant::now() < deadline {
        assert!(
            replies.is_empty(),
            "a cancelled, early or hidden viewport issued work: {:?}",
            replies.try_recv()
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn classify(cx: &mut TestAppContext, handle: WindowHandle<crate::state::Shell>) {
    frame(cx, handle);
    assert_eq!(
        request(cx, handle).get("c").and_then(Json::as_str),
        Some("fsinfo")
    );
    let event = next(cx, handle);
    receive(cx, handle, event);
    frame(cx, handle);
}

#[test]
fn u04_folder_size_requests_settle_on_visible_directories_and_cancel_stale_answers() {
    let mut cx = bench::real_cx();
    let handle = folders(&mut cx, "");
    classify(&mut cx, handle);
    assert_eq!(ask(&mut cx, handle, "rowSizeText", &["0"]), "\u{b7}");
    settle(&mut cx);
    let command = request(&mut cx, handle);
    assert_eq!(command.get("c").and_then(Json::as_str), Some("dirsize"));
    assert_eq!(ask(&mut cx, handle, "dirSizeRequests", &[]), "1");
    assert_eq!(command.get("listing").and_then(Json::as_f64), Some(2.));
    let rows = command.get("rows").and_then(Json::as_array).unwrap();
    assert_eq!(
        rows.len(),
        8,
        "the request walked beyond the visible directories"
    );
    for (at, row) in rows.iter().enumerate() {
        assert_eq!(row.as_f64(), Some(at as f64));
    }
    let stale: Vec<_> = (0..rows.len()).map(|_| next(&mut cx, handle)).collect();
    handle
        .update(&mut cx, |shell, _, _| {
            shell.panes[0].as_mut().unwrap().pane.scroll_y = 31. * 30.
        })
        .unwrap();
    frame(&mut cx, handle);
    assert_eq!(
        request(&mut cx, handle).get("c").and_then(Json::as_str),
        Some("dirsizecancel")
    );
    for event in stale {
        receive(&mut cx, handle, event);
    }
    handle
        .update(&mut cx, |shell, _, _| {
            shell.panes[0].as_mut().unwrap().pane.scroll_y = 0.
        })
        .unwrap();
    frame(&mut cx, handle);
    assert_eq!(ask(&mut cx, handle, "rowSizeText", &["0"]), "\u{b7}");
    advance(&mut cx, 70);
    quiet(&mut cx, handle);
    advance(&mut cx, 49);
    quiet(&mut cx, handle);
    advance(&mut cx, 1);
    assert_eq!(
        request(&mut cx, handle).get("c").and_then(Json::as_str),
        Some("dirsize")
    );
    assert_eq!(
        ask(&mut cx, handle, "dirSizeRequests", &[]),
        "2",
        "a dirsizecancel is not a size request"
    );
    let replies: Vec<_> = (0..8).map(|_| next(&mut cx, handle)).collect();
    handle
        .update(&mut cx, |shell, window, cx| {
            for event in replies {
                shell.receive(0, event, window, cx);
            }
            // Check before TestAppContext services the notified frame at this update's boundary.
            let (answer, read) = std::sync::mpsc::sync_channel(1);
            shell.ipc(
                crate::ipc::server::Event::Call {
                    request: flea::ipc::wire::Request::Call {
                        target: "flea".into(),
                        function: "rowSizeText".into(),
                        arguments: vec!["0".into()],
                    },
                    answer,
                },
                window,
                cx,
            );
            let reply = read.try_recv().unwrap().reply;
            assert_eq!(reply.code, 0);
            assert_eq!(
                reply.stdout.trim(),
                "\u{b7}",
                "an unpainted reply leaked into IPC"
            );
        })
        .unwrap();
    frame(&mut cx, handle);
    assert_eq!(ask(&mut cx, handle, "rowSizeText", &["0"]), "243 B");
    assert_eq!(ask(&mut cx, handle, "rowSizeText", &["1"]), ">244 B");
    settle(&mut cx);
    handle
        .update(&mut cx, |shell, _, _| {
            assert!(
                shell.panes[0]
                    .as_ref()
                    .unwrap()
                    .backend
                    .as_ref()
                    .unwrap()
                    .replies
                    .is_empty(),
                "answered folders were walked again"
            );
        })
        .unwrap();
}

#[test]
fn u04_hidden_size_and_remote_classes_never_request_folder_walks() {
    for (class, hide) in [("network", false), ("phone", false), ("", true)] {
        let mut cx = bench::real_cx();
        let handle = folders(&mut cx, class);
        if hide {
            handle
                .update(&mut cx, |shell, _, _| {
                    shell.launch.settings.data =
                        jsondoc::parse(r#"{"columns":["mode"],"preview":{"thumbnails":"off"}}"#)
                            .unwrap();
                })
                .unwrap();
        }
        classify(&mut cx, handle);
        settle(&mut cx);
        handle
            .update(&mut cx, |shell, _, _| {
                assert!(
                    shell.panes[0]
                        .as_ref()
                        .unwrap()
                        .backend
                        .as_ref()
                        .unwrap()
                        .replies
                        .is_empty(),
                    "a hidden Size or remote class requested a walk"
                );
            })
            .unwrap();
        assert_eq!(
            ask(&mut cx, handle, "rowSizeText", &["0"]),
            if hide { "" } else { "\u{b7}" }
        );
    }
}

#[test]
fn u04_selection_uses_the_drawn_theme_role_and_lifts_ink_in_the_same_frame() {
    let mut cx = bench::real_cx();
    let handle = open(&mut cx);
    let original = Theme::default();
    let mut marked = Vec::new();
    for theme in [
        original,
        Theme {
            background: original.foreground,
            foreground: original.background,
            ..original
        },
    ] {
        cx.update(|cx| cx.set_global(theme));
        handle
            .update(&mut cx, |shell, _, _| {
                shell.panes[0].as_mut().unwrap().answered = true;
                let scene = jsondoc::parse(r#"{"cursor":0,"selection":[1999,1,1],"text_size":14,"reduced_motion":true,"density":"compact","caret_blink":false,"view":"list","corner_radius":0}"#).unwrap();
                let settings = jsondoc::parse(r#"{"path":"/files"}"#).unwrap();
                let spec = crate::visual::contract::Spec::from_scene("u04-selection", &scene, &settings).unwrap();
                let (answer, _) = std::sync::mpsc::sync_channel(1);
                shell.visual.pending = Some(crate::visual::Pending { spec, answer });
            })
            .unwrap();
        frame(&mut cx, handle);
        let foreground = handle
            .update(&mut cx, |shell, _, _| {
                assert_eq!(shell.panes[0].as_ref().unwrap().selected, [1, 1999]);
                let list = shell.panes[0].as_ref().unwrap().list.borrow();
                let shown = list
                    .visible()
                    .iter()
                    .find(|shown| shown.index == 1)
                    .unwrap();
                assert!(
                    shown.state.selected,
                    "the live join dropped the pane's selection"
                );
                assert!(!shown.state.cursor);
                let palette = list.drawn.palette.unwrap();
                marked.push(palette.marked);
                crate::ipc::rows::colour(palette.foreground)
            })
            .unwrap();
        assert_eq!(ask(&mut cx, handle, "rowNameColor", &["1"]), foreground);
        assert_eq!(ask(&mut cx, handle, "rowCellColor", &["1"]), foreground);
    }
    assert_ne!(
        marked[0], marked[1],
        "the selection plate kept a fixed grey across themes"
    );
}

#[test]
fn u04_preference_marks_are_normalized_before_list_membership() {
    let mut cx = bench::real_cx();
    let handle = open(&mut cx);
    handle
        .update(&mut cx, |shell, window, cx| {
            let host = shell.panes[0].as_mut().unwrap();
            let preference = crate::navigation::anchor::Preference {
                path: host.path.clone(),
                name: host.pane.held.rows[0].name.clone(),
                index: 0,
                start: 0,
                selected: [2, 1, 1]
                    .into_iter()
                    .map(|at| host.pane.held.rows[at].name.clone())
                    .collect(),
            };
            preference.apply(&mut host.pane, &mut host.selected);
            assert_eq!(host.selected, [1, 2]);
            shell.run_navigation(0, window, cx);
        })
        .unwrap();
    frame(&mut cx, handle);
    handle
        .update(&mut cx, |shell, _, _| {
            let list = shell.panes[0].as_ref().unwrap().list.borrow();
            for at in [1, 2] {
                assert!(
                    list.visible()
                        .iter()
                        .find(|row| row.index == at)
                        .unwrap()
                        .state
                        .selected
                );
            }
        })
        .unwrap();
}

#[path = "fixreaders_steering_tests.rs"]
mod fixreaders;

#[path = "join_filter_sizes_tests.rs"]
mod filter_sizes;
