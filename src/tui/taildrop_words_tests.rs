use super::*;
fn ready() -> Taildrop {
    let mut taildrop = Taildrop::new();
    taildrop.refresh_locale(&crate::tui::render::cards::words_tests::english());
    taildrop
}
#[test]
fn refreshed_target_keeps_node_and_address_identity() {
    let mut taildrop = ready();
    let selected = Peer {
        id: "node-one".into(),
        label: "before".into(),
        address: "alpha.invalid".into(),
    };
    taildrop.peers = vec![Peer {
        label: "renamed".into(),
        ..selected.clone()
    }];
    assert_eq!(taildrop.current_peer(&selected).unwrap().label, "renamed");
    taildrop.peers[0].id = "node-two".into();
    assert!(taildrop.current_peer(&selected).is_none());
    taildrop.peers[0] = Peer {
        address: "beta.invalid".into(),
        ..selected.clone()
    };
    assert!(taildrop.current_peer(&selected).is_none());
    taildrop.peers.clear();
    assert!(taildrop.current_peer(&selected).is_none());
}

// A child already reaped, so poll learns its status with no wait: try_wait hands back the saved one.
fn finished(code: i32) -> Child {
    let mut child = Command::new("sh")
        .args(["-c", &format!("exit {code}")])
        .stdin(Stdio::null())
        .spawn()
        .unwrap();
    child.wait().unwrap();
    child
}
#[test]
fn a_failed_send_and_a_failed_status_call_read_as_plain_sentences() {
    let mut taildrop = ready();
    taildrop.child = Some(finished(7));
    taildrop.sending = Some("Alpha".into());
    taildrop.poll();
    assert_eq!(
        taildrop.sent,
        Some(Err("Could not send to Alpha (exit status: 7)".into()))
    );
    // Each way the status call ends has its own sentence, so a test of one cannot pass through another.
    let status_after = |code: i32, answer: io::Result<Vec<u8>>| {
        let mut taildrop = ready();
        let (tx, rx) = mpsc::channel();
        tx.send(answer).unwrap();
        taildrop.child = Some(finished(code));
        taildrop.output = Some(rx);
        taildrop.poll();
        taildrop.error.clone()
    };
    assert_eq!(
        status_after(7, Ok(Vec::new())),
        "Tailscale reported an error"
    );
    assert_eq!(
        status_after(0, Ok(Vec::new())),
        "Taildrop status could not be read",
        "an empty answer is not a status"
    );
    assert_eq!(
        status_after(
            0,
            Err(io::Error::new(io::ErrorKind::InvalidData, TOO_LARGE))
        ),
        "Taildrop status was too large to read"
    );
    assert_eq!(
        status_after(7, Err(io::Error::from(io::ErrorKind::UnexpectedEof))),
        "Taildrop status could not be read"
    );
}
// The limit is the reader thread's own, so a reply past it ends as the too-large sentence and one at it still parses.
#[test]
fn a_reply_past_the_limit_is_cut_by_the_reader_thread_and_reads_as_too_large() {
    let after = |output: Box<dyn Read + Send>| {
        let mut taildrop = ready();
        taildrop.output = Some(read_status(output));
        taildrop.child = Some(finished(0));
        for _ in 0..200 {
            taildrop.poll();
            if taildrop.child.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        (
            taildrop.error.clone(),
            taildrop
                .peers
                .iter()
                .map(|peer| peer.label.clone())
                .collect::<Vec<_>>(),
        )
    };
    // A valid status padded with spaces to an exact size, so only the size differs between the legs.
    let status = br#"{"BackendState":"Running","Self":{"Capabilities":["https://tailscale.com/cap/file-sharing"]},"Peer":{"a":{"Online":true,"TaildropTarget":1,"HostName":"alpha"}}}"#;
    let padded = |size: u64| -> Box<dyn Read + Send> {
        Box::new(
            io::Cursor::new(status.as_slice())
                .chain(io::repeat(b' '))
                .take(size),
        )
    };
    assert_eq!(
        after(Box::new(io::repeat(b'x'))),
        (
            "Taildrop status was too large to read".to_string(),
            Vec::new()
        )
    );
    assert_eq!(
        after(padded(MAX_STATUS_BYTES + 1)),
        (
            "Taildrop status was too large to read".to_string(),
            Vec::new()
        ),
        "one byte past the limit is refused"
    );
    assert_eq!(
        after(padded(MAX_STATUS_BYTES)),
        (String::new(), vec!["alpha".to_string()]),
        "a reply at the limit is read whole and parsed"
    );
}
#[test]
fn provider_state_overrides_stale_peer_eligibility() {
    let fixture = r#"{"BackendState":"Running","Self":{"Capabilities":["https://tailscale.com/cap/file-sharing"]},"Peer":{"old":{"Online":true,"TaildropTarget":1,"HostName":"alpha"}}}"#;
    assert_eq!(
        available_peers(&jsondoc::parse(fixture).unwrap()).unwrap()[0].label,
        "alpha"
    );
    for (state, reason) in [
        ("NeedsLogin", "menu.state.signedOut"),
        ("Stopped", "menu.state.offline"),
        ("", "menu.state.offline"),
    ] {
        let value = jsondoc::parse(&fixture.replace("Running", state)).unwrap();
        assert_eq!(available_peers(&value).err(), Some(reason));
    }
    let disabled =
        jsondoc::parse(&fixture.replace("https://tailscale.com/cap/file-sharing", "unrelated"))
            .unwrap();
    assert_eq!(
        available_peers(&disabled).err(),
        Some("tui.taildrop.disabled")
    );
    let mapped = fixture.replace(
        r#""Capabilities":["https://tailscale.com/cap/file-sharing"]"#,
        r#""CapMap":{"https://tailscale.com/cap/file-sharing":[]}"#,
    );
    assert_eq!(
        available_peers(&jsondoc::parse(&mapped).unwrap())
            .unwrap()
            .len(),
        1
    );
    let empty = jsondoc::parse(&fixture.replace(r#""Online":true"#, r#""Online":false"#)).unwrap();
    assert_eq!(available_peers(&empty).err(), Some("menu.state.noDevices"));
}

#[test]
fn targets_match_oem_eligibility_and_refuse_option_names() {
    let value = jsondoc::parse(r#"{"Self":{"UserID":1},"Peer":{"a":{"Online":true,"UserID":1,"HostName":"alpha"},"b":{"Online":false,"TaildropTarget":1,"HostName":"offline"},"c":{"Online":true,"TaildropTarget":2,"HostName":"denied"},"d":{"Online":true,"TaildropTarget":1,"DNSName":"exit.mullvad.ts.net."},"e":{"Online":true,"TaildropTarget":1,"HostName":"--help"},"f":{"Online":true,"TaildropTarget":1,"HostName":"localhost","DNSName":"beta.tail.ts.net."}}}"#).unwrap();
    let found = peers(&value);
    assert_eq!(
        found.iter().map(|p| p.label.as_str()).collect::<Vec<_>>(),
        vec!["alpha", "beta"]
    );
    assert_eq!(found[1].address, "beta.tail.ts.net");
    assert!(peers(&Json::Null).is_empty());
}
#[test]
fn a_sender_that_cannot_start_names_taildrop_the_peer_and_the_cause() {
    let root = crate::backend::testdir::TestDir::new("taildrop-sender");
    let unrunnable = root.file("omarchy-tailscale-send", "not a program");
    let peer = Peer {
        id: "node-one".into(),
        label: "phone".into(),
        address: "phone.invalid".into(),
    };
    for (sender, cause) in [
        ("/nonexistent/omarchy-tailscale-send", "item not found"),
        (unrunnable.to_str().unwrap(), "permission denied"),
    ] {
        let mut taildrop = Taildrop::new();
        let error = taildrop
            .send_with(sender, &peer, &["/tmp/a".into()])
            .unwrap_err();
        assert!(!taildrop.loading());
        let mut m = crate::tui::render::frame_tests::model();
        m.fail_io(&error);
        assert_eq!(
            m.error,
            format!("Taildrop · Could not send to phone ({sender}: {cause})")
        );
    }
}
