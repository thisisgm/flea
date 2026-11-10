//! A timed-out listing must retire before a later quiet answer can clear its cached rows.
use super::*;
use crate::g2::device_rail::bfx_net_tests::Root;
use std::time::Duration;

#[gpui::test]
fn network_unchanged_gio_poll_reloads_saved_places(cx: &mut TestAppContext) {
    let root = Root::new();
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, _) = open(recorder.clone(), cx);
    let bookmarks = root.0.join("bookmarks");
    std::fs::write(&bookmarks, "smb://fixture/share/ Before\n").unwrap();
    shell.update(cx, |shell, _| {
        shell.network().unwrap().bookmarks = bookmarks.clone();
    });
    let text = "Mount(0): Fixture -> smb://fixture/share/\n";
    recorder.answer(*recorder.find("-li").last().unwrap(), 0, text);
    cx.run_until_parked();
    std::fs::write(&bookmarks, "smb://fixture/share/ After\n").unwrap();
    cx.executor().advance_clock(Duration::from_millis(5_000));
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        assert!(
            shell
                .rail
                .as_ref()
                .unwrap()
                .network
                .as_ref()
                .unwrap()
                .listing
                .running
        );
    });
    recorder.answer(*recorder.find("-li").last().unwrap(), 0, text);
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        let network = shell.rail.as_ref().unwrap().network.as_ref().unwrap();
        assert!(!network.listing.running);
        assert!(network.rows.iter().any(|row| row.label == "After"));
    });
}

#[gpui::test]
fn bfx_net_quiet_listing_after_timeout_replaces_the_previous_mount_response(
    cx: &mut TestAppContext,
) {
    let root = Root::new();
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, _) = open(recorder.clone(), cx);
    std::fs::write(root.0.join("mountinfo"), "").unwrap();
    shell.update(cx, |shell, cx| {
        let places = shell.rail.as_ref().unwrap().places.clone();
        places.update(cx, |places, _| places.polling = true);
        let network = shell.rail.as_mut().unwrap().network.as_mut().unwrap();
        network.bookmarks = root.0.join("bookmarks");
        network.sources.mountinfo = root.0.join("mountinfo");
        if !network.listing.running {
            shell.network_list(cx);
        }
    });
    let first = *recorder
        .find("-li")
        .last()
        .expect("the rail starts its listing");
    let previous = "Mount(0): Fixture -> smb://fixture/share/\n";
    recorder.answer(first, 0, previous);
    cx.run_until_parked();
    shell.update(cx, |shell, cx| {
        let network = shell.network().unwrap();
        assert_eq!(network.listing.text, previous);
        assert_eq!(network.rows.len(), 1);
        shell.network_list(cx);
    });
    let hung = *recorder.find("-li").last().unwrap();
    assert_ne!(first, hung);
    cx.executor().advance_clock(Duration::from_millis(10_001));
    cx.run_until_parked();
    shell.update(cx, |shell, cx| shell.network_sync_mounts(cx));
    shell.update(cx, |shell, _| {
        let network = shell.network().unwrap();
        assert!(network.listing.timed_out());
    });
    recorder.answer(hung, 0, "partial timed-out response");
    cx.run_until_parked();
    shell.update(cx, |shell, cx| {
        let network = shell.network().unwrap();
        assert_eq!(network.listing.text, previous);
        assert_eq!(network.rows.len(), 1);
        if !network.listing.running {
            shell.network_list(cx);
        }
    });
    let quiet = *recorder.find("-li").last().unwrap();
    assert_ne!(quiet, hung, "the timed-out slot must admit the next poll");
    recorder.answer(quiet, 0, "");
    cx.run_until_parked();
    shell.update(cx, |shell, _| {
        let network = shell.network().unwrap();
        assert!(network.listing.text.is_empty());
        assert!(!network.listing.running);
        assert!(network.rows.is_empty());
    });
}
