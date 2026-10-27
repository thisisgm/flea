//! Network form words and remembered share labels through the mounted window.
use super::*;
use crate::window::u07_tests::{press, read};

#[gpui::test]
fn pb_network_2_nfs_note_keeps_the_shipped_sentence(cx: &mut TestAppContext) {
    let (shell, cx, _) = open(Rc::new(Recorder::default()), cx);
    cx.simulate_keystrokes("a");
    frame(cx);
    let chip = read(&shell, cx, "networkChipCentre", &["NFS"]);
    assert!(!chip.is_empty(), "the add key opens the network card");
    press(cx, &chip, 1);
    frame(cx);
    assert_eq!(
        read(&shell, cx, "networkNote", &[]),
        "No password needed: the NFS server trusts this computer"
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
    let (shell, cx) = open_root(cx, "default");
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
