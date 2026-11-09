//! A quiet native provider command must not reuse an earlier Taildrop response.
use crate::backend::Backend;
use crate::g2::device_rail::bfx_net_tests::Root;
use crate::g2::send_share::seams::Wire;
use crate::state::tests::shell_with;
use flea::ui_logic_send_share::providers::{Facts, Role};
use std::time::{Duration, Instant};

struct NoBackend;
impl Wire for NoBackend {
    fn send(&self, _: String) -> bool {
        false
    }
}

#[gpui::test]
fn bfx_net_quiet_taildrop_status_drops_previous_peers_and_response(cx: &mut gpui::TestAppContext) {
    let root = Root::new();
    let status = r#"{"BackendState":"Running","Self":{"UserID":1,"Capabilities":["https://tailscale.com/cap/file-sharing"]},"Peer":{"nodekey:fixture":{"HostName":"fixture","DNSName":"fixture.ts.net.","Online":true,"TaildropTarget":1,"UserID":1}}}"#;
    let script = root.script(
        "tailscale",
        &format!(
            "#!/bin/sh\nprintf 'run\\n' >> \"$0.runs\"\nif [ ! -f \"$0.ran\" ]; then\n  touch \"$0.ran\"\n  printf '%s' '{status}'\n  printf previous-status-diagnostic >&2\nfi\n"
        ),
    );
    let facts = Facts {
        tailscale_command: script,
        send_command: "/fixture/send".into(),
        ..Facts::default()
    };
    let (backend, _lines) = Backend::test_lines();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    for quiet in [false, true] {
        shell.update_in(cx, |shell, window, cx| {
            let actions =
                shell.menu.providers.sides[0].menu_opened(&NoBackend, &facts, None, &[], 0);
            shell.provider_actions(0, actions, window, cx);
        });
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(3) {
            cx.run_until_parked();
            if !shell.read_with(cx, |shell, _| {
                shell.menu.providers.sides[0].taildrop_refreshing()
            }) {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        shell.update_in(cx, |shell, _, _| {
            let sender = &mut shell.menu.providers.sides[0];
            assert!(!sender.taildrop_refreshing(), "real helper must finish");
            assert_eq!(sender.peers_for(Role::Tailscale).is_empty(), quiet);
            assert_eq!(sender.taildrop_code(), if quiet { "invalid" } else { "" });
            sender.menu_closed();
        });
    }
    assert_eq!(
        std::fs::read_to_string(root.0.join("tailscale.runs")).unwrap(),
        "run\nrun\n"
    );
}
