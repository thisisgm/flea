//! A refused eject leaves the window where it was, a retry says its own verdict, and a running copy refuses first.
use super::*;

/// Answers the newest job whose command line starts with `prefix`, as its child would.
fn answer_last(service: &Ordered, prefix: &str, body: &str) {
    let jobs = service.0.jobs.borrow();
    let (_, sender, _) = jobs
        .iter()
        .rev()
        .find(|(argv, _, _)| argv.join(" ").starts_with(prefix))
        .unwrap_or_else(|| panic!("a {prefix} job ran"));
    sender
        .try_send(ChildEvent::Exited(Outcome {
            code: 0,
            stdout: body.into(),
            stderr: String::new(),
        }))
        .unwrap();
}

fn ejects(service: &Ordered) -> usize {
    jobs(service)
        .iter()
        .filter(|job| *job == "gio mount -e /media/128GB")
        .count()
}

fn place(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> String {
    shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().pane.place().to_owned()
    })
}

fn click_mark(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    let hit = row(shell, "128GB", cx).eject_hit().unwrap();
    cx.simulate_mouse_down(centre(hit), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(centre(hit), MouseButton::Left, Modifiers::none());
    frame(cx);
}

/// gio exits and the listing after it still mounts the stick, so the eject is refused as busy.
fn refuse_busy(service: &Ordered, cx: &mut VisualTestContext) {
    answer_last(service, "gio mount -e", "");
    frame(cx);
    answer_last(service, "lsblk", BLOCKS);
    frame(cx);
}

fn eject_safely(service: &Ordered, cx: &mut VisualTestContext) {
    answer_last(service, "gio mount -e", "");
    frame(cx);
    answer_last(
        service,
        "lsblk",
        &BLOCKS.replace(
            r#""mountpoints":["/media/128GB"]"#,
            r#""mountpoints":[null]"#,
        ),
    );
    frame(cx);
}

/// The pane's backend reads `line` as its worker would, then the window receives it.
fn reply(shell: &Entity<Shell>, line: &str, cx: &mut VisualTestContext) {
    shell.read_with(cx, |shell, _| {
        let backend = shell.panes[0].as_ref().unwrap().backend.as_ref().unwrap();
        backend.test_answered(line);
    });
    let event = crate::backend::Backend::test_reply_line(line);
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.receive(0, event, window, cx)));
    frame(cx);
}

fn send_transfer(shell: &Entity<Shell>, line: &str, cx: &mut VisualTestContext) {
    shell.read_with(cx, |shell, _| {
        let backend = shell.panes[0].as_ref().unwrap().backend.as_ref().unwrap();
        backend.line(line.into()).unwrap();
    });
    reply(
        shell,
        r#"{"t":"transferstarted","id":12,"n":1,"moving":false}"#,
        cx,
    );
}

#[gpui::test]
fn fx_eject_s2_9_a_retry_after_a_busy_refusal_says_safe_to_unplug(cx: &mut TestAppContext) {
    let (shell, service, cx) = shell_at("/fixture/work", cx);
    click_mark(&shell, cx);
    refuse_busy(&service, cx);
    assert_eq!(
        status(&shell, cx),
        "128GB is still mounted; close what is using it."
    );
    click_mark(&shell, cx);
    assert_eq!(
        ejects(&service),
        2,
        "the mark ejects again: {:?}",
        jobs(&service)
    );
    eject_safely(&service, cx);
    assert_eq!(
        status(&shell, cx),
        "Ejected 128GB. It is safe to unplug.",
        "the new verdict withdraws the refusal it replaces"
    );
}

#[gpui::test]
fn fx_eject_s8_9_a_busy_refusal_keeps_the_folder_and_ctrl_e_retries(cx: &mut TestAppContext) {
    let (shell, service, cx) = shell_at("/media/128GB/photos", cx);
    listing_focus(&shell, cx);
    cx.simulate_keystrokes("ctrl-e");
    frame(cx);
    assert_eq!(ejects(&service), 1, "{:?}", jobs(&service));
    assert_ne!(
        place(&shell, cx),
        "/media/128GB/photos",
        "the pane lets go of the stick while it ejects"
    );
    refuse_busy(&service, cx);
    assert_eq!(
        place(&shell, cx),
        "/media/128GB/photos",
        "a refused eject returns the pane to the folder it left"
    );
    listing_focus(&shell, cx);
    cx.simulate_keystrokes("ctrl-e");
    frame(cx);
    assert_eq!(
        ejects(&service),
        2,
        "the second Ctrl+E retries the stick: {:?}",
        jobs(&service)
    );
    eject_safely(&service, cx);
    assert_eq!(status(&shell, cx), "Ejected 128GB. It is safe to unplug.");
    assert_ne!(
        place(&shell, cx),
        "/media/128GB/photos",
        "an eject that took never returns the pane"
    );
}

#[gpui::test]
fn fx_eject_s8_9_a_pane_moved_on_during_the_eject_stays_where_it_went(cx: &mut TestAppContext) {
    let (shell, service, cx) = shell_at("/media/128GB/photos", cx);
    listing_focus(&shell, cx);
    cx.simulate_keystrokes("ctrl-e");
    frame(cx);
    shell.update_in(cx, |shell, window, cx| {
        let step = crate::window::navigate::Step::OpenPlace("/fixture/elsewhere".into());
        let _ = shell.step(0, step, window, cx);
    });
    refuse_busy(&service, cx);
    assert_eq!(
        place(&shell, cx),
        "/fixture/elsewhere",
        "the user's own navigation wins over the return"
    );
}

#[gpui::test]
fn fx_eject_s1a_4_ctrl_e_on_a_place_names_the_row(cx: &mut TestAppContext) {
    let (shell, service, cx) = shell_at("/fixture/work", cx);
    let home = row(&shell, "Home", cx);
    shell.update(cx, |shell, cx| shell.rail_focus(true, cx));
    frame(cx);
    let producer = producer(&shell, cx);
    for _ in 0..16 {
        if producer.read_with(cx, |p, _| p.cursor()) == Some(home.index) {
            break;
        }
        cx.simulate_keystrokes("up");
        frame(cx);
    }
    assert_eq!(producer.read_with(cx, |p, _| p.cursor()), Some(home.index));
    cx.simulate_keystrokes("ctrl-e");
    assert_eq!(status(&shell, cx), "Home has nothing to eject or unmount");
    assert_eq!(ejects(&service), 0);
}

#[gpui::test]
fn fx_eject_s8_16_a_running_copy_onto_the_stick_refuses_before_leaving(cx: &mut TestAppContext) {
    let (shell, service, cx) = shell_at("/media/128GB/photos", cx);
    send_transfer(
        &shell,
        r#"{"c":"transfer","op":"copy","paths":["/fixture/home/big"],"dest":"/media/128GB/photos"}"#,
        cx,
    );
    listing_focus(&shell, cx);
    cx.simulate_keystrokes("ctrl-e");
    frame(cx);
    assert_eq!(
        ejects(&service),
        0,
        "nothing ejects under a running copy: {:?}",
        jobs(&service)
    );
    assert_eq!(
        place(&shell, cx),
        "/media/128GB/photos",
        "the pane and its copy card stay"
    );
    let releasing = shell.read_with(cx, |shell, cx| {
        shell
            .rail
            .as_ref()
            .unwrap()
            .devices
            .read(cx)
            .devices
            .releasing()
    });
    assert!(!releasing, "the refused release holds nothing");
    assert_eq!(
        status(&shell, cx),
        "A copy on 128GB is still running. Try again when it finishes."
    );
    reply(
        &shell,
        r#"{"t":"transferdone","id":12,"ok":1,"failed":0,"cancelled":false}"#,
        cx,
    );
    reply(
        &shell,
        r#"{"t":"job","job":{"id":12,"kind":"copy","state":"done","head":""},"unfinished":0}"#,
        cx,
    );
    listing_focus(&shell, cx);
    cx.simulate_keystrokes("ctrl-e");
    frame(cx);
    assert_eq!(
        ejects(&service),
        1,
        "once the copy is done the eject runs: {:?}",
        jobs(&service)
    );
}

#[gpui::test]
fn fx_eject_s8_16_a_copy_off_the_stick_refuses_from_the_mark(cx: &mut TestAppContext) {
    let (shell, service, cx) = shell_at("/fixture/work", cx);
    send_transfer(
        &shell,
        r#"{"c":"transfer","op":"move","paths":["/media/128GB/photos/a.jpg"],"dest":"/fixture/work"}"#,
        cx,
    );
    click_mark(&shell, cx);
    assert_eq!(ejects(&service), 0, "{:?}", jobs(&service));
    assert_eq!(
        status(&shell, cx),
        "A copy on 128GB is still running. Try again when it finishes."
    );
}

#[gpui::test]
fn fx_eject_s8_16_an_unrelated_copy_never_holds_the_eject(cx: &mut TestAppContext) {
    let (shell, service, cx) = shell_at("/media/128GB/photos", cx);
    send_transfer(
        &shell,
        r#"{"c":"transfer","op":"copy","paths":["/fixture/home/a"],"dest":"/fixture/home/b"}"#,
        cx,
    );
    listing_focus(&shell, cx);
    cx.simulate_keystrokes("ctrl-e");
    frame(cx);
    assert_eq!(
        ejects(&service),
        1,
        "a copy elsewhere is no reason to refuse: {:?}",
        jobs(&service)
    );
}

#[gpui::test]
fn fx_eject_s8_16_a_copy_the_backend_has_not_started_still_refuses(cx: &mut TestAppContext) {
    let (shell, service, cx) = shell_at("/media/128GB/photos", cx);
    shell.read_with(cx, |shell, _| {
        let backend = shell.panes[0].as_ref().unwrap().backend.as_ref().unwrap();
        let line =
            r#"{"c":"transfer","op":"copy","paths":["/fixture/home/big"],"dest":"/media/128GB"}"#;
        backend.line(line.into()).unwrap();
    });
    listing_focus(&shell, cx);
    cx.simulate_keystrokes("ctrl-e");
    frame(cx);
    assert_eq!(ejects(&service), 0, "{:?}", jobs(&service));
    assert_eq!(
        status(&shell, cx),
        "A copy on 128GB is still running. Try again when it finishes."
    );
}

const USB_DRIVE: &str = r#"{"blockdevices":[
{"name":"nvme0n1","path":"/dev/nvme0n1","type":"disk","children":[{"name":"nvme0n1p1","type":"part","mountpoints":["/"],"fstype":"ext4"}]},
{"name":"sdc","path":"/dev/sdc","type":"disk","tran":"usb","rm":false,"children":[{"name":"sdc1","path":"/dev/sdc1","type":"part","rm":false,"label":"One","fstype":"ext4","mountpoints":["/media/one"]},{"name":"sdc2","path":"/dev/sdc2","type":"part","rm":false,"label":"Two","fstype":"ext4","mountpoints":["/media/two"]}]}
]}"#;

#[gpui::test]
fn fx_eject_s8_16_a_copy_onto_another_partition_of_the_drive_refuses_its_power_off(
    cx: &mut TestAppContext,
) {
    let (shell, service, cx) = shell_with_listing("/media/one/photos", USB_DRIVE, cx);
    send_transfer(
        &shell,
        r#"{"c":"transfer","op":"copy","paths":["/fixture/home/big"],"dest":"/media/two"}"#,
        cx,
    );
    listing_focus(&shell, cx);
    cx.simulate_keystrokes("ctrl-e");
    frame(cx);
    let unmounts = jobs(&service)
        .iter()
        .filter(|job| job.starts_with("gio mount -u") || job.starts_with("gio mount -t"))
        .count();
    assert_eq!(unmounts, 0, "{:?}", jobs(&service));
    assert_eq!(
        status(&shell, cx),
        "A copy on Two is still running. Try again when it finishes."
    );
    assert_eq!(place(&shell, cx), "/media/one/photos");
}
