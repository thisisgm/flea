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
                crate::g2::pl2::mount::Request {
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
                crate::g2::pl2::mount::Request {
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
