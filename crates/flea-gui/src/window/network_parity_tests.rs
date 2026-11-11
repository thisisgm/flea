//! Network form words and remembered share labels through the mounted window.
use super::*;
use crate::window::u07_tests::{press, read};

#[gpui::test]
fn pb_network_2_nfs_note_draws_the_board_sentences(cx: &mut TestAppContext) {
    let (shell, cx, _) = open(Rc::new(Recorder::default()), cx);
    cx.simulate_keystrokes("a");
    frame(cx);
    let chip = read(&shell, cx, "networkChipCentre", &["NFS"]);
    assert!(!chip.is_empty(), "the add key opens the network card");
    press(cx, &chip, 1);
    frame(cx);
    assert_eq!(
        read(&shell, cx, "networkNote", &[]),
        "No password needed. The NFS server trusts this computer."
    );
}

#[gpui::test]
fn pb_network_2_tls_label_keeps_the_shipped_sentence(cx: &mut TestAppContext) {
    let (shell, cx, _) = open(Rc::new(Recorder::default()), cx);
    cx.simulate_keystrokes("a");
    frame(cx);
    let chip = read(&shell, cx, "networkChipCentre", &["FTPS"]);
    press(cx, &chip, 1);
    frame(cx);
    let label = shell.read_with(cx, |shell, cx| {
        shell
            .rail
            .as_ref()
            .unwrap()
            .network
            .as_ref()
            .unwrap()
            .card
            .read(cx)
            .words
            .tls
            .clone()
    });
    assert_eq!(read(&shell, cx, "networkProtocol", &[]), "FTPS");
    assert_eq!(label, "Encrypted (TLS)");
}

#[gpui::test]
fn pb_network_2_opened_share_draws_its_saved_label_as_a_root_crumb(cx: &mut TestAppContext) {
    use crate::window::dispatch::test_shell::{land, open_root};
    let recorder = Rc::new(Recorder::default());
    cx.update(|cx| {
        cx.set_global(crate::g2::device_rail::source::fixture::Services {
            runner: recorder.clone(),
            hostname: "testhost".into(),
        })
    });
    let (shell, cx) = open_root(cx, "default");
    let first = *recorder
        .find("-li")
        .last()
        .expect("the shared mount listing");
    recorder.answer(first, 0, "Mount(0): NAS -> sftp://user@host/folder\n");
    cx.run_until_parked();
    land(&shell, cx, "/files", Vec::new(), 1);
    shell.update(cx, |shell, cx| {
        shell.network_publish(Some(vec![share()]), cx)
    });
    frame(cx);
    let index = shell.read_with(cx, |shell, cx| {
        shell
            .rail
            .as_ref()
            .unwrap()
            .places
            .read(cx)
            .entries()
            .iter()
            .position(|row| row.group == Group::Network)
            .unwrap()
    });
    let centre = read(&shell, cx, "railRowCentre", &[&index.to_string()]);
    press(cx, &centre, 1);
    frame(cx);
    let (service, origin) = shell.read_with(cx, |shell, _| {
        let network = shell.rail.as_ref().unwrap().network.as_ref().unwrap();
        (
            network.service.clone(),
            network
                .origins
                .last()
                .expect("the row opened its service")
                .token,
        )
    });
    cx.update(|_, cx| {
        service.update(cx, |_, cx| {
            cx.emit(ServiceEvent(Effect::Open {
                path: "/mounted/share".into(),
                uri: share().uri,
                label: "NAS".into(),
                origin,
            }))
        })
    });
    land(&shell, cx, "/mounted/share", Vec::new(), 2);
    assert_eq!(read(&shell, cx, "crumbText", &[]), "NAS");
    cx.executor().allow_parking();
    let scratch =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/PB-NETWORK-2");
    std::fs::create_dir_all(&scratch).unwrap();
    let output = std::process::Command::new("mktemp")
        .args(["-d", "--"])
        .arg(scratch.join("marks.XXXXXXXX"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let sandbox = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    assert!(sandbox.is_absolute());
    std::fs::write(sandbox.join(".flea-test-sandbox"), b"").unwrap();
    let marks = sandbox.join("bookmarks");
    std::fs::write(&marks, "sftp://user@host/folder NAS\n").unwrap();
    shell.update(cx, |shell, _| {
        shell.network().unwrap().bookmarks = marks.clone()
    });
    press(cx, &centre, 1);
    frame(cx);
    cx.simulate_keystrokes("tab");
    frame(cx);
    cx.simulate_keystrokes("r");
    frame(cx);
    assert_eq!(
        read(&shell, cx, "railRenamingIndex", &[]),
        index.to_string()
    );
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("Work files");
    cx.simulate_keystrokes("enter");
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while read(&shell, cx, "crumbText", &[]) != "Work files" {
        assert!(
            std::time::Instant::now() < deadline,
            "a persisted rename relabels the crumb"
        );
        frame(cx);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(std::fs::read_to_string(&marks)
        .unwrap()
        .contains("Work files"));
    assert!(sandbox.is_absolute() && sandbox.join(".flea-test-sandbox").is_file());
    std::fs::remove_dir_all(&sandbox).unwrap();
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

/// Fires Remove from Network on the row by keyboard over a sandboxed bookmarks file; answers the menu rows and the file after.
fn remove_from_network(
    cx: &mut TestAppContext,
    row: RailEntry,
    saved: &str,
    said: &str,
) -> (String, String) {
    use crate::window::dispatch::test_shell::{land, open_root};
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", Vec::new(), 1);
    shell.update(cx, |shell, cx| shell.network_publish(Some(vec![row]), cx));
    frame(cx);
    cx.executor().allow_parking();
    let scratch =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/FIX-NETPLACE");
    std::fs::create_dir_all(&scratch).unwrap();
    let output = std::process::Command::new("mktemp")
        .args(["-d", "--"])
        .arg(scratch.join("marks.XXXXXXXX"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let sandbox = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    assert!(sandbox.is_absolute());
    std::fs::write(sandbox.join(".flea-test-sandbox"), b"").unwrap();
    let marks = sandbox.join("bookmarks");
    std::fs::write(&marks, saved).unwrap();
    shell.update_in(cx, |shell, window, cx| {
        shell.network().unwrap().bookmarks = marks.clone();
        let producer = shell.rail.as_ref().unwrap().places.clone();
        let entries = producer.read(cx).entries();
        let index = entries
            .iter()
            .position(|row| row.group == Group::Network)
            .unwrap();
        producer.update(cx, |producer, cx| producer.set_cursor(index, cx));
        let side = shell.focus_side;
        shell.network_menu(side, &entries[index], window, cx);
    });
    frame(cx);
    // 1610 (EY-280): the share's open row menu reads as visible.
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "true");
    let rows = read(&shell, cx, "contextMenuEntries", &[]);
    let remove = rows
        .split('|')
        .position(|row| row == "Remove from Network")
        .expect("the share's menu offers Remove from Network")
        .to_string();
    for _ in 0..8 {
        if read(&shell, cx, "contextMenuCursor", &[]) == remove {
            break;
        }
        cx.simulate_keystrokes("down");
        frame(cx);
    }
    assert_eq!(read(&shell, cx, "contextMenuCursor", &[]), remove);
    cx.simulate_keystrokes("enter");
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while read(&shell, cx, "lastMessage", &[]) != said {
        assert!(
            std::time::Instant::now() < deadline,
            "Remove from Network says: {said}"
        );
        frame(cx);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "false");
    let after = std::fs::read_to_string(&marks).unwrap();
    assert!(sandbox.is_absolute() && sandbox.join(".flea-test-sandbox").is_file());
    std::fs::remove_dir_all(&sandbox).unwrap();
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
    (rows, after)
}

fn mounted_share() -> RailEntry {
    RailEntry {
        mounted: true,
        path: "/mounted/share".into(),
        ..share()
    }
}

#[gpui::test]
fn ey324_remove_from_network_forgets_the_saved_line_and_says_so(cx: &mut TestAppContext) {
    let (rows, after) = remove_from_network(
        cx,
        share(),
        "sftp://user@host/folder NAS\nsmb://keep/share Keep\n",
        "NAS is forgotten",
    );
    assert_eq!(rows, "Rename|Edit Address…|-|Remove from Network");
    assert_eq!(after, "smb://keep/share Keep\n");
}

#[gpui::test]
fn ey324_removing_a_mounted_share_says_it_stays_until_unmounted(cx: &mut TestAppContext) {
    let (_, after) = remove_from_network(
        cx,
        mounted_share(),
        "sftp://user@host/folder NAS\nsmb://keep/share Keep\n",
        "NAS is forgotten, and stays in the sidebar until it is unmounted",
    );
    assert_eq!(after, "smb://keep/share Keep\n");
}

#[gpui::test]
fn ey324_removing_an_unmounted_place_already_gone_says_it_is_forgotten(cx: &mut TestAppContext) {
    let (_, after) =
        remove_from_network(cx, share(), "smb://keep/share Keep\n", "NAS is forgotten");
    assert_eq!(
        after, "smb://keep/share Keep\n",
        "a file that lacks the line is left as it was"
    );
}

#[gpui::test]
fn ey324_removing_an_unsaved_mount_says_it_was_not_saved(cx: &mut TestAppContext) {
    let (_, after) = remove_from_network(
        cx,
        mounted_share(),
        "smb://keep/share Keep\n",
        "NAS is not saved, and stays in the sidebar until it is unmounted",
    );
    assert_eq!(
        after, "smb://keep/share Keep\n",
        "a file that lacks the line is left as it was"
    );
}

// German types a space with AltGr+Space, which is text, so only the plain Space chooses the menu's cursor row.
#[gpui::test]
fn altgr_space_chooses_no_network_menu_row(cx: &mut TestAppContext) {
    use crate::window::dispatch::test_shell::{german_press, land, open_root};
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", Vec::new(), 1);
    shell.update(cx, |shell, cx| {
        shell.network_publish(Some(vec![share()]), cx)
    });
    frame(cx);
    let index = shell.update_in(cx, |shell, window, cx| {
        let producer = shell.rail.as_ref().unwrap().places.clone();
        let entries = producer.read(cx).entries();
        let index = entries
            .iter()
            .position(|row| row.group == Group::Network)
            .unwrap();
        producer.update(cx, |producer, cx| producer.set_cursor(index, cx));
        producer.read(cx).model.borrow_mut().pointer_focus = false;
        let side = shell.focus_side;
        shell.network_menu(side, &entries[index], window, cx);
        index
    });
    frame(cx);
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "true");
    assert_eq!(read(&shell, cx, "contextMenuCursor", &[]), "0");
    cx.simulate_event(german_press("space", " ", None, true));
    frame(cx);
    assert_eq!(
        read(&shell, cx, "contextMenuVisible", &[]),
        "true",
        "AltGr+Space types a space and keeps the menu"
    );
    cx.simulate_event(german_press("space", " ", None, false));
    frame(cx);
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "false");
    assert_eq!(
        read(&shell, cx, "railRenamingIndex", &[]),
        index.to_string(),
        "Space chooses Rename, the cursor row"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
