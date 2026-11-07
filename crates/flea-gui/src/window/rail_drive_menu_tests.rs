//! The device menu by every opening key, and its Mount and Unmount rows on a stick and a phone, by real keys.
use super::*;

const SCRATCH: &str = r#"{"blockdevices":[
{"name":"nvme0n1","path":"/dev/nvme0n1","type":"disk","children":[{"name":"nvme0n1p1","type":"part","mountpoints":["/"],"fstype":"ext4"}]},
{"name":"sda1","path":"/dev/sda1","type":"part","label":"128GB","mountpoints":["/run/media/gm/128GB"],"fstype":"exfat","rm":true,"size":128000000000},
{"name":"sdc1","path":"/dev/sdc1","type":"part","label":"Scratch","mountpoints":[null],"fstype":"exfat","rm":true,"size":64000000000}
]}"#;
const PHONE: &str = "Volume(0): Pixel 8\n  Type: GProxyVolume (GProxyVolumeMonitorMTP)\n  activation_root=mtp://pixel/\n  can_mount=1\n  Mount(0): Pixel 8 -> mtp://pixel/\n";
const UNMOUNTED_PHONE: &str = "Volume(0): Pixel 8\n  Type: GProxyVolume (GProxyVolumeMonitorMTP)\n  activation_root=mtp://pixel/\n  can_mount=1\n";
const PLACES: &str = r#"{"rail":"shown","sidebarWidth":192}"#;

// Runs the menu row under the cursor after `keys`, lets its job start, and proves the menu took the Enter.
fn choose(shell: &Entity<Shell>, keys: &str, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes(keys);
    for _ in 0..3 {
        frame(cx);
    }
    assert!(
        menu_rows(shell, cx).is_empty(),
        "the chosen row closes the menu"
    );
}

// Opens the device menu by `keys` and asserts its rows were painted in a native frame.
fn open_painted(shell: &Entity<Shell>, keys: &str, cx: &mut VisualTestContext) -> Vec<String> {
    crate::g2::mn1::element::PAINTED.with(|painted| painted.borrow_mut().clear());
    cx.simulate_keystrokes(keys);
    native_frames(cx);
    assert!(
        crate::g2::mn1::element::PAINTED.with(|painted| !painted.borrow().is_empty()),
        "{keys} paints the device menu"
    );
    menu_rows(shell, cx)
}

#[gpui::test]
fn raildrive_menu_named_key_on_a_mounted_stick_paints_the_device_menu(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    let (shell, _service, cx) = root_at(cx);
    walk_to_stick(&shell, cx);
    assert_eq!(
        open_painted(&shell, "menu", cx),
        ["open", "unmountVolume", "eject", "flashImage"],
        "the Menu key opens the device menu"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn raildrive_shift_f10_on_a_mounted_stick_paints_the_device_menu(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    let (shell, _service, cx) = root_at(cx);
    walk_to_stick(&shell, cx);
    assert_eq!(
        open_painted(&shell, "shift-f10", cx),
        ["open", "unmountVolume", "eject", "flashImage"],
        "Shift+F10 opens the device menu"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn raildrive_flash_image_takes_the_keys_and_up_reaches_the_image_row(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    let (shell, _service, cx) = root_at(cx);
    walk_to_stick(&shell, cx);
    let rows = open_painted(&shell, "menu", cx);
    let at = rows.iter().position(|id| id == "flashImage").unwrap();
    choose(&shell, &format!("{}enter", "down ".repeat(at)), cx);
    assert!(shell.read_with(cx, |shell, _| shell.operations.archive.flash.shown()));
    assert!(
        cx.update(|window, cx| shell.read(cx).flash_has_keys(window, cx)),
        "Flash image from the rail gives the flash pane the keys"
    );
    cx.simulate_keystrokes("up");
    frame(cx);
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.operations.archive.flash.park()),
        crate::g2::op2::flash::Seat::Image,
        "Up from the drive reaches Choose an image… before an image is chosen"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn raildrive_device_menu_unmount_runs_on_the_stick(cx: &mut TestAppContext) {
    let (shell, service, cx) = root_at(cx);
    walk_to_stick(&shell, cx);
    cx.simulate_keystrokes("m");
    native_frames(cx);
    choose(&shell, "down enter", cx);
    let jobs = argv_lines(&service);
    assert!(
        jobs.iter()
            .any(|job| job == "gio mount -u /run/media/gm/128GB"),
        "Unmount releases the stick's mount: {jobs:?}"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn raildrive_device_menu_mount_runs_on_an_unmounted_stick(cx: &mut TestAppContext) {
    let (shell, service, cx) = root_with(cx, PLACES, SCRATCH);
    walk_to(&shell, "Scratch", cx);
    cx.simulate_keystrokes("m");
    native_frames(cx);
    assert_eq!(
        menu_rows(&shell, cx),
        ["mountVolume", "flashImage"],
        "an unmounted stick offers Mount"
    );
    choose(&shell, "enter", cx);
    let jobs = argv_lines(&service);
    assert!(
        jobs.iter().any(|job| job == "gio mount -d /dev/sdc1"),
        "Mount runs gio mount on the stick: {jobs:?}"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn raildrive_device_menu_unmount_runs_on_a_mounted_phone(cx: &mut TestAppContext) {
    let (shell, service, cx) = root_devices(cx, PLACES, BLOCKS, PHONE);
    walk_to(&shell, "Pixel 8", cx);
    cx.simulate_keystrokes("m");
    native_frames(cx);
    assert_eq!(
        menu_rows(&shell, cx),
        ["open", "unmountPhone"],
        "a mounted phone offers Unmount"
    );
    choose(&shell, "down enter", cx);
    let jobs = argv_lines(&service);
    assert!(
        jobs.iter().any(|job| job == "gio mount -u mtp://pixel/"),
        "Unmount releases the phone's GVFS mount: {jobs:?}"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn raildrive_device_menu_mount_opens_an_unmounted_phone_through_gvfs(cx: &mut TestAppContext) {
    let (shell, _service, cx) = root_devices(cx, PLACES, BLOCKS, UNMOUNTED_PHONE);
    // The phone's GVFS flight runs on the network owner's own runner, as in the phone source test.
    let network = Rc::new(FakeDevices::default());
    shell.update(cx, |shell, _| {
        let join = shell.rail.as_mut().unwrap();
        join.network = None;
        join.network_runner = network.clone();
    });
    walk_to(&shell, "Pixel 8", cx);
    cx.simulate_keystrokes("m");
    native_frames(cx);
    assert_eq!(
        menu_rows(&shell, cx),
        ["mountPhone"],
        "an unmounted phone offers Mount"
    );
    let mount = "gio mount -- mtp://pixel/";
    let before = argv_lines(&network);
    assert!(
        !before.iter().any(|job| job == mount),
        "no mount runs before the choice: {before:?}"
    );
    choose(&shell, "enter", cx);
    let jobs = argv_lines(&network);
    assert!(
        jobs.iter().any(|job| job == mount),
        "Mount opens the phone through GVFS: {jobs:?}"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
