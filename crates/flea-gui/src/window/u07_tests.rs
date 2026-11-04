//! The live shell's chrome and path seam, driven through the same pointer and keyboard path as the native runner.
use crate::{backend::Backend, ipc::server::Event, state::Shell};
use flea::ipc::wire::{Reply, Request};
use gpui::{
    point, px, size, Entity, Modifiers, MouseButton, MouseDownEvent, TestAppContext,
    VisualTestContext,
};

pub(super) fn open(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext) {
    let (backend, _requests) = Backend::test_channel();
    open_backend(backend, cx)
}

fn open_backend(
    backend: Backend,
    cx: &mut TestAppContext,
) -> (Entity<Shell>, &mut VisualTestContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| crate::state::tests::shell_with(backend, cx));
    cx.simulate_resize(size(px(1100.), px(700.)));
    cx.update(|window, cx| shell.update(cx, |shell, cx| window.focus(&shell.focus, cx)));
    (shell, cx)
}

pub(super) fn reply(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    name: &str,
    args: &[&str],
) -> Reply {
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
            receive.try_recv().expect("the real shell answered").reply
        })
    })
}

pub(super) fn read(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    name: &str,
    args: &[&str],
) -> String {
    cx.run_until_parked();
    let reply = reply(shell, cx, name, args);
    assert_eq!(
        reply.code, 0,
        "the mounted shell must answer {name}: {reply:?}"
    );
    reply.stdout.trim_end_matches('\n').into()
}

pub(super) fn press(cx: &mut VisualTestContext, centre: &str, taps: usize) {
    let mut values = centre
        .split_whitespace()
        .map(|value| value.parse::<f32>().unwrap());
    let position = point(
        px(values.next().expect("drawn x")),
        px(values.next().expect("drawn y")),
    );
    cx.simulate_event(MouseDownEvent {
        position,
        button: MouseButton::Left,
        click_count: taps,
        modifiers: Modifiers::none(),
        first_mouse: false,
    });
    cx.simulate_mouse_up(position, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn u07_live_buttons_report_drawn_state_and_pointer_changes_the_view(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    assert_eq!(read(&shell, cx, "chromeHeight", &[]), "27");
    let back =
        flea::jsondoc::parse(&read(&shell, cx, "chromeButtonState", &["arrow-left"])).unwrap();
    assert_eq!(
        back.get("enabled").and_then(flea::jsondoc::Json::as_bool),
        Some(false)
    );
    let back = read(&shell, cx, "chromeButtonCentre", &["arrow-left"]);
    press(cx, &back, 1);
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .nav
            .counters
            .lists),
        0,
        "a disabled Back never submits navigation"
    );
    let before = read(&shell, cx, "chromeButtonState", &["search"]);
    assert!(flea::jsondoc::parse(&before)
        .unwrap()
        .get("visible")
        .unwrap()
        .as_bool()
        .unwrap());
    let centre = read(&shell, cx, "chromeButtonCentre", &["grid"]);
    press(cx, &centre, 1);
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.view),
        super::startup::View::Grid
    );
    let after = flea::jsondoc::parse(&read(&shell, cx, "chromeButtonState", &["search"])).unwrap();
    assert_eq!(
        after.get("visible").and_then(flea::jsondoc::Json::as_bool),
        Some(false)
    );
    shell.update(cx, |shell, cx| {
        let (backend, _requests) = Backend::test_channel();
        let mut right =
            super::pane::PaneHost::new("/right/child".into(), None, 620., &shell.locale);
        right.backend = Some(backend);
        shell.panes[1] = Some(right);
        shell.view = super::startup::View::Dual;
        shell.path_labels[1] = "/right/child".into();
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(read(&shell, cx, "paneCrumbText", &["0"]), "/files");
    assert_eq!(read(&shell, cx, "paneCrumbText", &["1"]), "/right/child");
    assert_eq!(read(&shell, cx, "crumbCentre", &["0"]), "");
    assert_eq!(read(&shell, cx, "paneCrumbText", &["2"]), "");
    assert_eq!(read(&shell, cx, "chromeButtonCentre", &["unknown"]), "");
    assert_eq!(
        read(&shell, cx, "settingsOpen", &[]),
        "false",
        "the existing U11 reader remains mounted"
    );
}

#[gpui::test]
fn u07_pointer_and_keyboard_edit_the_mounted_path_and_escape_cancels(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = open_backend(backend, cx);
    assert_eq!(read(&shell, cx, "crumbText", &[]), "/files");
    assert_eq!(read(&shell, cx, "pathCentre", &[]), "474 13");
    assert_eq!(read(&shell, cx, "pathFrameRect", &[]), "");
    let leaf = read(&shell, cx, "crumbCentre", &["1"]);
    press(cx, &leaf, 1);
    assert_eq!(read(&shell, cx, "pathBarOpen", &[]), "false");
    press(cx, &leaf, 2);
    assert_eq!(read(&shell, cx, "pathBarOpen", &[]), "true");
    assert_eq!(read(&shell, cx, "pathBarText", &[]), "/files/");
    let frame = read(&shell, cx, "pathFrameRect", &[]);
    assert_eq!(
        frame, "80 2 788 23",
        "the real field matches the 1100 px board"
    );
    assert_eq!(
        frame
            .split_whitespace()
            .skip(1)
            .step_by(2)
            .collect::<Vec<_>>(),
        ["2", "23"]
    );
    assert_eq!(read(&shell, cx, "crumbCentre", &["1"]), "");
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("file://otherbox/etc");
    assert_eq!(read(&shell, cx, "pathBarText", &[]), "file://otherbox/etc");
    cx.simulate_keystrokes("enter");
    assert_eq!(read(&shell, cx, "pathBarOpen", &[]), "false");
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .pane
            .path()
            .to_owned()),
        "/files"
    );
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .status
            .to_string()),
        crate::test_catalogs::english().tr("path.refused_uri")
    );
    let sentence = "That URI names a file on another host, not a path on this machine.";
    assert_eq!(read(&shell, cx, "statusPrimary", &[]), sentence);
    assert_eq!(read(&shell, cx, "statusError", &[]), "false");
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
    shell.read_with(cx, |shell, cx| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.nav.counters.lists, 0, "a refused URI never navigates");
        assert!(host.pane.swap.pending.is_none());
        let footer = shell.surfaces[0].footer.as_ref().unwrap().read(cx);
        let line = footer.message_line();
        let painted = line
            .read(cx)
            .painted
            .get("primary")
            .expect("painted refusal");
        assert_eq!(painted.text.as_ref(), sentence);
    });
    cx.simulate_keystrokes("ctrl-l");
    assert_eq!(read(&shell, cx, "pathBarOpen", &[]), "true");
    cx.simulate_keystrokes("escape");
    assert_eq!(read(&shell, cx, "pathBarOpen", &[]), "false");
}

#[gpui::test]
fn u07_local_file_uri_authorities_still_submit_navigation(cx: &mut TestAppContext) {
    for uri in [
        "file:///etc",
        "file://localhost/etc",
        "file://LOCALHOST/etc",
    ] {
        let (backend, _requests) = Backend::test_channel();
        let (shell, cx) = open_backend(backend, cx);
        cx.simulate_keystrokes("ctrl-l ctrl-a");
        cx.simulate_input(uri);
        cx.simulate_keystrokes("enter");
        assert_eq!(read(&shell, cx, "pathBarOpen", &[]), "false", "{uri}");
        assert_eq!(read(&shell, cx, "statusPrimary", &[]), "", "{uri}");
        shell.read_with(cx, |shell, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.pane.place(), "/etc", "{uri}");
            assert_eq!(host.nav.counters.lists, 1, "{uri}");
        });
    }
}

#[gpui::test]
fn u07_elision_is_inert_and_a_parent_press_sends_the_real_navigation(cx: &mut TestAppContext) {
    let (backend, mut requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| crate::state::tests::shell_with(backend, cx));
    cx.simulate_resize(size(px(1100.), px(700.)));
    let deep = format!(
        "/{}",
        ["a-directory-with-a-deliberately-long-name"; 12].join("/")
    );
    shell.update(cx, |shell, cx| {
        let host = shell.panes[0].as_mut().unwrap();
        host.pane.begin(
            deep.clone(),
            crate::pane::swap::Options::default(),
            host.pane.now(),
        );
        host.pane
            .receive(flea::protocol::Reply::Listed {
                path: deep.clone(),
                total: 1,
            })
            .unwrap();
        host.pane
            .receive(flea::protocol::Reply::Rows {
                start: 0,
                rows: vec![Default::default()],
                listing: 3,
            })
            .unwrap();
        host.changed(&shell.locale);
        shell.path_labels[0] = deep.clone().into();
        cx.notify();
    });
    cx.run_until_parked();
    let marker = read(&shell, cx, "elisionCentre", &[]);
    assert!(
        !marker.is_empty(),
        "a deep path must draw its collapsed marker"
    );
    press(cx, &marker, 1);
    assert_eq!(read(&shell, cx, "pathBarOpen", &[]), "false");
    press(cx, &marker, 2);
    assert_eq!(read(&shell, cx, "pathBarOpen", &[]), "true");
    cx.simulate_keystrokes("escape");
    while requests().is_some() {}
    let before = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().nav.counters.lists
    });
    let count: usize = read(&shell, cx, "crumbCount", &[]).parse().unwrap();
    let index = (count - 2).to_string();
    let parent = read(&shell, cx, "crumbCentre", &[&index]);
    press(cx, &parent, 1);
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .nav
            .counters
            .lists),
        before + 1,
        "a parent press queues exactly one backend listing"
    );
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .pane
            .swap
            .pending
            .as_ref()
            .map(|pending| pending.path.clone())),
        Some(deep.rsplit_once('/').unwrap().0.to_owned())
    );
    assert_eq!(read(&shell, cx, "pathBarOpen", &[]), "false");
    assert_eq!(read(&shell, cx, "crumbCentre", &["999"]), "");
}
