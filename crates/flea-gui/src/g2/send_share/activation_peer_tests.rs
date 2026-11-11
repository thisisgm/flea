//! An installed provider must finish its fresh status before the cached peer can be chosen.
use super::*;
use crate::g2::device_rail::bfx_net_tests::Root;
use std::time::{Duration, Instant};

#[gpui::test]
fn uish_def1_provider_choice_waits_for_fresh_status_of_an_installed_peer(cx: &mut TestAppContext) {
    for (outside, remains) in [(true, false), (true, true), (false, true)] {
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
        let (shell, cx, mut lines, id) = warm_at(cx, outside);
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
        if remains {
            let request = with(&after, "\"op\":\"validate\"").pop().unwrap();
            assert_eq!(
                flea::json::field_str(&request, "action").as_deref(),
                Some(if outside {
                    "taildrop:old"
                } else {
                    "plugin-send:flea.tailscale:old"
                }),
                "the built-in plugin must keep its captured cursor or marked group"
            );
        }
        if !remains {
            assert!(shell.read_with(cx, |shell, _| shell.menu.providers.sides[0]
                .peers_for(Role::Tailscale)
                .is_empty()));
            assert!(!pending(&shell, cx));
            assert!(shell.read_with(cx, |shell, _| shell.menu.captured.is_none()));
        }
    }
}

#[test]
fn uish_def1_validation_choice_keeps_each_builtin_target_rule() {
    use crate::g2::plugins::menu::validation_choice;
    let tailscale = format!("plugin-send:{}", Role::Tailscale.plugin_id());
    let airdrop = format!("plugin-send:{}", Role::AirDrop.plugin_id());
    assert_eq!(
        validation_choice(&tailscale, "peer", false),
        "taildrop:peer"
    );
    assert_eq!(
        validation_choice(&tailscale, "peer", true),
        format!("{tailscale}:peer")
    );
    for inside in [false, true] {
        assert_eq!(
            validation_choice(&airdrop, "phone", inside),
            "airdrop:phone"
        );
        assert_eq!(validation_choice("sharelink", "", inside), "sharelink");
    }
}
