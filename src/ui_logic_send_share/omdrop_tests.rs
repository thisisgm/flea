use super::*;

#[test]
fn owly_wins_when_both_are_installed_and_neither_is_no_airdrop() {
    assert_eq!(pick("/usr/bin/owly", "/usr/bin/omdrop").map(|t| t.stack), Some(Stack::Owly));
    assert_eq!(
        pick("", "/usr/bin/omdrop"),
        Some(Tool { stack: Stack::Omdrop, path: "/usr/bin/omdrop".into() })
    );
    assert_eq!(pick("", ""), None);
}

#[test]
fn status_reads_the_radio_name_and_folder_and_a_missing_radio_backend_is_not_ready() {
    let on = r#"{"receiving": true, "visible": true, "remaining": null, "name": "Omarchy", "dir": "/home/u/Downloads", "mode": "forever", "reason": "ok"}"#;
    let status = status(on, 0);
    assert!(status.adapter.ready() && status.on);
    assert_eq!((status.name.as_str(), status.dir.as_str()), ("Omarchy", "/home/u/Downloads"));
    assert_eq!(status.adapter.card, STACK);
    // Receiving stopped but the radio still advertising for a send is on; receiving alone is not.
    let radio_only = super::status(r#"{"receiving": false, "visible": true}"#, 0);
    assert!(radio_only.on && !radio_only.receiving);
    let receiver_only = super::status(r#"{"receiving": true, "visible": false, "visibility": "contacts"}"#, 0);
    assert!(receiver_only.adapter.ready() && !receiver_only.on && receiver_only.receiving);
    assert!(receiver_only.contacts && !status.contacts, "Contacts Only is Omdrop's saved visibility");
    let no_backend = super::status(r#"{"receiving": false, "visible": null}"#, 0);
    assert_eq!(no_backend.adapter.readiness, Readiness::NotReady);
    assert!(!no_backend.on);
    assert_eq!(super::status("", 124).adapter.readiness, Readiness::NotReady);
    assert!(!super::status(on, 1).adapter.ready(), "a failed status run is not trusted");
}

#[test]
fn a_streamed_peer_is_listed_by_address_once_it_gives_a_name() {
    let phone = peer_line(r#"{"mac": "f2:f2:ee:c4:5a:f7", "rssi": null, "name": "Jane's iPhone"}"#);
    assert_eq!(
        phone,
        Some(Peer { id: "f2:f2:ee:c4:5a:f7".into(), name: "Jane's iPhone".into(), phone: true })
    );
    let mac = peer_line(r#"{"mac": "22:8b:38:31:89:4e", "rssi": -34, "name": "Studio Mac"}"#).unwrap();
    assert!(!mac.phone);
    assert_eq!(peer_line(r#"{"mac": "b2:c4:98:5e:e6:be", "rssi": null, "name": null}"#), None);
    assert_eq!(peer_line(r#"{"rssi": -40, "name": "No address"}"#), None);
    assert_eq!(peer_line("Looking for nearby devices…"), None);
}

#[test]
fn a_send_is_delivered_on_success_declined_in_omdrops_words_and_else_failed_with_its_reason() {
    assert_eq!(verdict("", 0), Verdict::Delivered);
    assert_eq!(verdict("They declined it.\n", 1), Verdict::Declined);
    assert_eq!(
        verdict("They never answered: timed out\n", 1),
        Verdict::Failed("They never answered: timed out".into())
    );
    assert_eq!(
        verdict("No device called Fred is nearby.\nCancelled.\n", 130),
        Verdict::Failed("No device called Fred is nearby.".into()),
        "Cancelled. is not a reason"
    );
    assert_eq!(verdict("", 2), Verdict::Failed(String::new()));
}

#[test]
fn argv_keeps_files_after_the_double_dash_and_bounds_the_status_run() {
    let files = ["-a.png".to_string(), "b c.jpg".to_string()];
    assert_eq!(
        send_argv("/usr/bin/omdrop", "f2:f2:ee:c4:5a:f7", &files),
        ["setsid", "-w", "/usr/bin/omdrop", "send", "--to=f2:f2:ee:c4:5a:f7", "--", "-a.png", "b c.jpg"]
    );
    assert_eq!(
        status_argv("/usr/bin/omdrop"),
        ["timeout", "--signal=KILL", "10", "/usr/bin/omdrop", "status", "--json"]
    );
}
