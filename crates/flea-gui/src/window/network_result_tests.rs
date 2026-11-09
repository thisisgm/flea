// The network result reader follows the same service the live rail and card use.
use super::*;
use crate::ipc::server::Event;
use flea::ipc::wire::{Reply, Request};

fn result(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Reply {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let (answer, receive) = std::sync::mpsc::sync_channel(1);
            shell.ipc(
                Event::Call {
                    request: Request::Call {
                        target: "flea".into(),
                        function: "networkResult".into(),
                        arguments: vec![],
                    },
                    answer,
                },
                window,
                cx,
            );
            receive.try_recv().unwrap().reply
        })
    })
}

#[gpui::test]
fn followup_network_result_tracks_idle_mount_resolve_and_success(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, _) = open(recorder.clone(), cx);
    assert_eq!(result(&shell, cx), Reply::value("idle"));
    press_share(&shell, 0, cx);
    assert_eq!(result(&shell, cx), Reply::value("mounting"));
    let mount = recorder.mounts()[0];
    recorder.answer(mount, 0, "");
    cx.run_until_parked();
    assert_eq!(result(&shell, cx), Reply::value("resolving"));
    let info = *recorder.find("info").last().unwrap();
    recorder.answer(info, 0, "local path: /fixture/mnt\n");
    cx.run_until_parked();
    assert_eq!(result(&shell, cx), Reply::value("mounted"));
}

#[gpui::test]
fn uish_misc_bare_server_result_finishes_when_share_listing_arrives(cx: &mut TestAppContext) {
    // A guest listing nothing is an invitation to sign in (#343), so it settles as a missing credential, not a failure.
    for (names, expected) in [("old-share\n", "mounted"), ("", "missing-credential")] {
        let recorder = Rc::new(Recorder::default());
        let (shell, cx, _) = open(recorder.clone(), cx);
        let producer = cx.update(|_, cx| shell.read(cx).rail.as_ref().unwrap().places.clone());
        cx.update(|_, cx| {
            shell.update(cx, |shell, cx| {
                let mut entry = share();
                entry.uri = "smb://shares-one.test/".into();
                shell.network_publish(Some(vec![entry]), cx);
            });
            producer.update(cx, |producer, cx| {
                let index = producer
                    .entries()
                    .iter()
                    .position(|entry| entry.group == Group::Network)
                    .unwrap();
                producer.side = 0;
                producer.activate(index, cx);
            });
        });
        frame(cx);
        let mount = recorder.mounts()[0];
        recorder.answer(mount, 0, "");
        cx.run_until_parked();
        let info = *recorder.find("info").last().unwrap();
        recorder.answer(info, 1, "");
        cx.run_until_parked();
        let listing = *recorder.find("list").last().unwrap();
        assert_eq!(
            recorder.jobs.borrow()[listing].argv,
            ["gio", "list", "--", "smb://shares-one.test/"]
        );
        assert_eq!(result(&shell, cx), Reply::value("resolving"));
        recorder.answer(listing, 0, names);
        cx.run_until_parked();
        assert_eq!(result(&shell, cx), Reply::value(expected));
        cx.update(|_, cx| {
            let browser = &shell
                .read(cx)
                .rail
                .as_ref()
                .unwrap()
                .network
                .as_ref()
                .unwrap()
                .shares
                .read(cx)
                .model;
            assert_eq!(browser.open, !names.is_empty());
            if !names.is_empty() {
                assert_eq!(browser.names, ["old-share"]);
                assert_eq!(browser.uri, "smb://shares-one.test/");
            }
        });
    }
}

#[gpui::test]
fn followup_network_result_keeps_cancelled_and_missing_credential(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, _) = open(recorder, cx);
    let service = cx.update(|_, cx| {
        shell
            .read(cx)
            .rail
            .as_ref()
            .unwrap()
            .network
            .as_ref()
            .unwrap()
            .service
            .clone()
    });
    cx.update(|_, cx| {
        service.update(cx, |service, cx| {
            service.open_share(
                crate::g2::network_shares::mount::Request {
                    id: 7,
                    uri: "sftp://user@host/".into(),
                    label: "NAS".into(),
                    origin: 0,
                    mounted: false,
                    authenticated: true,
                    password: Secret::default(),
                },
                0,
                cx,
            );
        })
    });
    assert_eq!(result(&shell, cx), Reply::value("missing-credential"));
    cx.update(|_, cx| {
        service.update(cx, |service, cx| {
            service.open_share(
                crate::g2::network_shares::mount::Request {
                    id: 8,
                    uri: "sftp://host/".into(),
                    label: "NAS".into(),
                    origin: 0,
                    mounted: false,
                    authenticated: false,
                    password: Secret::default(),
                },
                0,
                cx,
            );
            service.cancel(999, 0, cx);
        })
    });
    assert_eq!(
        result(&shell, cx),
        Reply::value("mounting"),
        "a stale cancel changes no state"
    );
    cx.update(|_, cx| service.update(cx, |service, cx| service.cancel(8, 0, cx)));
    assert_eq!(result(&shell, cx), Reply::value("cancelled"));
}

#[gpui::test]
fn followup_network_result_direct_open_preserves_the_last_mount_result(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, mut lines) = open(recorder.clone(), cx);
    let mut entry = share();
    entry.path = "/fixture/already-mounted".into();
    entry.mounted = true;
    drain(&mut lines[0]);
    let producer = cx.update(|_, cx| shell.read(cx).rail.as_ref().unwrap().places.clone());
    cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            shell.network_publish(Some(vec![entry.clone()]), cx);
            let service = shell
                .rail
                .as_ref()
                .unwrap()
                .network
                .as_ref()
                .unwrap()
                .service
                .clone();
            service.update(cx, |service, _| {
                service.result = crate::g2::network_shares::result::ResultState::Failed
            });
        });
        producer.update(cx, |producer, cx| {
            let index = producer
                .entries()
                .iter()
                .position(|row| row.group == Group::Network && row.path == entry.path)
                .expect("the published mounted row");
            producer.side = 0;
            producer.activate(index, cx);
        });
    });
    frame(cx);
    assert_eq!(
        result(&shell, cx),
        Reply::value("failed"),
        "opening an existing path does not invent a mount result"
    );
    assert!(
        recorder.mounts().is_empty(),
        "an existing mount starts no gio mount"
    );
    let sent = drain(&mut lines[0]);
    assert!(sent.iter().any(|line| line.contains(r#""c":"list""#) && line.contains("/fixture/already-mounted")), "direct activation must still navigate: {sent:?}");
}
