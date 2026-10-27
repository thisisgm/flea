use crate::{backend::Backend, state::Shell};
use flea::{jsondoc::Json, ui_logic_st1::pages};
use gpui::TestAppContext;

fn assert_display(
    shell: &mut Shell,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<Shell>,
    size: u32,
) {
    let view = shell.settings.panel.as_ref().unwrap().read(cx);
    assert_eq!(
        view.panel.facts.base_size, 14,
        "Omarchy's size stays unchanged"
    );
    assert_eq!(f32::from(view.typeface.body), size as f32);
    let ruler = view
        .panel
        .rows
        .iter()
        .find(|row| row.id == "textStop")
        .unwrap();
    assert_eq!(ruler.stops[ruler.index], size);
    let (answer, receive) = std::sync::mpsc::sync_channel(1);
    shell.ipc(
        crate::ipc::server::Event::Call {
            request: flea::ipc::wire::Request::Call {
                target: "flea".into(),
                function: "settingsRows".into(),
                arguments: vec![],
            },
            answer,
        },
        window,
        cx,
    );
    let reply = receive.try_recv().unwrap().reply;
    assert_eq!(reply.code, 0);
    assert!(
        reply
            .stdout
            .lines()
            .any(|line| line == format!("fact|Effective|{size} px")),
        "{}",
        reply.stdout
    );
}

#[gpui::test]
fn ey162_display_ruler_keys_report_the_effective_override(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (entity, cx) = cx.add_window_view(|window, cx| {
        let mut shell = crate::state::tests::shell_with(backend, cx);
        shell.launch.settings.data = flea::uischema::defaults();
        shell.open_settings(window, cx);
        shell
            .settings
            .panel
            .as_ref()
            .unwrap()
            .update(cx, |view, cx| {
                view.panel.show_section("display");
                cx.notify();
            });
        shell
    });
    cx.simulate_keystrokes("right down right");
    entity.update_in(cx, |shell, window, cx| {
        assert_eq!(
            pages::at(
                &shell.launch.settings.data,
                &["display", "textSize", "mode"]
            )
            .and_then(Json::as_f64),
            Some(16.)
        );
        assert_display(shell, window, cx, 16);
    });
    cx.simulate_keystrokes("left left");
    entity.update_in(cx, |shell, window, cx| {
        assert_display(shell, window, cx, 12)
    });
    cx.simulate_keystrokes("up right");
    entity.update_in(cx, |shell, window, cx| {
        assert_eq!(
            pages::at(
                &shell.launch.settings.data,
                &["display", "textSize", "mode"]
            )
            .and_then(Json::as_str),
            Some("system")
        );
        assert_display(shell, window, cx, 14);
    });
    cx.simulate_keystrokes("down right");
    entity.update_in(cx, |shell, window, cx| {
        let panel = &shell.settings.panel.as_ref().unwrap().read(cx).panel;
        assert_eq!(panel.rows[panel.cursor].id, "display.hyprlandIcons");
        assert_display(shell, window, cx, 14);
    });
}
