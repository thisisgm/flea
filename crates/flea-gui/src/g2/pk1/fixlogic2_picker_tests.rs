//! The picker's phone mount re-lists its rail once, at the info exit, as v0.3.12 NetworkMounts.qml infoProcess.onExited does.
use super::*;

fn relists(runner: &Recorder) -> usize {
    runner
        .0
        .borrow()
        .iter()
        .filter(|(argv, _)| argv.iter().any(|arg| arg == "-li"))
        .count()
}

/// Chooses the unmounted phone, lets gio mount succeed, then answers gio info; returns the location and the re-lists the mount started.
fn info_exit(code: i32, stdout: &str, cx: &mut TestAppContext) -> (String, usize, usize) {
    let (mut f, runner) = opening(cx);
    let before = relists(&runner);
    f.update(|picker, cx| {
        picker.rail.entries.push(remote(true));
        picker.rail_choose(picker.rail.entries.len() - 1, cx);
    });
    f.cx.run_until_parked();
    runner.answer(&["gio", "mount"], 0, "");
    f.cx.run_until_parked();
    let at_mount_exit = relists(&runner) - before;
    runner.answer(&["gio", "info"], code, stdout);
    f.cx.run_until_parked();
    let location = f.read(|picker| picker.listing.location.clone());
    (location, at_mount_exit, relists(&runner) - before)
}

#[gpui::test]
fn fixlogic2_picker_phone_mount_relists_once_at_its_info_exit(cx: &mut TestAppContext) {
    let (location, at_mount_exit, total) = info_exit(0, "local path: /fixture/phone\n", cx);
    assert_eq!(
        location, "/fixture/phone",
        "the info exit opened the phone's folder"
    );
    assert_eq!(at_mount_exit, 0, "the mount exit re-lists nothing");
    assert_eq!(
        total, 1,
        "the info exit re-lists the picker's rail once, and the open after it adds none"
    );
}

#[gpui::test]
fn fixlogic2_picker_failed_info_exit_still_relists(cx: &mut TestAppContext) {
    let (location, at_mount_exit, total) = info_exit(1, "", cx);
    assert_eq!(location, "/d", "an info exit with no folder opens nothing");
    assert_eq!(at_mount_exit, 0, "the mount exit re-lists nothing");
    assert_eq!(total, 1, "the failed info exit still re-lists once");
}
