//! Dropbox and local FUSE mounts are published by the mounted network rail's own poll.
use super::*;
use crate::window::u07_tests::{press, read};

#[gpui::test]
fn pb_network_2_local_cloud_sources_publish_rows_and_the_dropbox_mark(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let scratch =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/PB-NETWORK-2");
    std::fs::create_dir_all(&scratch).unwrap();
    let output = std::process::Command::new("mktemp")
        .args(["-d", "--"])
        .arg(scratch.join("cloud.XXXXXXXX"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let sandbox = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim())
        .canonicalize()
        .unwrap();
    std::fs::write(sandbox.join(".flea-test-sandbox"), b"").unwrap();
    let home = sandbox.join("home");
    let dropbox = sandbox.join("dropbox.json");
    let mountinfo = sandbox.join("mountinfo");
    let folder = home.join("Dropbox");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        &dropbox,
        format!(r#"{{"personal":{{"path":"{}"}}}}"#, folder.display()),
    )
    .unwrap();
    std::fs::write(&mountinfo, format!("20 1 0:20 / {}/Merged rw - fuse.mergerfs union: rw\n21 1 0:21 / {}/Drive rw - fuse.rclone remote: rw\n22 1 0:22 / {}/Outside rw - fuse.mergerfs union: rw\n", home.display(), home.display(), sandbox.display())).unwrap();
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, _lines) = open(recorder.clone(), cx);
    shell.update(cx, |shell, _| {
        let sources = &mut shell.network().unwrap().sources;
        sources.home = home;
        sources.mountinfo = mountinfo;
        sources.dropbox = dropbox;
    });
    let listing = *recorder
        .find("-li")
        .last()
        .expect("the shown rail asked its listing");
    recorder.answer(listing, 0, "");
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let entries = loop {
        frame(cx);
        if read(&shell, cx, "networkBuilt", &[]) == "true" {
            break read(&shell, cx, "networkEntries", &[]);
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the source read publishes"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(
        entries.contains("Dropbox|network|dropbox|true"),
        "{entries}"
    );
    assert!(entries.contains("Merged|network|cloud|true"), "{entries}");
    assert!(
        entries.contains("Drive|network|share|true"),
        "network FUSE keeps its share row: {entries}"
    );
    assert!(
        !entries.contains("Outside"),
        "a local cloud mount must be inside home"
    );
    let index = shell.read_with(cx, |shell, cx| {
        let producer = shell.rail.as_ref().unwrap().places.read(cx);
        let index = producer
            .entries()
            .iter()
            .position(|row| row.path == folder.to_str().unwrap())
            .unwrap();
        let laid = producer.laid.borrow();
        let row = laid.row(index).unwrap();
        assert_eq!(row.glyph, "dropbox");
        assert!(!row.indicator_visible, "Dropbox has no status dot");
        index
    });
    let centre = read(&shell, cx, "railRowCentre", &[&index.to_string()]);
    press(cx, &centre, 1);
    frame(cx);
    assert!(
        recorder.mounts().is_empty(),
        "a Dropbox folder opens locally"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
    assert!(sandbox.is_absolute() && sandbox.join(".flea-test-sandbox").is_file());
    std::fs::remove_dir_all(&sandbox).unwrap();
}
