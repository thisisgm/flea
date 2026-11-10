//! FX-EJECT3: menuDialogState answers the drawn Copy To, Move To and New File card, as v0.3.12's menu action card did.
use super::*;

fn control(state: &Json, index: usize) -> &Json {
    state
        .get("controls")
        .and_then(Json::as_array)
        .and_then(|controls| controls.get(index))
        .unwrap_or_else(|| panic!("control {index}: {state:?}"))
}

#[gpui::test]
fn fx_eject3_menu_dialog_state_answers_the_drawn_copy_to_card(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", vec![row("notes.txt", false)], 1);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.open_overlay_copy_to(false, "/files/out", window, cx)
        })
    });
    draw(cx);
    let state = json(&shell, cx, "menuDialogState");
    assert_eq!(state.get("opened"), Some(&Json::Bool(true)), "{state:?}");
    assert_eq!(text(&state, &["action"]), Some("copyTo"));
    assert_eq!(text(&state, &["field"]), Some("/files/out"));
    assert_eq!(state.get("busy"), Some(&Json::Bool(false)));
    assert!(
        !text(&state, &["rect"]).unwrap_or_default().is_empty(),
        "{state:?}"
    );
    let submit = control(&state, 1);
    assert_eq!(text(submit, &["name"]), Some("Submit"));
    assert_eq!(text(submit, &["label"]), Some("Copy"));
    assert_eq!(submit.get("visible"), Some(&Json::Bool(true)), "{submit:?}");
    assert_eq!(submit.get("enabled"), Some(&Json::Bool(true)));
    assert_eq!(text(control(&state, 0), &["name"]), Some("Cancel"));
    assert_eq!(
        control(&state, 2).get("focused"),
        Some(&Json::Bool(true)),
        "the field opens focused"
    );
    let keys = |index: usize| -> Vec<String> {
        let pairs = control(&state, index).as_object().unwrap_or_default();
        pairs.iter().map(|(key, _)| key.clone()).collect()
    };
    let field = keys(2);
    assert_eq!(field, ["name", "focused"], "{state:?}");
    let omitted: Vec<String> = keys(0)
        .into_iter()
        .filter(|key| !field.contains(key))
        .collect();
    assert_eq!(
        omitted,
        ["label", "visible", "enabled", "hovered", "pressed", "centre", "rect"],
        "{state:?}"
    );
    let unavailable = text(&state, &["unavailable"]).unwrap_or_default();
    let declared: Vec<&str> = unavailable.split(", ").collect();
    for key in &omitted {
        assert!(
            declared.contains(&format!("controls[2].{key}").as_str()),
            "the field omits {key} without saying so: {unavailable}"
        );
    }
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.close_overlay(window, cx)));
    draw(cx);
    assert_eq!(
        read(&shell, cx, "menuDialogState", &[]),
        "{\"opened\":false}"
    );
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.open_overlay_copy_to(true, "", window, cx)
        })
    });
    draw(cx);
    let state = json(&shell, cx, "menuDialogState");
    assert_eq!(text(&state, &["action"]), Some("moveTo"), "{state:?}");
    assert_eq!(state.get("busy"), Some(&Json::Bool(false)));
    assert_eq!(
        control(&state, 1).get("visible"),
        Some(&Json::Bool(true)),
        "{state:?}"
    );
    assert_eq!(
        control(&state, 1).get("enabled"),
        Some(&Json::Bool(false)),
        "an empty destination cannot be submitted"
    );
}

#[gpui::test]
fn fx_eject3_menu_dialog_state_answers_the_drawn_new_file_card(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", vec![row("notes.txt", false)], 1);
    let view = cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let view = shell.operation_overlay(0, window, cx).unwrap();
            view.update(cx, |overlay, cx| overlay.new_file(7, "/files", cx));
            shell.show_operation(&view, cx);
            view
        })
    });
    draw(cx);
    let state = json(&shell, cx, "menuDialogState");
    assert_eq!(text(&state, &["action"]), Some("newFile"), "{state:?}");
    assert!(
        !text(&state, &["field"]).unwrap_or_default().is_empty(),
        "the card opens on its default name: {state:?}"
    );
    let cancel = control(&state, 0);
    assert!(
        !text(cancel, &["label"]).unwrap_or_default().is_empty(),
        "{cancel:?}"
    );
    assert_eq!(cancel.get("visible"), Some(&Json::Bool(true)), "{cancel:?}");
    let submit = control(&state, 1);
    assert_eq!(text(submit, &["label"]), Some("Create"), "{submit:?}");
    assert_eq!(submit.get("enabled"), Some(&Json::Bool(true)));
    cx.update(|_, cx| {
        view.update(cx, |overlay, cx| {
            if let Some(crate::g2::file_ops::model::Dialog::NewFile { busy, .. }) =
                &mut overlay.model.dialog
            {
                *busy = true;
            }
            cx.notify();
        })
    });
    draw(cx);
    let state = json(&shell, cx, "menuDialogState");
    assert_eq!(state.get("busy"), Some(&Json::Bool(true)), "{state:?}");
    let submit = control(&state, 1);
    assert_eq!(submit.get("visible"), Some(&Json::Bool(true)), "{submit:?}");
    assert_eq!(
        submit.get("enabled"),
        Some(&Json::Bool(false)),
        "a create in flight cannot be sent again"
    );
}
