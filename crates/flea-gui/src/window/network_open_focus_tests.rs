//! Saved-share opens use the real rail keys, service replies and pane-origin checks.
use super::*;

use crate::window::{
    locale::Snapshot,
    startup::{Arguments, Launch, Settings},
};
use std::sync::Arc;

thread_local! {
    static BACKENDS: RefCell<Vec<Lines>> = const { RefCell::new(Vec::new()) };
}

pub(super) fn root(
    recorder: Rc<Recorder>,
    cx: &mut TestAppContext,
) -> (Entity<Shell>, &mut VisualTestContext, Vec<Lines>) {
    BACKENDS.with(|lines| lines.borrow_mut().clear());
    cx.update(|cx| {
        cx.set_global(crate::state::constructor_fixture::Services {
            backend: |_, _| {
                let (backend, lines) = Backend::test_lines();
                BACKENDS.with(|kept| kept.borrow_mut().push(Box::new(lines)));
                Ok(backend)
            },
            monitor: None,
        })
    });
    let state = flea::jsondoc::parse(r#"{"view":"dual","keys":"default","dual":{"paths":["/fixture/work","/fixture/work"],"focus":0},"places":{"rail":"shown","sidebarWidth":192}}"#).unwrap();
    let launch = Launch::resolve(
        Settings::from_json(state),
        Arguments {
            path: Some("/fixture/work".into()),
            select: None,
        },
        "/fixture/home",
    );
    let locale = Snapshot {
        revision: 1,
        catalog: Arc::new(crate::test_catalogs::english()),
    };
    let (_sender, events) = async_channel::unbounded();
    let (root, cx) = cx.add_window_view(|window, cx| crate::window::root::Root {
        shell: cx.new(|cx| {
            let mut shell = Shell::new(launch, locale, events, window, cx);
            shell.rail.as_mut().unwrap().network_runner = recorder;
            for host in shell.panes.iter_mut().flatten() {
                *host.nav.writer.borrow_mut() = crate::navigation::folder_state::Writer::default();
            }
            shell
        }),
    });
    let shell = root.read_with(cx, |root, _| root.shell.clone());
    cx.update(|window, cx| shell.update(cx, |shell, cx| window.focus(&shell.focus, cx)));
    cx.run_until_parked();
    frame(cx);
    let lines = BACKENDS.with(|lines| std::mem::take(&mut *lines.borrow_mut()));
    (shell, cx, lines)
}

const MOUNT: &str = "/fixture/saved-share";

fn saved(mounted: bool) -> RailEntry {
    RailEntry {
        uri: "smb://fixture/share".into(),
        path: if mounted { MOUNT.into() } else { String::new() },
        mounted,
        ..share()
    }
}

fn held(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> bool {
    cx.update(|window, cx| shell.read(cx).dual_focus.rail.contains_focused(window, cx))
}

fn listed(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> bool {
    cx.update(|window, cx| shell.read(cx).focus_view(window, cx) == "list")
}

fn publish(shell: &Entity<Shell>, mounted: bool, cx: &mut VisualTestContext) {
    shell.update(cx, |shell, cx| {
        for host in shell.panes.iter_mut().flatten() {
            host.pane.swap.pending = None;
        }
        shell.network_publish(Some(vec![saved(mounted)]), cx);
    });
    frame(cx);
}

fn enter_share(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    for _ in 0..8 {
        if held(shell, cx) {
            break;
        }
        cx.simulate_keystrokes("tab");
        frame(cx);
    }
    let producer = shell.read_with(cx, |shell, _| shell.rail.as_ref().unwrap().places.clone());
    let index = producer.read_with(cx, |producer, _| {
        producer
            .entries()
            .iter()
            .position(|entry| entry.group == Group::Network)
            .unwrap()
    });
    for _ in 0..16 {
        if producer.read_with(cx, |producer, _| producer.cursor()) == Some(index) {
            break;
        }
        cx.simulate_keystrokes("down");
        frame(cx);
    }
    assert!(held(shell, cx), "actual rail focus before Return");
    assert_eq!(
        producer.read_with(cx, |producer, _| producer.cursor()),
        Some(index)
    );
    cx.simulate_keystrokes("enter");
    frame(cx);
}

fn place(shell: &Entity<Shell>, side: usize, cx: &mut VisualTestContext) -> String {
    shell.read_with(cx, |shell, _| {
        shell.panes[side].as_ref().unwrap().pane.place().into()
    })
}

fn flight(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> (Entity<Service>, u64) {
    shell.read_with(cx, |shell, _| {
        let network = shell.rail.as_ref().unwrap().network.as_ref().unwrap();
        (
            network.service.clone(),
            network.origins.last().unwrap().token,
        )
    })
}

#[gpui::test]
fn railnet_mounted_share_and_same_path_return_hand_keyboard_to_list(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, _lines) = root(recorder.clone(), cx);
    publish(&shell, true, cx);
    enter_share(&shell, cx);
    assert_eq!(place(&shell, 0, cx), MOUNT);
    assert!(listed(&shell, cx));
    assert!(
        recorder.mounts().is_empty(),
        "already mounted path uses the direct leg"
    );
    crate::window::dispatch::test_shell::land(&shell, cx, MOUNT, Vec::new(), 1);
    enter_share(&shell, cx);
    assert_eq!(place(&shell, 0, cx), MOUNT);
    assert!(
        listed(&shell, cx),
        "an accepted same-path open also hands off"
    );
}

#[gpui::test]
fn railnet_mount_and_info_open_hands_keyboard_to_list(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, _lines) = root(recorder.clone(), cx);
    publish(&shell, false, cx);
    enter_share(&shell, cx);
    assert!(held(&shell, cx), "mount has not answered");
    assert_eq!(recorder.mounts().len(), 1);
    answer_mount(&recorder, MOUNT, cx);
    assert_eq!(place(&shell, 0, cx), MOUNT);
    assert!(listed(&shell, cx));
}

#[gpui::test]
fn railnet_stopped_direct_and_service_opens_keep_existing_place_and_rail_focus(
    cx: &mut TestAppContext,
) {
    for mounted in [true, false] {
        let recorder = Rc::new(Recorder::default());
        let (shell, cx, _lines) = root(recorder.clone(), cx);
        publish(&shell, mounted, cx);
        shell.update(cx, |shell, _| {
            shell.panes[0].as_mut().unwrap().stopped = true;
        });
        let before = place(&shell, 0, cx);
        enter_share(&shell, cx);
        assert!(
            flight(&shell, cx).1 > 0,
            "Return created a real open origin"
        );
        if !mounted {
            assert_eq!(recorder.mounts().len(), 1);
            answer_mount(&recorder, MOUNT, cx);
        }
        assert_eq!(place(&shell, 0, cx), before);
        assert!(held(&shell, cx));
    }
}

#[gpui::test]
fn railnet_failed_mount_keeps_place_and_does_not_focus_list(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, _lines) = root(recorder.clone(), cx);
    publish(&shell, false, cx);
    let before = place(&shell, 0, cx);
    enter_share(&shell, cx);
    recorder.answer(*recorder.mounts().last().unwrap(), 1, "");
    cx.run_until_parked();
    recorder.answer(*recorder.find("info").last().unwrap(), 1, "");
    frame(cx);
    assert_eq!(place(&shell, 0, cx), before);
    cx.update(|window, cx| {
        let shell = shell.read(cx);
        assert!(
            !shell.focus.is_focused(window),
            "failure must not hand focus to the list"
        );
        assert!(
            shell
                .rail
                .as_ref()
                .unwrap()
                .network
                .as_ref()
                .unwrap()
                .card
                .read(cx)
                .model
                .opened,
            "the existing retry card retains its failure behavior"
        );
    });
}

#[gpui::test]
fn railnet_rail_key_during_mount_cancels_only_keyboard_handoff(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, _lines) = root(recorder.clone(), cx);
    publish(&shell, false, cx);
    enter_share(&shell, cx);
    cx.simulate_keystrokes("up");
    frame(cx);
    answer_mount(&recorder, MOUNT, cx);
    assert_eq!(place(&shell, 0, cx), MOUNT, "the mount still opens");
    assert!(held(&shell, cx), "user continued walking the rail");
}

#[gpui::test]
fn railnet_replaced_origin_drops_reply_without_navigation_or_focus(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, _lines) = root(recorder.clone(), cx);
    publish(&shell, false, cx);
    enter_share(&shell, cx);
    let (fresh, fresh_lines) = Backend::test_lines();
    let mut fresh_lines: Lines = Box::new(fresh_lines);
    shell.update(cx, |shell, cx| {
        shell.panes[0] = shell_with(fresh, cx).panes[0].take()
    });
    drain(&mut fresh_lines);
    let before = place(&shell, 0, cx);
    answer_mount(&recorder, MOUNT, cx);
    assert_eq!(place(&shell, 0, cx), before);
    assert!(!drain(&mut fresh_lines)
        .iter()
        .any(|line| line.contains(MOUNT)));
    assert!(held(&shell, cx));
}

#[gpui::test]
fn railnet_cancelled_old_child_cannot_consume_new_request_handoff(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, _lines) = root(recorder.clone(), cx);
    publish(&shell, false, cx);
    enter_share(&shell, cx);
    let old = *recorder.mounts().last().unwrap();
    let (service, origin) = flight(&shell, cx);
    service.update(cx, |service, cx| service.cancel_origin(origin, cx));
    frame(cx);
    assert!(held(&shell, cx));
    enter_share(&shell, cx);
    let (_, current) = flight(&shell, cx);
    assert_ne!(origin, current);
    assert_eq!(recorder.mounts().len(), 2);
    let _ = recorder.answers.borrow()[old].try_send(Proc::Exited(Outcome {
        code: 0,
        stdout: String::new(),
        stderr: String::new(),
    }));
    frame(cx);
    assert!(
        recorder.find("info").is_empty(),
        "old generation never advances current request"
    );
    assert!(held(&shell, cx));
    answer_mount(&recorder, MOUNT, cx);
    assert_eq!(place(&shell, 0, cx), MOUNT);
    assert!(listed(&shell, cx), "only current request takes its handoff");
}

#[gpui::test]
fn railnet_reply_opens_origin_side_without_stealing_other_side_rail(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, _lines) = root(recorder.clone(), cx);
    publish(&shell, false, cx);
    enter_share(&shell, cx);
    let before = place(&shell, 1, cx);
    shell.update(cx, |shell, _| shell.focus_side = 1);
    answer_mount(&recorder, MOUNT, cx);
    assert_eq!(place(&shell, 0, cx), MOUNT);
    assert_eq!(place(&shell, 1, cx), before);
    assert!(held(&shell, cx));
}

#[gpui::test]
fn railnet_older_bridge_open_cannot_take_new_origin_claim(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, _lines) = root(recorder.clone(), cx);
    publish(&shell, false, cx);
    enter_share(&shell, cx);
    let (service, first) = flight(&shell, cx);
    let bridge = service.read_with(cx, |service, _| {
        format!(
            "{}/smb-share:server=fixture,share=share",
            flea::ui_logic_pl2::bridge::bridge_dir(&service.runtime_dir)
        )
    });
    answer_mount(&recorder, &bridge, cx);
    let check = *recorder
        .find("-d")
        .last()
        .expect("real service checked its GVFS bridge");
    // The mount's Info exit re-lists from the real bookmarks, which do not hold this fixture row, so the rail gets it back.
    shell.update(cx, |shell, cx| {
        shell.network_publish(Some(vec![saved(false)]), cx)
    });
    frame(cx);
    enter_share(&shell, cx);
    let (_, second) = flight(&shell, cx);
    assert_ne!(first, second);
    assert_eq!(recorder.mounts().len(), 2);
    recorder.answer(check, 0, "");
    frame(cx);
    assert_eq!(
        place(&shell, 0, cx),
        bridge,
        "older detached bridge still opens in its original pane"
    );
    assert!(
        held(&shell, cx),
        "older origin cannot consume the new request's keyboard claim"
    );
    crate::window::dispatch::test_shell::land(&shell, cx, &bridge, Vec::new(), 1);
    answer_mount(&recorder, MOUNT, cx);
    assert_eq!(place(&shell, 0, cx), MOUNT);
    assert!(
        listed(&shell, cx),
        "new origin retains and lands its own claim"
    );
}

#[gpui::test]
fn railnet_a_bridge_starting_note_keeps_the_rail_open_claim(cx: &mut TestAppContext) {
    use crate::g2::pl2::mount::Effect;
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, _lines) = root(recorder.clone(), cx);
    publish(&shell, false, cx);
    enter_share(&shell, cx);
    let (_, origin) = flight(&shell, cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let note = flea::ui_logic_pl1::Note::new("network.bridge_starting", &["share"]);
            shell.network_service(&Effect::Say { note, origin }, window, cx);
        })
    });
    answer_mount(&recorder, MOUNT, cx);
    assert_eq!(place(&shell, 0, cx), MOUNT);
    assert!(
        listed(&shell, cx),
        "an opening note is not the end of the open"
    );
}
