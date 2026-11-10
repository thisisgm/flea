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

/// S8-14: Flea launched on a folder inside a mounted share names the share's root by its rail label, as a rail click does.
#[gpui::test]
fn s8_14_a_launch_path_inside_a_mounted_share_takes_its_rail_label(cx: &mut TestAppContext) {
    let (shell, cx, _lines) = open(Rc::new(Recorder::default()), cx);
    let root = "/run/user/1000/gvfs/dav:host=127.0.0.1,port=18767,ssl=false";
    // A gvfs share row carries its uri alone, as the rail rebuild sends it.
    let mounted = |label: &str, uri: &str| RailEntry {
        path: String::new(),
        label: label.into(),
        uri: uri.into(),
        mounted: true,
        ..share()
    };
    let aliases = |shell: &Entity<Shell>, cx: &mut VisualTestContext| {
        shell.read_with(cx, |shell, _| shell.chrome.aliases.clone())
    };
    shell.update(cx, |shell, cx| {
        shell.launch.paths[0] = format!("{root}/photos");
        shell.network_publish(
            Some(vec![
                mounted("127.0.0.1:9", "dav://127.0.0.1:9/"),
                mounted("other", "sftp://other/"),
                mounted("Studio NAS", "dav://127.0.0.1:18767/"),
            ]),
            cx,
        );
    });
    assert_eq!(
        aliases(&shell, cx),
        vec![crate::path::crumbs::Alias {
            path: root.into(),
            label: "Studio NAS".into(),
            key: "dav://127.0.0.1:18767/".into(),
        }],
        "only the share holding a launch path is named"
    );
    // S8-11: a read refused at that root is named by its rail label in the footer, never by the gvfs folder.
    let side = shell.read_with(cx, |shell, _| shell.focus_side);
    let line = format!(
        r#"{{"t":"error","where":"scan","path":"{root}","msg":"the drive could not read or write the data","mode":0}}"#
    );
    let refused = crate::backend::Backend::test_reply_line(&line);
    let (said, expected) = cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.panes[side].as_mut().unwrap().pane.begin(
                root.into(),
                crate::pane::swap::Options::default(),
                0,
            );
            shell.receive(side, refused, window, cx);
            let said = shell.status_messages(cx, |messages, _| {
                messages
                    .errors()
                    .iter()
                    .map(|entry| entry.text.clone())
                    .collect::<Vec<_>>()
            });
            let expected = shell
                .locale
                .catalog
                .tr_args("feedback.error.unread_named", &["Studio NAS"]);
            (said, expected)
        })
    });
    assert_eq!(said, [expected]);
    shell.update(cx, |shell, cx| {
        shell.launch.paths[0] = "/fixture/home".into();
        shell.network_publish(None, cx);
    });
    assert_eq!(
        aliases(&shell, cx).len(),
        1,
        "a later publish keeps the name"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

/// S8-14: a row with its own mount path names that root for a launch path inside it; a row whose path is only a name prefix of it, or lies below it, names nothing.
#[gpui::test]
fn s8_14_a_launch_path_inside_a_mounted_path_row_takes_its_rail_label(cx: &mut TestAppContext) {
    let (shell, cx, _lines) = open(Rc::new(Recorder::default()), cx);
    let mounted = |label: &str, path: &str, uri: &str| RailEntry {
        path: path.into(),
        label: label.into(),
        uri: uri.into(),
        mounted: true,
        ..share()
    };
    let smb = "/run/user/1000/gvfs/smb-share:server=nas,share=media";
    shell.update(cx, |shell, cx| {
        shell.launch.paths = ["/mnt/nas/films".into(), format!("{smb}/2026")];
        shell.network_publish(
            Some(vec![
                mounted("Prefix", "/mnt/na", "/mnt/na"),
                mounted("Below", "/mnt/nas/films/old", "/mnt/nas/films/old"),
                mounted("Vault", "/mnt/nas", "/mnt/nas"),
                mounted("Other share", "", "smb://nas/other/"),
                mounted("Media", "", "smb://nas/media/"),
            ]),
            cx,
        );
    });
    let mut named: Vec<(String, String)> = shell.read_with(cx, |shell, _| {
        shell
            .chrome
            .aliases
            .iter()
            .map(|alias| (alias.path.clone(), alias.label.clone()))
            .collect()
    });
    named.sort();
    assert_eq!(
        named,
        [
            ("/mnt/nas".to_owned(), "Vault".to_owned()),
            (smb.to_owned(), "Media".to_owned()),
        ]
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
