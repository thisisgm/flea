//! An installed provider must finish its fresh status before the cached peer can be chosen.
use super::*;
use crate::g2::pl1::bfx_net_tests::Root;
use std::time::{Duration, Instant};

#[gpui::test]
fn uish_misc_provider_choice_waits_for_fresh_status_of_an_installed_peer(cx: &mut TestAppContext) {
    for remains in [false, true] {
        let root = Root::new();
        let status = if remains {
            READY
        } else {
            r#"{"BackendState":"Running","Self":{"Capabilities":["https://tailscale.com/cap/file-sharing"]},"Peer":{}}"#
        };
        let command = root.script(
            "tailscale",
            &format!("#!/bin/sh\ntouch \"$0.started\"\nprintf '%s' '{status}'\n"),
        );
        let (shell, cx, mut lines, id) = warm(cx);
        choose(&shell, cx);
        let asked = drain(&mut lines);
        assert_eq!(with(&asked, "\"c\":\"formats\"").len(), 1);
        feed(
            &shell,
            cx,
            0,
            &format!(
                r#"{{"t":"formats","id":{id},"archive":[],"providers":{{"taildrop":{{"installed":true,"command":"{command}"}},"taildropSend":{{"installed":true,"command":"/fixture/send"}}}}}}"#
            ),
        );
        assert!(shell.read_with(cx, |shell, _| shell.menu.providers.sides[0]
            .row_on(Role::Tailscale)));
        let held = super::state_tests::state(&shell, cx);
        assert_eq!(
            held.get("taildrop")
                .unwrap()
                .get("checking")
                .and_then(Json::as_bool),
            Some(true)
        );
        assert!(
            pending(&shell, cx),
            "installed provider status must keep activation pending"
        );
        assert!(with(&drain(&mut lines), "\"op\":\"validate\"").is_empty());

        let deadline = Instant::now() + Duration::from_secs(3);
        while shell.read_with(cx, |shell, _| shell.menu.providers.sides[0].td_checking) {
            cx.run_until_parked();
            assert!(
                Instant::now() < deadline,
                "fresh status helper did not finish"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(root.0.join("tailscale.started").is_file());
        let after = drain(&mut lines);
        assert_eq!(
            with(&after, "\"op\":\"validate\"").len(),
            usize::from(remains),
            "only a peer retained by fresh status can reach file validation: {after:?}"
        );
        if !remains {
            assert!(shell.read_with(cx, |shell, _| shell.menu.providers.sides[0]
                .peers_for(Role::Tailscale)
                .is_empty()));
            assert!(!pending(&shell, cx));
            assert!(shell.read_with(cx, |shell, _| shell.menu.captured.is_none()));
        }
    }
}
