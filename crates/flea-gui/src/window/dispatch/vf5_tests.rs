use super::{
    preview_tests::*,
    test_shell::{land, set_cursor},
};
use crate::{
    backend::Backend,
    window::{startup::View, u07_tests::read},
};
use flea::{jsondoc::Json, ui_logic_preview_core::kinds::State};
use gpui::{
    point, px, Modifiers, MouseButton, MouseDownEvent, ScrollDelta, ScrollWheelEvent,
    TestAppContext,
};

pub(super) fn opened(
    cx: &mut TestAppContext,
) -> (
    gpui::Entity<crate::state::Shell>,
    &mut gpui::VisualTestContext,
    Lines,
) {
    let (shell, cx, requests) = super::preview_tests::opened(cx);
    shell.update(cx, |shell, _| {
        shell.launch.settings.data =
            flea::uistate::patched(&shell.launch.settings.data, &Json::Obj(Vec::new())).unwrap();
    });
    (shell, cx, requests)
}

#[gpui::test]
fn vf5_grid_ctrl_wheel_uses_production_route_and_persists(cx: &mut TestAppContext) {
    let (shell, cx, _requests) = opened(cx);
    land(
        &shell,
        cx,
        "/files",
        vec![file("notes.txt", "text-plain")],
        1,
    );
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.set_view(View::Grid, window, cx)));
    draw(cx);
    let bounds = shell.read_with(cx, |shell, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .grid
            .as_ref()
            .unwrap()
            .state
            .borrow()
            .bounds
    });
    cx.simulate_event(ScrollWheelEvent {
        position: bounds.center(),
        modifiers: Modifiers {
            control: true,
            ..Default::default()
        },
        delta: ScrollDelta::Lines(point(0., 3.)),
        ..Default::default()
    });
    draw(cx);
    shell.read_with(cx, |shell, _| {
        assert_eq!(
            shell.panes[0]
                .as_ref()
                .unwrap()
                .grid
                .as_ref()
                .unwrap()
                .view
                .thumb
                .name(),
            "large"
        );
        assert_eq!(
            shell
                .launch
                .settings
                .data
                .get("preview")
                .and_then(|v| v.get("thumbSize"))
                .and_then(Json::as_str),
            Some("large")
        );
    });
    close(&shell, cx);
}

#[gpui::test]
fn vf5_tab_secondary_click_opens_shared_menu(cx: &mut TestAppContext) {
    let (shell, cx, _requests) = opened(cx);
    land(
        &shell,
        cx,
        "/files",
        vec![file("notes.txt", "text-plain")],
        1,
    );
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.open_tab(0, "/files/Docs".into(), window, cx)
        })
    });
    land(
        &shell,
        cx,
        "/files/Docs",
        vec![file("notes.txt", "text-plain")],
        2,
    );
    draw(cx);
    let center = read(&shell, cx, "tabCentre", &["1"]);
    let values: Vec<f32> = center
        .split_whitespace()
        .map(|v| v.parse().unwrap())
        .collect();
    assert_eq!(values.len(), 2, "second tab is painted: {center}");
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Right,
        position: point(px(values[0]), px(values[1])),
        ..Default::default()
    });
    draw(cx);
    shell.read_with(cx, |shell, _| {
        assert!(
            shell.menu.menu.borrow().opened(),
            "tab opens the shared menu"
        )
    });
    cx.simulate_keystrokes("down enter");
    draw(cx);
    assert_eq!(
        read(&shell, cx, "tabCount", &[]),
        "3",
        "Duplicate Tab uses the existing tab model"
    );
    close(&shell, cx);
}

#[gpui::test]
fn vf5_binary_refusal_resizes_mounted_quick_look(cx: &mut TestAppContext) {
    let (shell, cx, mut requests) = opened(cx);
    land(
        &shell,
        cx,
        "/files",
        vec![file("blob.rs", "text-x-rust")],
        1,
    );
    set_cursor(&shell, cx, 0);
    cx.simulate_keystrokes("space");
    draw(cx);
    let asks = sent(&mut requests, "code");
    assert!(!asks.is_empty(), "binary starts through code reader");
    answer(
        &shell,
        cx,
        format!(
            r#"{{"t":"code","token":{},"slot":"look","state":"binary","first":1,"total":0,"lines":[]}}"#,
            token(&asks[0])
        ),
    );
    shell.read_with(cx, |shell, cx| {
        let subject = shell
            .quick_look
            .as_ref()
            .unwrap()
            .view
            .read(cx)
            .subject()
            .unwrap();
        assert_eq!(subject.shape.kind, State::Unsupported);
    });
    close(&shell, cx);
}

#[gpui::test]
fn vf5_model_fact_labels_take_the_3mf_board_column(cx: &mut TestAppContext) {
    let (shell, cx) = super::previewtypes_tests::open_at(cx, "/files", Some(14.));
    land(
        &shell,
        cx,
        "/files",
        vec![file("bracket.3mf", "application-octet-stream")],
        1,
    );
    use super::previewtypes_fixture_tests::{answer, ask, field, local, meta, pump};
    let mut lines = local(&shell, cx, "/files");
    meta(&shell, cx, &lines, "");
    lines.extend(pump(cx));
    let model = ask(&lines, "model3d");
    let slot = flea::jsondoc::parse(model).unwrap();
    let slot = slot
        .get("slot")
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_owned();
    let token = field(model, "token");
    answer(
        &shell,
        cx,
        &format!(
            r#"{{"t":"model3d","slot":"{slot}","token":{token},"state":"failed","reason":"test"}}"#
        ),
    );
    pump(cx);
    draw(cx);
    let fact = cx
        .debug_bounds("column-preview-fact-0")
        .expect("facts are mounted");
    let value = cx
        .debug_bounds("column-preview-value-0")
        .expect("fact value is mounted");
    assert_eq!(
        f32::from(value.left() - fact.left()),
        crate::g2::model_preview::parts::LABEL_COLUMN,
        "cols-preview-3mf.html: values start after the Layer height column"
    );
    close(&shell, cx);
}

#[gpui::test]
fn vf5_git_setting_requests_existing_background_producer(cx: &mut TestAppContext) {
    let (shell, cx, _requests) = opened(cx);
    let (backend, mut lines) = Backend::test_lines();
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend)
    });
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.change_settings(
                Json::Obj(vec![("showGitStatus".into(), Json::Bool(true))]),
                window,
                cx,
            )
        })
    });
    land(
        &shell,
        cx,
        "/files",
        vec![file("notes.txt", "text-plain")],
        1,
    );
    draw(cx);
    assert!(
        std::iter::from_fn(&mut lines).any(|line| line.contains("\"c\":\"gitstatus\"")),
        "enabled Git status reaches the existing worker"
    );
    close(&shell, cx);
}
