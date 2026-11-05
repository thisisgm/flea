//! The flash view through the production window: the origin backend, the chooser, the handed run and its ending.
use super::*;

#[test]
fn m9_integ_flash_runs_on_its_origin_across_hide_and_reopen() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, mut requests, mut other_requests) = start(&mut cx);

    // Flash to drive opens the view on the origin backend and asks the image and drives.
    choose(&mut cx, handle, 44, "flashDrive", "", &["/files/disk.img"]);
    assert_eq!(
        drain(&mut requests),
        [
            r#"{"c":"menuaction","op":"close","id":44}"#,
            r#"{"c":"flashimage","path":"/files/disk.img"}"#,
            r#"{"c":"flashtargets","image":"/files/disk.img"}"#,
        ]
    );
    assert!(
        VisualTestContext::from_window(handle.into(), &cx)
            .debug_bounds("op2-flash")
            .is_some(),
        "the flash view is drawn"
    );
    // The image row's chooser answers through the host's chooser and the view asks the chosen image.
    edit(&mut cx, handle, |shell, window, cx| {
        let effects = shell.operations.archive.flash.choose_image();
        assert_eq!(effects, [super::super::Effect::PickImage]);
        shell.archive_effects(0, effects, window, cx);
    });
    cx.run_until_parked();
    frame(&mut cx, handle);
    assert_eq!(
        drain(&mut requests),
        [
            r#"{"c":"flashimage","path":"/files/chosen.iso"}"#,
            r#"{"c":"flashtargets","image":"/files/chosen.iso"}"#
        ]
    );
    receive(
        &mut cx,
        handle,
        0,
        r#"{"t":"flashimage","path":"/files/chosen.iso","kind":"iso","bytes":100,"unpacked":100,"known":true,"member":""}"#,
    );
    receive(
        &mut cx,
        handle,
        0,
        r#"{"t":"flashtargets","drives":[{"dev":"sdz","seq":3,"serial":"S1","label":"STICK","model":"Fixture","bytes":4096,"volumes":[],"refused":""}]}"#,
    );
    for _ in 0..4 {
        if edit(&mut cx, handle, |shell, _, _| {
            shell.operations.archive.flash.park()
        }) == super::super::flash::Seat::Flash
        {
            break;
        }
        keys(&mut cx, handle, "down");
    }
    keys(&mut cx, handle, "enter");
    let pressed = drain(&mut requests);
    assert_eq!(pressed.len(), 1, "{pressed:?}");
    assert!(
        pressed[0]
            .starts_with(r#"{"c":"flash","id":1,"image":"/files/chosen.iso","dev":"sdz","seq":3"#),
        "{}",
        pressed[0]
    );
    receive(
        &mut cx,
        handle,
        0,
        r#"{"t":"flashstarted","id":31,"total":100,"known":true}"#,
    );
    assert!(edit(&mut cx, handle, |shell, _, _| shell
        .operations
        .archive
        .flash
        .running()));

    // Hiding the running flash hands it to the status line; a sample while hidden updates it; reopening takes it back.
    let shown = footer(&mut cx, handle);
    keys(&mut cx, handle, "escape");
    let handed = footer(&mut cx, handle);
    assert_ne!(handed, shown, "the handed run is on the status line");
    edit(&mut cx, handle, |shell, _, _| {
        assert!(!shell.operations.archive.flash.shown());
        assert_eq!(
            shell.operations.archive.handed,
            Some(31),
            "the hidden run is handed over"
        );
    });
    receive(
        &mut cx,
        handle,
        0,
        r#"{"t":"flashprogress","id":31,"phase":"write","bytes":50,"total":100,"permille":500}"#,
    );
    assert_eq!(
        edit(&mut cx, handle, |shell, _, _| shell
            .operations
            .archive
            .handed),
        Some(31)
    );
    choose(&mut cx, handle, 45, "flashDrive", "", &["/files/disk.img"]);
    edit(&mut cx, handle, |shell, _, _| {
        assert!(
            shell.operations.archive.flash.shown() && shell.operations.archive.flash.running(),
            "reopening shows the same run"
        );
        assert_eq!(
            shell.operations.archive.handed, None,
            "reopening takes the run back"
        );
    });
    assert!(
        !drain(&mut requests)
            .iter()
            .any(|line| line.contains(r#""c":"flash""#)),
        "reopening never restarts the flash"
    );
    assert_ne!(
        footer(&mut cx, handle),
        handed,
        "reopening takes the run off the status line"
    );
    // A terminal line from another backend is not this flash's; its own ends the run.
    receive(
        &mut cx,
        handle,
        1,
        r#"{"t":"flashdone","id":31,"ok":true,"phase":"verify","at":0,"err":"","blank":false}"#,
    );
    assert!(
        edit(&mut cx, handle, |shell, _, _| shell
            .operations
            .archive
            .flash
            .running()),
        "a wrong owner's reply is ignored"
    );
    receive(
        &mut cx,
        handle,
        0,
        r#"{"t":"flashdone","id":31,"ok":true,"phase":"verify","at":0,"err":"","blank":false}"#,
    );
    edit(&mut cx, handle, |shell, _, _| {
        let flash = &shell.operations.archive.flash;
        assert!(
            !flash.running() && flash.ending().is_some(),
            "the shown view lands on its ending"
        );
    });
    let leaked = drain(&mut other_requests);
    assert!(
        leaked.is_empty(),
        "nothing reached the other pane's backend: {leaked:?}"
    );
}

#[test]
fn m9_integ_a_chooser_that_cannot_start_says_so() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, mut requests, _) = start(&mut cx);
    edit(&mut cx, handle, |shell, _, _| {
        shell.operations.archive.chooser =
            || std::process::Command::new("/nonexistent/flea-chooser-fixture");
    });
    choose(&mut cx, handle, 45, "flashDrive", "", &["/files/disk.img"]);
    drain(&mut requests);
    edit(&mut cx, handle, |shell, window, cx| {
        let effects = shell.operations.archive.flash.choose_image();
        shell.archive_effects(0, effects, window, cx);
    });
    cx.run_until_parked();
    frame(&mut cx, handle);
    let said = footer(&mut cx, handle);
    assert!(
        said.contains("Could not start /nonexistent/flea-chooser-fixture to choose an image"),
        "a chooser that never started names its binary: {said:?}"
    );
    assert!(
        drain(&mut requests).is_empty(),
        "no image is asked after a failed chooser"
    );
}

#[test]
fn a_chooser_that_fails_says_why() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, mut requests, _) = start(&mut cx);
    edit(&mut cx, handle, |shell, _, _| {
        shell.operations.archive.chooser = || {
            let mut command = std::process::Command::new("/bin/sh");
            command.args(["-c", "echo noise >&2; echo 'the chooser broke' >&2; exit 2"]);
            command
        };
    });
    choose(&mut cx, handle, 47, "flashDrive", "", &["/files/disk.img"]);
    drain(&mut requests);
    edit(&mut cx, handle, |shell, window, cx| {
        let effects = shell.operations.archive.flash.choose_image();
        shell.archive_effects(0, effects, window, cx);
    });
    cx.run_until_parked();
    frame(&mut cx, handle);
    let said = footer(&mut cx, handle);
    assert!(
        said.contains("the chooser broke"),
        "a chooser that failed names its last line of stderr: {said:?}"
    );
    assert!(
        !said.contains("noise"),
        "only the last line of stderr is named: {said:?}"
    );
    assert!(
        drain(&mut requests).is_empty(),
        "no image is asked after a failed chooser"
    );
}

#[test]
fn a_chooser_that_fails_silently_names_its_exit_status() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, mut requests, _) = start(&mut cx);
    edit(&mut cx, handle, |shell, _, _| {
        shell.operations.archive.chooser = || {
            let mut command = std::process::Command::new("/bin/sh");
            command.args(["-c", "exit 3"]);
            command
        };
    });
    choose(&mut cx, handle, 49, "flashDrive", "", &["/files/disk.img"]);
    drain(&mut requests);
    edit(&mut cx, handle, |shell, window, cx| {
        let effects = shell.operations.archive.flash.choose_image();
        shell.archive_effects(0, effects, window, cx);
    });
    cx.run_until_parked();
    frame(&mut cx, handle);
    let said = footer(&mut cx, handle);
    assert!(
        said.contains("exit status: 3"),
        "a chooser with an empty stderr names its exit status: {said:?}"
    );
}

#[test]
fn a_chooser_failure_after_the_pane_closed_stays_silent() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, mut requests, _) = start(&mut cx);
    edit(&mut cx, handle, |shell, _, _| {
        shell.operations.archive.chooser = || {
            let mut command = std::process::Command::new("/bin/sh");
            command.args(["-c", "echo 'the chooser broke' >&2; exit 2"]);
            command
        };
    });
    choose(&mut cx, handle, 50, "flashDrive", "", &["/files/disk.img"]);
    drain(&mut requests);
    // One update, so the chooser's answer can only arrive after the pane has closed.
    edit(&mut cx, handle, |shell, window, cx| {
        let effects = shell.operations.archive.flash.choose_image();
        shell.archive_effects(0, effects, window, cx);
        let _ = shell.operations.archive.flash.close();
    });
    cx.run_until_parked();
    frame(&mut cx, handle);
    let said = footer(&mut cx, handle);
    assert!(
        !said.contains("the chooser broke"),
        "a failure the pane no longer waits for says nothing: {said:?}"
    );
}

#[test]
fn a_cancelled_chooser_stays_silent() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, mut requests, _) = start(&mut cx);
    edit(&mut cx, handle, |shell, _, _| {
        shell.operations.archive.chooser = || std::process::Command::new("/bin/true");
    });
    choose(&mut cx, handle, 48, "flashDrive", "", &["/files/disk.img"]);
    drain(&mut requests);
    let before = footer(&mut cx, handle);
    edit(&mut cx, handle, |shell, window, cx| {
        let effects = shell.operations.archive.flash.choose_image();
        shell.archive_effects(0, effects, window, cx);
    });
    cx.run_until_parked();
    frame(&mut cx, handle);
    assert_eq!(footer(&mut cx, handle), before, "a cancel says nothing");
    assert!(
        drain(&mut requests).is_empty(),
        "a cancel asks for no image"
    );
}

#[test]
fn m9_integ_a_drained_backend_ends_the_handed_flash() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, mut requests, _) = start(&mut cx);
    choose(&mut cx, handle, 46, "flashDrive", "", &["/files/disk.img"]);
    receive(
        &mut cx,
        handle,
        0,
        r#"{"t":"flashimage","path":"/files/disk.img","kind":"img","bytes":100,"unpacked":100,"known":true,"member":""}"#,
    );
    receive(
        &mut cx,
        handle,
        0,
        r#"{"t":"flashtargets","drives":[{"dev":"sdz","seq":3,"serial":"S1","label":"STICK","model":"Fixture","bytes":4096,"volumes":[],"refused":""}]}"#,
    );
    for _ in 0..4 {
        if edit(&mut cx, handle, |shell, _, _| {
            shell.operations.archive.flash.park()
        }) == super::super::flash::Seat::Flash
        {
            break;
        }
        keys(&mut cx, handle, "down");
    }
    keys(&mut cx, handle, "enter");
    drain(&mut requests);
    receive(
        &mut cx,
        handle,
        0,
        r#"{"t":"flashstarted","id":31,"total":100,"known":true}"#,
    );
    keys(&mut cx, handle, "escape");
    let handed = footer(&mut cx, handle);
    assert_eq!(
        edit(&mut cx, handle, |shell, _, _| shell
            .operations
            .archive
            .handed),
        Some(31),
        "the run is handed"
    );
    edit(&mut cx, handle, |shell, window, cx| {
        shell.receive(0, BackendEvent::Drained, window, cx)
    });
    frame(&mut cx, handle);
    edit(&mut cx, handle, |shell, _, _| {
        assert_eq!(
            shell.operations.archive.handed, None,
            "the dead run leaves the status line"
        );
        assert!(
            !shell.operations.archive.flash.running(),
            "the dead run no longer runs"
        );
    });
    let said = footer(&mut cx, handle);
    assert_ne!(said, handed, "the footer no longer shows the handed run");
    assert!(
        said.contains("Could not flash STICK"),
        "the failure is said: {said:?}"
    );
}
