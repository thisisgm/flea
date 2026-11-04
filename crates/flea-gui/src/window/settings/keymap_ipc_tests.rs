//! The keyboard sheet's readers, keymapQuery and keymapCardRect, through the live dispatch.
use crate::{backend::Backend, state::Shell};
use gpui::TestAppContext;

fn shell(backend: Backend, cx: &mut gpui::Context<Shell>) -> Shell {
    let mut shell = crate::state::tests::shell_with(backend, cx);
    shell.launch.settings.data = flea::uischema::defaults();
    shell
}

fn ipc_value(
    shell: &mut Shell,
    name: &str,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<Shell>,
) -> String {
    let (answer, receive) = std::sync::mpsc::sync_channel(1);
    let request = flea::ipc::wire::Request::Call {
        target: "flea".into(),
        function: name.into(),
        arguments: vec![],
    };
    shell.ipc(
        crate::ipc::server::Event::Call { request, answer },
        window,
        cx,
    );
    let reply = receive.try_recv().unwrap().reply;
    assert_eq!(reply.code, 0, "{name}: {}", reply.stderr);
    reply.stdout.trim().to_owned()
}

#[gpui::test]
fn keymap_query_and_card_rect_read_the_open_sheet(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (entity, cx) = cx.add_window_view(|window, cx| {
        let shell = shell(backend, cx);
        window.focus(&shell.focus, cx);
        shell
    });
    entity.update_in(cx, |shell, window, cx| {
        assert_eq!(ipc_value(shell, "keymapQuery", window, cx), "");
        assert_eq!(ipc_value(shell, "keymapCardRect", window, cx), "");
        shell.open_sheet(window, cx);
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("c o p y");
    cx.run_until_parked();
    let card = cx.debug_bounds("sheet-card").expect("the sheet card drew");
    entity.update_in(cx, |shell, window, cx| {
        assert_eq!(ipc_value(shell, "keymapQuery", window, cx), "copy");
        let expected = crate::ipc::rows::rect_of(
            card.origin.x.into(),
            card.origin.y.into(),
            card.size.width.into(),
            card.size.height.into(),
        );
        assert_eq!(ipc_value(shell, "keymapCardRect", window, cx), expected);
    });
}
