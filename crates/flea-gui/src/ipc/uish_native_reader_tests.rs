//! Native failures are pinned through the same window readers the shell harness uses.
use super::*;
use flea::{
    jsondoc::{parse, Json},
    ui_logic_file_ops::{confirm, trash},
};

fn frame(cx: &mut TestAppContext, handle: WindowHandle<Root>) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
}

fn state(cx: &mut TestAppContext, handle: WindowHandle<Root>, name: &str) -> Json {
    let reply = read(cx, handle, name);
    assert_eq!(reply.code, 0, "{reply:?}");
    // Sample reader document: {"opened":true,"confirmation":{"count":1}}.
    parse(&reply.stdout).unwrap()
}

#[gpui::test]
fn uish_misc_native_delete_reader_names_the_drawn_card_and_buttons(cx: &mut TestAppContext) {
    use crate::g2::file_ops::{host::Overlay, palette::CardPalette};
    let (handle, _backend) = open(cx);
    update(cx, handle, |shell, window, cx| {
        let palette = CardPalette::from_theme(&window.text_style(), Default::default(), 8.);
        let catalog = shell.locale.catalog.clone();
        shell.overlay = Some(cx.new(|cx| {
            let mut overlay = Overlay::new(catalog, palette, "default");
            overlay.confirm_delete(
                trash::Review {
                    count: Some(1),
                    token: "73".into(),
                    ..Default::default()
                },
                trash::Scope::Items,
                confirm::Source::Menu(4),
                cx,
            );
            overlay
        }));
    });
    frame(cx, handle);
    let s = state(cx, handle, "menuDialogState");
    let c = s.get("confirmation").expect("the Delete card is open");
    assert_eq!(c.get("opened").and_then(Json::as_bool), Some(true));
    assert_eq!(c.get("count").and_then(Json::as_f64), Some(1.));
    assert_eq!(c.get("token").and_then(Json::as_f64), Some(73.));
    let expected = {
        let mut visual = gpui::VisualTestContext::from_window(handle.into(), cx);
        let b = visual.debug_bounds("op1-card").unwrap();
        crate::ipc::rows::rect_of(
            f32::from(b.origin.x),
            f32::from(b.origin.y),
            f32::from(b.size.width),
            f32::from(b.size.height),
        )
    };
    // Sample painted rect: "752 455 416 171"; debug bounds snap fractional edges to device pixels.
    let rect = |text: &str| {
        text.split_whitespace()
            .map(|n| n.parse::<f32>().unwrap())
            .collect::<Vec<_>>()
    };
    let actual = rect(c.get("rect").and_then(Json::as_str).unwrap());
    let expected = rect(&expected);
    assert_eq!(actual.len(), 4);
    for (actual, expected) in actual.iter().zip(&expected) {
        assert!(
            (actual - expected).abs() <= 1.,
            "Delete frame omits a painted edge: {actual} vs {expected}"
        );
    }
    let danger = c.get("danger").unwrap();
    assert_eq!(danger.get("label").and_then(Json::as_str), Some("Delete"));
    assert_eq!(danger.get("enabled").and_then(Json::as_bool), Some(true));
    assert!(!danger
        .get("centre")
        .and_then(Json::as_str)
        .unwrap()
        .is_empty());
    update(cx, handle, |shell, _, cx| {
        shell.overlay.as_ref().unwrap().clone().update(cx, |o, cx| {
            o.model.dialog = None;
            cx.notify();
        })
    });
    frame(cx, handle);
    value(cx, handle, "menuDialogState", r#"{"opened":false}"#);
}

#[gpui::test]
fn uish_misc_native_delete_reader_empty_token_has_the_qt_zero_sentinel(cx: &mut TestAppContext) {
    let (handle, _backend) = open(cx);
    update(cx, handle, |shell, window, cx| {
        shell.confirm_operation(
            0,
            trash::Review::default(),
            trash::Scope::Items,
            confirm::Source::Menu(4),
            window,
            cx,
        );
        shell.overlay.as_ref().unwrap().clone().update(cx, |o, cx| {
            let Some(crate::g2::file_ops::model::Dialog::Delete { busy, .. }) =
                o.model.dialog.as_mut()
            else {
                panic!("Delete is open")
            };
            *busy = true;
            cx.notify();
        });
    });
    frame(cx, handle);
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(crate::pane::swap::HOLD_MS));
    cx.run_until_parked();
    frame(cx, handle);
    let s = state(cx, handle, "menuDialogState");
    assert_eq!(s.get("busy").and_then(Json::as_bool), Some(true));
    let c = s.get("confirmation").unwrap();
    assert_eq!(c.get("opened").and_then(Json::as_bool), Some(true));
    assert_eq!(c.get("token").and_then(Json::as_f64), Some(0.));
}

#[gpui::test]
fn uish_misc_native_extract_reader_observes_the_drawn_source_line(cx: &mut TestAppContext) {
    use crate::g2::file_ops::{host::Overlay, palette::CardPalette};
    let (handle, _backend) = open(cx);
    update(cx, handle, |shell, window, cx| {
        let palette = CardPalette::from_theme(&window.text_style(), Default::default(), 8.);
        let catalog = shell.locale.catalog.clone();
        shell.overlay = Some(cx.new(|_| {
            let mut overlay = Overlay::new(catalog.clone(), palette, "default");
            overlay.model.line(&catalog, r#"{"t":"job","job":{"id":73,"kind":"extract","head":"Extracting","file":"slow.zip","done":0,"total":0,"bytes":0,"totalBytes":0,"itemBytes":0,"rate":0,"left":null,"state":"running","reason":"","output":[],"undo":false,"cancelled":false},"unfinished":1}"#, 0);
            overlay
        }));
        shell.footer(cx);
    });
    frame(cx, handle);
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(crate::pane::swap::HOLD_MS));
    cx.run_until_parked();
    frame(cx, handle);
    let s = state(cx, handle, "statusActivityState");
    let c = s.get("transferCard").unwrap();
    assert_eq!(c.get("visible").and_then(Json::as_bool), Some(true));
    assert_eq!(c.get("headline").and_then(Json::as_str), Some("Extracting"));
    assert_eq!(c.get("sourceLine").and_then(Json::as_str), Some("slow.zip"));
    assert_eq!(c.get("byteLine").and_then(Json::as_str), Some(""));
    assert_eq!(
        c.get("cancel")
            .unwrap()
            .get("enabled")
            .and_then(Json::as_bool),
        Some(true)
    );
}
