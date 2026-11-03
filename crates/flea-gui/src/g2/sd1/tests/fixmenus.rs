//! FIX-MENUS EY-185: a Tailscale on PATH that cannot run keeps its Taildrop row, which reads unavailable.
use super::*;
use flea::ui_logic_sd1::providers::parse_facts;

// The backend's providers facts for a tailscale file without its executable bit, as src/backend/providers.rs words them.
const UNRUNNABLE: &str = r#"{"taildrop":{"installed":true,"command":"","reason":"/usr/bin/tailscale is not executable."},
"taildropSend":{"installed":true,"command":"/usr/bin/omarchy-tailscale-send","reason":""},
"localsend":{"installed":false,"command":"","reason":"localsend-cli is not installed."},
"rclone":{"installed":false,"command":"","reason":"rclone is not installed."},
"dropbox":{"installed":false,"command":"","reason":"dropbox-cli is not installed."},
"dropboxInfo":"","dropboxError":""}"#;

#[test]
fn fixmenus_ey185_an_unrunnable_tailscale_keeps_its_row_unavailable() {
    let facts = parse_facts(UNRUNNABLE).expect("facts parse");
    let mut send = ProviderSend::default();
    let opened = send.menu_opened(&Fake::default(), &facts, Some(&states()), &[], 0);
    assert!(
        send.row_on(Role::Tailscale),
        "v0.3.12 keeps Send with Taildrop for an installed Tailscale"
    );
    assert_eq!(
        status_runs(&opened),
        0,
        "nothing runs a tailscale that cannot run"
    );
    assert!(
        !send.taildrop_refreshing() && send.peers_for(Role::Tailscale).is_empty(),
        "so the row has no peers and is not checking, which mn1 draws red"
    );
    let absent = parse_facts(&UNRUNNABLE.replacen(
        r#""installed":true,"command":"","reason":"/usr/bin/tailscale is not executable.""#,
        r#""installed":false,"command":"","reason":"tailscale is not installed.""#,
        1,
    ))
    .expect("facts parse");
    let mut send = ProviderSend::default();
    send.menu_opened(&Fake::default(), &absent, Some(&states()), &[], 0);
    assert!(
        !send.row_on(Role::Tailscale),
        "a Tailscale that is not installed still has no row"
    );
}
