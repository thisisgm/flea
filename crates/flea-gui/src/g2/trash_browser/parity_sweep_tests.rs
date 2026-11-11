//! A mounted window's delayed sweep uses a scripted transport, never the user's Trash.
use super::join_tests::{root, shell};
use crate::{backend::Event, list::bench::real_cx};
use flea::{jsondoc::Json, ui_logic_trash_browser::dates};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

fn browse(lines: &mut impl FnMut() -> Option<String>) -> Json {
    loop {
        let request =
            flea::jsondoc::parse(&lines().expect("the live backend receives a Trash request"))
                .unwrap();
        if request.get("c").and_then(Json::as_str) == Some("trashbrowse") {
            return request;
        }
    }
}

#[test]
fn pb_trash_delayed_sweep_honours_opt_in_day_guard_and_open_view() {
    for (enabled, today, opened) in [
        (false, false, false),
        (true, true, false),
        (true, false, true),
        (true, false, false),
    ] {
        let mut cx = real_cx();
        let window = root(&mut cx);
        shell(&mut cx, window, |shell, window, cx| {
            shell.launch.settings.data = flea::jsondoc::parse(&format!(
                r#"{{"trashAutoEmpty":{enabled},"trashSweptOn":{}}}"#,
                if today { dates::day_number(now()) } else { 0 }
            ))
            .unwrap();
            shell.mount_trash(window, cx);
            shell
                .trash
                .as_ref()
                .unwrap()
                .pane
                .update(cx, |pane, _| pane.monitor_start = None);
            if opened {
                shell.trash_action("openTrash", window, cx);
            }
        });
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(1999));
        cx.run_until_parked();
        shell(&mut cx, window, |shell, _, cx| {
            assert!(!shell.trash.as_ref().unwrap().pane.read(cx).host.sweeping())
        });
        cx.executor().advance_clock(Duration::from_millis(1));
        cx.run_until_parked();
        shell(&mut cx, window, |shell, _, cx| {
            assert_eq!(
                shell.trash.as_ref().unwrap().pane.read(cx).host.sweeping(),
                enabled && !today && !opened
            )
        });
    }
}

#[test]
fn pb_trash_sweep_records_only_success_and_open_cancels_it() {
    for (failed, interrupt) in [(false, false), (true, false), (false, true)] {
        let mut cx = real_cx();
        let window = root(&mut cx);
        shell(&mut cx, window, |shell, window, cx| {
            shell.launch.settings.data =
                flea::jsondoc::parse(r#"{"trashAutoEmpty":true,"trashSweptOn":0}"#).unwrap();
            shell.mount_trash(window, cx);
            shell
                .trash
                .as_ref()
                .unwrap()
                .pane
                .update(cx, |pane, _| pane.monitor_start = None);
        });
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_secs(2));
        cx.run_until_parked();
        shell(&mut cx, window, |shell, window, cx| {
            assert!(shell.trash.as_ref().unwrap().pane.read(cx).host.sweeping());
            if interrupt {
                shell.trash_action("openTrash", window, cx);
            }
            shell.receive(
                0,
                Event::TrashLine(format!(
                    r#"{{"t":"trashbrowse","op":"list","id":1,"ok":{},"rows":[],"total":0}}"#,
                    !failed
                )),
                window,
                cx,
            );
            shell.flush_trash(window, cx);
            assert!(!shell.trash.as_ref().unwrap().pane.read(cx).host.sweeping());
            let recorded = shell
                .launch
                .settings
                .data
                .get("trashSweptOn")
                .and_then(Json::as_f64)
                .unwrap_or(0.) as i64;
            assert_eq!(
                recorded,
                if failed || interrupt {
                    0
                } else {
                    dates::day_number(now())
                }
            );
            if interrupt {
                assert_eq!(shell.trash.as_ref().unwrap().pane.read(cx).view().total, 0);
            }
        });
    }
}

#[test]
fn pb_trash_sweep_deletes_only_reviewed_expired_identities() {
    use crate::{backend::Backend, state::tests::shell_with, window::root::Root};
    use gpui::AppContext;
    let mut cx = real_cx();
    let (backend, mut lines) = Backend::test_lines();
    let window = cx.add_window(|_, cx| Root {
        shell: cx.new(|cx| shell_with(backend, cx)),
    });
    shell(&mut cx, window, |shell, window, cx| {
        shell.launch.settings.data =
            flea::jsondoc::parse(r#"{"trashAutoEmpty":true,"trashSweptOn":0}"#).unwrap();
        shell.mount_trash(window, cx);
    });
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    let request = browse(&mut lines);
    assert_eq!(request.get("op").and_then(Json::as_str), Some("list"));
    assert_eq!(request.get("recover").and_then(Json::as_bool), Some(false));
    assert_eq!(request.get("count").and_then(Json::as_f64), Some(350.));
    let mut id = request.get("id").and_then(Json::as_f64).unwrap() as u64;
    let rows = r#"[{"uri":"trash:///old","original":"/fixture/old","identity":"old-id","deleted":"2000-01-01T00:00:00Z"},{"uri":"trash:///future","original":"/fixture/future","identity":"future-id","deleted":"2999-01-01T00:00:00Z"},{"uri":"trash:///unknown","original":"/fixture/unknown","identity":"unknown-id"}]"#;
    shell(&mut cx, window, |shell, window, cx| {
        shell.receive(
            0,
            Event::TrashLine(format!(
                r#"{{"t":"trashbrowse","op":"list","id":{id},"ok":true,"rows":{rows},"total":3}}"#
            )),
            window,
            cx,
        );
        shell.flush_trash(window, cx);
    });
    let prepare = browse(&mut lines);
    assert_eq!(prepare.get("op").and_then(Json::as_str), Some("prepare"));
    assert_eq!(
        prepare.get("uris"),
        Some(&Json::Arr(vec![Json::Str("trash:///old".into())]))
    );
    assert_eq!(
        prepare.get("identities"),
        Some(&Json::Arr(vec![Json::Str("old-id".into())]))
    );
    assert_eq!(prepare.get("all").and_then(Json::as_bool), Some(false));
    id = prepare.get("id").and_then(Json::as_f64).unwrap() as u64;
    shell(&mut cx, window, |shell, window, cx| {
        shell.receive(
            0,
            Event::TrashLine(format!(
                r#"{{"t":"trashbrowse","op":"prepare","id":{id},"ok":true,"count":1,"token":47}}"#
            )),
            window,
            cx,
        );
        shell.flush_trash(window, cx);
        assert_eq!(
            shell
                .launch
                .settings
                .data
                .get("trashSweptOn")
                .and_then(Json::as_f64),
            Some(0.)
        );
    });
    let delete = browse(&mut lines);
    assert_eq!(delete.get("op").and_then(Json::as_str), Some("delete"));
    assert_eq!(delete.get("token").and_then(Json::as_f64), Some(47.));
    id = delete.get("id").and_then(Json::as_f64).unwrap() as u64;
    shell(&mut cx, window, |shell, window, cx| {
        shell.receive(
            0,
            Event::TrashLine(format!(
                r#"{{"t":"trashbrowse","op":"delete","id":{id},"ok":true,"done":1,"failed":0}}"#
            )),
            window,
            cx,
        );
        shell.flush_trash(window, cx);
        assert_eq!(
            shell
                .launch
                .settings
                .data
                .get("trashSweptOn")
                .and_then(Json::as_f64),
            Some(dates::day_number(now()) as f64)
        );
    });
    assert!(
        std::iter::from_fn(&mut lines).all(|line| flea::jsondoc::parse(&line)
            .unwrap()
            .get("c")
            .and_then(Json::as_str)
            != Some("trashbrowse"))
    );
}
