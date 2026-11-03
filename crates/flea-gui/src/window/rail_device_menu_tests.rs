//! Device menus reached through rail pointer input and the shared menu's keyboard and pointer handlers.
use super::*;
use gpui::{point, px, Modifiers, MouseButton};

fn menu(shell: &Entity<Shell>, label: &str, cx: &mut VisualTestContext) {
    let position = producer(shell, cx).read_with(cx, |producer, _| {
        let laid = producer.laid.borrow();
        let row = laid.rows.iter().find(|row| row.label == label).unwrap();
        point(
            px(row.label_rect.x + 4.),
            px(row.rect.y + row.rect.height / 2.),
        )
    });
    cx.simulate_mouse_down(position, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(position, MouseButton::Right, Modifiers::none());
    frame(cx);
    assert!(
        shell.read_with(cx, |shell, _| shell.menu.menu.borrow().opened()),
        "the right click opens a mounted menu"
    );
}

fn choose(shell: &Entity<Shell>, action: &str, cx: &mut VisualTestContext) {
    // The cursor starts on the first row, as raildrive's keyboard test walks it: down once per row, then enter.
    let index = shell.read_with(cx, |shell, _| {
        let menu = shell.menu.menu.borrow();
        let card = menu.card.as_ref().unwrap();
        card.entries
            .iter()
            .filter(|row| !row.separator)
            .position(|row| row.action == action)
            .unwrap()
    });
    cx.simulate_keystrokes(&format!("{}enter", "down ".repeat(index)));
    // The chosen action lands after the clock moves: its jobs and the menu's own timers settle here.
    for _ in 0..3 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(200));
        frame(cx);
    }
}

#[gpui::test]
fn pb_devices_menu_open_reaches_the_mounted_volumes_folder(cx: &mut TestAppContext) {
    let (shell, service, cx) = open("{}", cx);
    ready(&service, cx);
    menu(&shell, "Archive", cx);
    choose(&shell, "open", cx);
    // The requested place, as raildrive's Return test reads it: the listing answer is not part of this unit.
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .pane
            .place()
            .to_owned()),
        "/media/archive"
    );
}

#[gpui::test]
fn pb_devices_menu_unlock_reaches_the_locked_rows_card(cx: &mut TestAppContext) {
    let (shell, service, cx) = open("{}", cx);
    service.answer(
        "lsblk",
        0,
        &BLOCKS.replace(
            "\"fstype\":\"ext4\",\"size\":2000000",
            "\"fstype\":\"crypto_LUKS\",\"size\":2000000",
        ),
    );
    service.answer("gio", 0, PHONE);
    frame(cx);
    menu(&shell, "Backup", cx);
    choose(&shell, "unlockVolume", cx);
    shell.read_with(cx, |shell, cx| {
        let unlock = shell.rail.as_ref().unwrap().unlock.as_ref().unwrap();
        let card = unlock.view.read(cx).host.card.as_ref().unwrap();
        assert!(card.opened);
        assert_eq!(card.device, "/dev/sdb1");
    });
}

#[gpui::test]
fn pb_devices_menu_flash_image_opens_the_drive_picker_without_writing(cx: &mut TestAppContext) {
    let (shell, service, cx) = open("{}", cx);
    ready(&service, cx);
    menu(&shell, "Archive", cx);
    choose(&shell, "flashImage", cx);
    assert!(shell.read_with(cx, |shell, _| shell.operations.archive.flash.shown()));
    assert!(!shell.read_with(cx, |shell, _| shell.operations.archive.flash.running()));
    assert!(service
        .jobs
        .borrow()
        .iter()
        .all(|(argv, _, _)| argv[0] != "dd"));
}

#[gpui::test]
fn pb_devices_menu_removed_row_never_opens_a_replacement(cx: &mut TestAppContext) {
    let (shell, service, cx) = open("{}", cx);
    ready(&service, cx);
    menu(&shell, "Archive", cx);
    let before = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().pane.place().to_owned()
    });
    source(&shell, cx).update(cx, |source, cx| source.poll_now(cx));
    service.answer(
        "lsblk",
        1,
        &BLOCKS
            .replace("/dev/sda1", "/dev/sdc1")
            .replace("Archive", "Replacement"),
    );
    frame(cx);
    choose(&shell, "open", cx);
    // place() moves on the request, before any listing answers, so a stale open shows here.
    let after = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().pane.place().to_owned()
    });
    assert_eq!(after, before);
    assert_ne!(after, "/media/archive");
}

#[gpui::test]
fn pb_devices_menu_unmount_uses_unmount_instead_of_eject(cx: &mut TestAppContext) {
    let (shell, service, cx) = open("{}", cx);
    ready(&service, cx);
    menu(&shell, "Archive", cx);
    choose(&shell, "unmountVolume", cx);
    let jobs: Vec<_> = service
        .jobs
        .borrow()
        .iter()
        .map(|(argv, _, _)| argv.join(" "))
        .collect();
    assert!(
        jobs.iter()
            .any(|argv| argv == "gio mount -u /media/archive"),
        "{jobs:?}"
    );
    assert!(
        !jobs.iter().any(|argv| argv.contains("mount -e")),
        "Unmount must not eject: {jobs:?}"
    );
}
