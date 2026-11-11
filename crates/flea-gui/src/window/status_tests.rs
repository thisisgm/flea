use crate::{backend::Backend, ipc::server::Event, state::Shell};
use flea::ipc::wire::{Reply, Request};
use gpui::{TestAppContext, WindowHandle};
use std::time::Duration;

#[path = "status_backend_fault_tests.rs"]
mod backend_faults;
#[path = "status_card_tests.rs"]
mod cards;
#[path = "status_disk_tests.rs"]
mod disk;
#[path = "status_elide_tests.rs"]
mod elide;
#[path = "status_failure_tests.rs"]
mod failures;
#[path = "status_fsunknown_tests.rs"]
mod fsunknown;
#[path = "status_launch_tests.rs"]
mod launch_tests;
#[path = "status_locked_menu_tests.rs"]
mod locked_menu;
#[path = "status_omi_tests.rs"]
mod omi;
#[path = "status_owner_tests.rs"]
mod owners;
#[path = "status_slow_tests.rs"]
mod slow;

fn open(cx: &mut TestAppContext) -> WindowHandle<Shell> {
    let (backend, _) = Backend::test_channel();
    cx.add_window(|_, cx| crate::state::tests::shell_with(backend, cx))
}

fn read(cx: &mut TestAppContext, window: WindowHandle<Shell>, function: &str) -> Reply {
    window
        .update(cx, |shell, window, cx| {
            let (answer, receive) = std::sync::mpsc::sync_channel(1);
            shell.ipc(
                Event::Call {
                    request: Request::Call {
                        target: "flea".into(),
                        function: function.into(),
                        arguments: vec![],
                    },
                    answer,
                },
                window,
                cx,
            );
            receive.try_recv().unwrap().reply
        })
        .unwrap()
}

fn frame(cx: &mut TestAppContext, window: WindowHandle<Shell>) {
    cx.executor().advance_clock(Duration::from_millis(16));
    cx.run_until_parked();
    window
        .update(cx, |_, window, cx| {
            window.simulate_next_frame(cx);
        })
        .unwrap();
    cx.run_until_parked();
}

fn json(cx: &mut TestAppContext, window: WindowHandle<Shell>, name: &str) -> flea::jsondoc::Json {
    let reply = read(cx, window, name);
    assert_eq!(reply.code, 0, "{name}: {}", reply.stderr);
    flea::jsondoc::parse(reply.stdout.trim()).unwrap()
}

fn escape(cx: &mut TestAppContext, handle: WindowHandle<Shell>) {
    handle
        .update(cx, |shell, window, cx| {
            shell.key(
                &gpui::KeyDownEvent {
                    keystroke: gpui::Keystroke::parse("escape").unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                },
                window,
                cx,
            )
        })
        .unwrap();
}

#[gpui::test]
fn u05_idle_readers_use_live_footer_without_enabling_unowned_verbs(cx: &mut TestAppContext) {
    let window = open(cx);
    frame(cx, window);
    for (name, expected) in [
        ("lastMessage", ""),
        ("statusPrimary", ""),
        ("statusError", "false"),
        ("statusDetail", ""),
        ("stickyMessage", ""),
        ("emptyShown", "false"),
    ] {
        assert_eq!(read(cx, window, name), Reply::value(expected), "{name}");
    }
    assert_eq!(read(cx, window, "thumbnailPolicyState").code, 0);
    let layers = json(cx, window, "stateLayers");
    assert_eq!(layers.get("empty").and_then(|v| v.as_bool()), Some(false));
    assert_eq!(layers.get("message").and_then(|v| v.as_bool()), Some(false));
    let theme = crate::palette::Theme::default();
    let foreground = crate::list::Palette::inherited(&gpui::TextStyle::default(), theme).foreground;
    assert_eq!(
        read(cx, window, "statusColor"),
        Reply::value(crate::ipc::rows::colour(foreground))
    );
}

#[gpui::test]
fn u05_backend_failure_stays_in_footer_after_notice_deadline(cx: &mut TestAppContext) {
    let window = open(cx);
    window
        .update(cx, |shell, window, cx| {
            shell.receive(
                0,
                crate::backend::Event::Failed(crate::backend::Failure {
                    key: "window.backend_stopped",
                    detail: String::new(),
                }),
                window,
                cx,
            );
        })
        .unwrap();
    for _ in 0..300 {
        frame(cx, window);
    }
    assert_eq!(read(cx, window, "statusError"), Reply::value("true"));
    let error = crate::g2::settings_pages::Ink::inherited(
        &gpui::TextStyle::default(),
        crate::palette::Theme::default(),
    )
    .error;
    assert_eq!(
        read(cx, window, "statusColor"),
        Reply::value(crate::ipc::rows::colour(error)),
        "an error draws the centre message in the palette's error role"
    );
    let message = read(cx, window, "lastMessage");
    assert_eq!(message.code, 0);
    assert!(!message.stdout.trim().is_empty());
    assert_eq!(read(cx, window, "emptyShown"), Reply::value("false"));
    let layers = json(cx, window, "stateLayers");
    assert_eq!(layers.get("empty").and_then(|v| v.as_bool()), Some(false));
    assert_eq!(
        layers.get("message").and_then(|v| v.as_bool()),
        Some(true),
        "the failed listing mounts its state message"
    );
    window
        .update(cx, |shell, _, _| {
            let pane = &shell.panes[0].as_ref().unwrap().pane;
            assert_eq!(pane.state.kind, crate::pane::states::Kind::Error);
            assert_eq!(pane.held.total, 0);
        })
        .unwrap();
}

#[gpui::test]
fn u05_errors_queue_escape_acknowledges_one_at_a_time_and_a_newer_notice_replaces_them(
    cx: &mut TestAppContext,
) {
    use flea::ui_logic_status_feedback::status::Hint;
    let handle = open(cx);
    frame(cx, handle);
    let say = |cx: &mut TestAppContext, text: &'static str, error: bool, detail: &'static str| {
        handle
            .update(cx, |shell, _, cx| {
                shell.status_messages(cx, |messages, now| {
                    messages.say(text, error, detail, Hint::None, now)
                });
            })
            .unwrap();
    };
    say(cx, "first failure", true, "disk\nfailed");
    say(cx, "second failure", true, "");
    for _ in 0..320 {
        frame(cx, handle);
    }
    assert_eq!(
        read(cx, handle, "lastMessage"),
        Reply::value("first failure")
    );
    assert_eq!(
        read(cx, handle, "statusDetail"),
        Reply::value("disk\nfailed")
    );
    assert_eq!(
        json(cx, handle, "statusActivityState")
            .get("errors")
            .unwrap()
            .as_f64(),
        Some(2.)
    );
    let footer = json(cx, handle, "statusFooterState");
    assert_eq!(
        footer
            .get("secondary")
            .unwrap()
            .get("text")
            .unwrap()
            .as_str(),
        Some(" · esc dismisses")
    );
    let detail = gpui::VisualTestContext::from_window(handle.into(), cx)
        .debug_bounds("fb1-detail")
        .unwrap();
    assert!(f32::from(detail.size.height) > 0.);
    escape(cx, handle);
    assert_eq!(
        read(cx, handle, "lastMessage"),
        Reply::value("second failure")
    );
    // S1B-2: a newer notice replaces the error still standing, and keeps its own whole time.
    say(cx, "done", false, "");
    assert_eq!(read(cx, handle, "statusError"), Reply::value("false"));
    assert_eq!(read(cx, handle, "statusPrimary"), Reply::value("done"));
    for _ in 0..240 {
        frame(cx, handle);
    }
    assert_eq!(read(cx, handle, "lastMessage"), Reply::value("done"));
    for _ in 0..20 {
        frame(cx, handle);
    }
    assert_eq!(read(cx, handle, "lastMessage"), Reply::value(""));
    frame(cx, handle);
    assert!(gpui::VisualTestContext::from_window(handle.into(), cx)
        .debug_bounds("fb1-detail")
        .is_none());
}

#[gpui::test]
fn u05_footer_reports_painted_counts_filesystem_and_real_activity_owner(cx: &mut TestAppContext) {
    let handle = open(cx);
    cx.simulate_window_resize(handle.into(), gpui::size(gpui::px(1100.), gpui::px(700.)));
    frame(cx, handle);
    handle
        .update(cx, |shell, window, cx| {
            shell.panes[0].as_mut().unwrap().selected = vec![1, 3];
            shell.receive(
                0,
                crate::backend::Event::Filesystem(crate::backend::status::Filesystem {
                    class: String::new(),
                    tagable: None,
                    path: "/files".into(),
                    name: "btrfs".into(),
                    free: Some(442_000_000_000),
                }),
                window,
                cx,
            );
            shell.status_messages(cx, |m, now| {
                m.set_activity(0, "Dragging a file", 0, false, false, now)
            });
        })
        .unwrap();
    frame(cx, handle);
    let footer = json(cx, handle, "statusFooterState");
    let left = footer.get("left").unwrap();
    assert_eq!(
        left.get("text").unwrap().as_str(),
        Some("2 of 1,000 selected · 0 B")
    );
    assert_eq!(
        footer.get("filesystem").unwrap().as_str(),
        Some("btrfs · 442.0 GB free")
    );
    assert_eq!(footer.get("frame").unwrap().as_str(), Some("0 673 1100 27"));
    assert_eq!(left.get("x").unwrap().as_f64(), Some(14.));
    assert_eq!(
        read(cx, handle, "stickyMessage"),
        Reply::value("Dragging a file")
    );
    let activity = json(cx, handle, "statusActivityState");
    let first = &activity.get("activities").unwrap().as_array().unwrap()[0];
    assert_eq!(first.get("ownerPath").unwrap().as_str(), Some("/files"));
    assert_eq!(first.get("ownerFocused").unwrap().as_bool(), Some(true));
    handle
        .update(cx, |shell, window, cx| {
            shell.receive(
                0,
                crate::backend::Event::Filesystem(crate::backend::status::Filesystem {
                    class: String::new(),
                    tagable: None,
                    path: "/old".into(),
                    name: "cifs".into(),
                    free: Some(0),
                }),
                window,
                cx,
            );
        })
        .unwrap();
    frame(cx, handle);
    assert_eq!(
        json(cx, handle, "statusFooterState").get("filesystem"),
        footer.get("filesystem")
    );
}

#[gpui::test]
fn u05_empty_geometry_observes_the_painted_hero_then_drops_it(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    let handle = open(cx);
    cx.simulate_window_resize(handle.into(), gpui::size(gpui::px(1100.), gpui::px(700.)));
    frame(cx, handle);
    assert_eq!(read(cx, handle, "emptyStateRect"), Reply::value(""));
    handle
        .update(cx, |shell, _, cx| {
            shell.panes[0].as_mut().unwrap().pane.state.kind = crate::pane::states::Kind::Empty;
            cx.notify();
        })
        .unwrap();
    frame(cx, handle);
    frame(cx, handle);
    assert_eq!(read(cx, handle, "emptyShown"), Reply::value("true"));
    assert_eq!(
        read(cx, handle, "emptyStateRect"),
        Reply::value("192 54 908 619")
    );
    let mark = gpui::VisualTestContext::from_window(handle.into(), cx)
        .debug_bounds("fb1-hero-mark")
        .unwrap();
    assert_eq!(
        read(cx, handle, "emptyMarkRect"),
        Reply::value(crate::ipc::rows::rect_of(
            f32::from(mark.origin.x),
            f32::from(mark.origin.y),
            f32::from(mark.size.width),
            f32::from(mark.size.height)
        ))
    );
    let state = json(cx, handle, "emptyHeroState");
    assert_eq!(state.get("visible").unwrap().as_bool(), Some(true));
    assert_eq!(state.get("captionOpacity").unwrap().as_f64(), Some(1.));
    assert!(!state.get("caption").unwrap().as_str().unwrap().is_empty());
    handle
        .update(cx, |shell, _, cx| {
            shell.panes[0].as_mut().unwrap().pane.state.kind = crate::pane::states::Kind::Ready;
            cx.notify();
        })
        .unwrap();
    frame(cx, handle);
    assert_eq!(read(cx, handle, "emptyStateRect"), Reply::value(""));
    assert_eq!(read(cx, handle, "emptyHeroState"), Reply::value("{}"));
}

#[gpui::test]
fn review_u05_backend_death_removes_dead_owners_running_activity(cx: &mut TestAppContext) {
    let handle = open(cx);
    frame(cx, handle);
    handle
        .update(cx, |shell, window, cx| {
            shell.status_messages(cx, |messages, now| {
                messages.set_activity(0, "Copying file", 12, true, false, now);
            });
            shell.receive(
                0,
                crate::backend::Event::Failed(crate::backend::Failure {
                    key: "window.backend_eof",
                    detail: String::new(),
                }),
                window,
                cx,
            );
        })
        .unwrap();
    frame(cx, handle);
    let state = json(cx, handle, "statusActivityState");
    assert_eq!(
        state.get("activities").unwrap().as_array().unwrap().len(),
        0,
        "a dead backend cannot complete the operation: {}",
        flea::jsondoc::render_at(&state, 0)
    );
}
