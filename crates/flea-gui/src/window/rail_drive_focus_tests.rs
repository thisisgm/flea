//! EY-423: a keyboard Return on a device row hands the keyboard to the list once the open lands, as RailKeys.landed.
use super::*;

const UNMOUNTED: &str = r#"{"blockdevices":[
{"name":"nvme0n1","path":"/dev/nvme0n1","type":"disk","children":[{"name":"nvme0n1p1","type":"part","mountpoints":["/"],"fstype":"ext4"}]},
{"name":"sda1","path":"/dev/sda1","type":"part","label":"128GB","mountpoints":[null],"fstype":"exfat","rm":true,"size":128000000000}
]}"#;

fn rail_holds_keys(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> bool {
    cx.update(|window, cx| {
        let shell = shell.read(cx);
        shell.dual_focus.rail.contains_focused(window, cx)
            || shell.keyboard.context == flea::keys::Context::Rail
    })
}

// Ends the stick's gio mount -d with the given exit.
fn end_mount(service: &FakeDevices, code: i32, stderr: &str) {
    let jobs = service.jobs.borrow();
    let (_, sender, _) = jobs
        .iter()
        .find(|(argv, _, _)| argv.join(" ") == "gio mount -d /dev/sda1")
        .expect("Return mounts the stick");
    sender
        .try_send(ChildEvent::Exited(Outcome {
            code,
            stdout: String::new(),
            stderr: stderr.into(),
        }))
        .unwrap();
}

#[gpui::test]
fn raildrive_return_on_a_mounted_stick_hands_the_keyboard_to_the_list(cx: &mut TestAppContext) {
    let (shell, _service, cx) = root_at(cx);
    walk_to_stick(&shell, cx);
    assert!(
        rail_holds_keys(&shell, cx),
        "Tab put the keyboard in the rail"
    );
    cx.simulate_keystrokes("enter");
    native_frames(cx);
    let asked = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().pane.place().to_owned()
    });
    assert_eq!(asked, "/run/media/gm/128GB");
    assert!(
        !rail_holds_keys(&shell, cx),
        "the open landed, so the list holds the keyboard"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn raildrive_return_to_the_already_displayed_mount_hands_the_keyboard_to_the_list(
    cx: &mut TestAppContext,
) {
    let (shell, _service, cx) = root_at(cx);
    shell.update(cx, |shell, _| {
        let pane = &mut shell.panes[0].as_mut().unwrap().pane;
        *pane = crate::pane::Pane::new("/run/media/gm/128GB".into(), 600.);
        pane.swap.pending = None;
        pane.state.kind = crate::pane::Kind::Ready;
    });
    walk_to_stick(&shell, cx);
    cx.simulate_keystrokes("enter");
    native_frames(cx);
    assert!(
        !rail_holds_keys(&shell, cx),
        "an already displayed mount still accepts the open"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn raildrive_return_mounts_the_stick_then_hands_the_keyboard_to_the_list(cx: &mut TestAppContext) {
    let (shell, service, cx) = root_with(cx, r#"{"rail":"shown","sidebarWidth":192}"#, UNMOUNTED);
    walk_to_stick(&shell, cx);
    cx.simulate_keystrokes("enter");
    native_frames(cx);
    assert!(rail_holds_keys(&shell, cx), "the mount is still running");
    end_mount(&service, 0, "");
    native_frames(cx);
    service.answer("lsblk", 1, BLOCKS);
    native_frames(cx);
    let asked = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().pane.place().to_owned()
    });
    assert_eq!(asked, "/run/media/gm/128GB", "the mounted stick opens");
    assert!(
        !rail_holds_keys(&shell, cx),
        "the mount's open landed, so the list holds the keyboard"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn raildrive_a_failed_mount_leaves_the_keyboard_in_the_rail(cx: &mut TestAppContext) {
    let (shell, service, cx) = root_with(cx, r#"{"rail":"shown","sidebarWidth":192}"#, UNMOUNTED);
    walk_to_stick(&shell, cx);
    cx.simulate_keystrokes("enter");
    native_frames(cx);
    end_mount(&service, 2, "gio: /dev/sda1: Operation was cancelled\n");
    native_frames(cx);
    let asked = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().pane.place().to_owned()
    });
    assert_eq!(asked, "/fixture/work", "nothing opened");
    shell.read_with(cx, |shell, cx| {
        let footer = shell.surfaces[0].footer.as_ref().unwrap().read(cx);
        assert_eq!(
            footer.line.read(cx).drawn().detail.as_ref(),
            "/dev/sda1: Operation was cancelled"
        );
    });
    assert!(
        rail_holds_keys(&shell, cx),
        "RailKeys.messaged: a failed mount has no open to land"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn raildrive_a_rail_key_during_the_mount_ends_the_claim(cx: &mut TestAppContext) {
    let (shell, service, cx) = root_with(cx, r#"{"rail":"shown","sidebarWidth":192}"#, UNMOUNTED);
    walk_to_stick(&shell, cx);
    cx.simulate_keystrokes("enter");
    native_frames(cx);
    cx.simulate_keystrokes("up");
    native_frames(cx);
    end_mount(&service, 0, "");
    native_frames(cx);
    service.answer("lsblk", 1, BLOCKS);
    native_frames(cx);
    let asked = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().pane.place().to_owned()
    });
    assert_eq!(asked, "/run/media/gm/128GB", "the mount still opens");
    assert!(
        rail_holds_keys(&shell, cx),
        "RailKeys.act: the user kept walking the rail, so the keyboard stays there"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
