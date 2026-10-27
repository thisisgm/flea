//! Saved network labels reach mounted breadcrumbs and survive live provider labels.
use super::*;
use crate::window::u07_tests::{press, read};

fn bookmarks(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> PathBuf {
    let parent =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/PB-PATH");
    std::fs::create_dir_all(&parent).unwrap();
    let root = std::process::Command::new("mktemp")
        .arg("-d")
        .arg(parent.join("aliases.XXXXXX"))
        .output()
        .unwrap();
    assert!(root.status.success());
    let root = PathBuf::from(String::from_utf8(root.stdout).unwrap().trim());
    std::fs::write(root.join(".flea-test-sandbox"), "network aliases").unwrap();
    let path = root.join("bookmarks");
    shell.update(cx, |shell, _| {
        shell.network().unwrap().bookmarks = path.clone()
    });
    path
}

fn list(recorder: &Recorder, cx: &mut VisualTestContext) {
    let at = *recorder
        .find("-li")
        .last()
        .expect("the live network listing");
    recorder.answer(at, 0, "");
    cx.run_until_parked();
    frame(cx);
}

#[gpui::test]
fn pb_path_saved_network_label_and_rename_reach_the_drawn_root(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, _lines) = open(recorder.clone(), cx);
    let path = bookmarks(&shell, cx);
    std::fs::write(&path, "sftp://user@host/folder NAS\n").unwrap();
    list(&recorder, cx);
    let index = shell.read_with(cx, |shell, cx| {
        shell
            .rail
            .as_ref()
            .unwrap()
            .places
            .read(cx)
            .entries()
            .iter()
            .position(|entry| entry.group == Group::Network)
            .unwrap()
    });
    let centre = read(&shell, cx, "railRowCentre", &[&index.to_string()]);
    assert!(
        !centre.is_empty(),
        "the saved place has a mounted pointer target"
    );
    press(cx, &centre, 1);
    answer_mount(&recorder, "/fixture/mnt", cx);
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.chrome.aliases.clone()),
        vec![crate::path::crumbs::Alias {
            path: "/fixture/mnt".into(),
            label: "NAS".into(),
            key: "sftp://user@host/folder".into(),
        }]
    );
    let side = shell.read_with(cx, |shell, _| shell.focus_side);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            for reply in [
                flea::protocol::Reply::Listed {
                    path: "/fixture/mnt".into(),
                    total: 1,
                },
                flea::protocol::Reply::Rows {
                    start: 0,
                    rows: vec![flea::protocol::RowFacts {
                        name: "child".into(),
                        dir: true,
                        ..Default::default()
                    }],
                    listing: 3,
                },
            ] {
                shell.receive(side, crate::backend::Event::Reply(reply), window, cx);
            }
        })
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(400));
    frame(cx);
    assert_eq!(
        read(&shell, cx, "paneCrumbText", &[&side.to_string()]),
        "NAS"
    );
    std::fs::write(&path, "sftp://user@host/folder Archive\n").unwrap();
    list(&recorder, cx);
    assert_eq!(
        read(&shell, cx, "paneCrumbText", &[&side.to_string()]),
        "Archive"
    );
    shell.update(cx, |shell, cx| {
        let mut live = share();
        live.label = "Generated provider label".into();
        shell.network_publish(Some(vec![live]), cx);
    });
    frame(cx);
    assert_eq!(
        read(&shell, cx, "paneCrumbText", &[&side.to_string()]),
        "Archive"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
