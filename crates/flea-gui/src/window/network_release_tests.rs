//! Ctrl+E on a share: from a listing inside a mounted share or from the rail's cursor row it unmounts through the network owner (ui/js/Eject.js release).
use super::*;

/// A mounted rclone share, released by fusermount3 with no gio listing needed.
fn fuse() -> RailEntry {
    RailEntry {
        path: "/mnt/rclone".into(),
        uri: "/mnt/rclone".into(),
        fstype: "fuse.rclone".into(),
        mounted: true,
        ..share()
    }
}

fn at(shell: &Entity<Shell>, path: &str, rows: Vec<RailEntry>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.network_publish(Some(rows), cx);
            let host = shell.panes[0].as_mut().unwrap();
            host.path = path.into();
            host.pane.swap.pending = None;
            shell.focus_side = 0;
            shell.rail_focus(false, cx);
            shell.keyboard.context = flea::keys::Context::Listing;
            window.focus(&shell.focus, cx);
        })
    });
    frame(cx);
}

fn releases(recorder: &Recorder) -> Vec<Vec<String>> {
    recorder
        .jobs
        .borrow()
        .iter()
        .filter(|job| {
            job.argv.first().map(String::as_str) == Some("fusermount3")
                || job.argv.iter().any(|arg| arg == "-u")
        })
        .map(|job| job.argv.clone())
        .collect()
}

fn status(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> String {
    frame(cx);
    shell.read_with(cx, |shell, cx| {
        let footer = shell.surfaces[0].footer.as_ref().unwrap().read(cx);
        let line = footer.line.read(cx);
        line.painted
            .get("primary")
            .map(|drawn| drawn.text.to_string())
            .unwrap_or_default()
    })
}

#[gpui::test]
fn m9_integ_ctrl_e_inside_a_mounted_share_unmounts_it(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, _lines) = open(recorder.clone(), cx);
    at(&shell, "/mnt/rclone/docs", vec![fuse()], cx);
    cx.simulate_keystrokes("ctrl-e");
    cx.run_until_parked();
    assert_eq!(
        releases(&recorder),
        [vec!["fusermount3", "-u", "--", "/mnt/rclone"]],
        "Ctrl+E inside the share releases the share's own identity"
    );
}

#[gpui::test]
fn m9_integ_ctrl_e_in_a_local_folder_never_touches_a_share(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, _lines) = open(recorder.clone(), cx);
    at(&shell, "/mnt/rclone-other/docs", vec![fuse()], cx);
    cx.simulate_keystrokes("ctrl-e");
    cx.run_until_parked();
    assert!(
        releases(&recorder).is_empty(),
        "a folder outside every share releases no share: {:?}",
        releases(&recorder)
    );
}

#[gpui::test]
fn m9_integ_ctrl_e_on_the_rail_share_row_unmounts_or_says_nothing_to_release(
    cx: &mut TestAppContext,
) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, _lines) = open(recorder.clone(), cx);
    for (row, unmounts) in [(fuse(), true), (share(), false)] {
        at(&shell, "/fixture/home", vec![row], cx);
        let producer = cx.update(|_, cx| shell.read(cx).rail.as_ref().unwrap().places.clone());
        cx.update(|_, cx| {
            producer.update(cx, |producer, cx| {
                let index = producer
                    .entries()
                    .iter()
                    .position(|entry| entry.group == Group::Network);
                producer.set_cursor(index.expect("the share is a drawn rail row"), cx);
            });
            shell.update(cx, |shell, cx| {
                shell.keyboard.context = flea::keys::Context::Rail;
                shell.release_device(false, cx);
            });
        });
        cx.run_until_parked();
        if unmounts {
            assert_eq!(
                releases(&recorder),
                [vec!["fusermount3", "-u", "--", "/mnt/rclone"]]
            );
        } else {
            assert_eq!(
                releases(&recorder).len(),
                1,
                "an unmounted share starts no release"
            );
            assert_eq!(status(&shell, cx), "NAS has nothing to eject or unmount");
        }
    }
}

#[gpui::test]
fn fx_eject2_a_transfer_on_the_share_refuses_its_unmount(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, _lines) = open(recorder.clone(), cx);
    let answer = |shell: &Entity<Shell>, line: &str, cx: &mut VisualTestContext| {
        shell.read_with(cx, |shell, _| {
            let backend = shell.panes[1].as_ref().unwrap().backend.as_ref().unwrap();
            if line.starts_with(r#"{"c""#) {
                backend.line(line.into()).unwrap();
            } else {
                backend.test_answered(line);
            }
        })
    };
    let copy =
        r#"{"c":"transfer","op":"copy","paths":["/fixture/home/big"],"dest":"/mnt/rclone/docs"}"#;
    answer(&shell, copy, cx);
    answer(
        &shell,
        r#"{"t":"transferstarted","id":12,"n":1,"moving":false}"#,
        cx,
    );
    at(&shell, "/mnt/rclone/docs", vec![fuse()], cx);
    cx.simulate_keystrokes("ctrl-e");
    cx.run_until_parked();
    assert!(releases(&recorder).is_empty(), "{:?}", releases(&recorder));
    assert_eq!(
        status(&shell, cx),
        "A copy on NAS is still running. Try again when it finishes."
    );
    answer(
        &shell,
        r#"{"t":"transferdone","id":12,"ok":1,"failed":0}"#,
        cx,
    );
    cx.simulate_keystrokes("ctrl-e");
    cx.run_until_parked();
    assert_eq!(
        releases(&recorder),
        [vec!["fusermount3", "-u", "--", "/mnt/rclone"]],
        "the copy has ended"
    );
}
