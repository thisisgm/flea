use super::*;

#[test]
fn reviewer_r2_completed_tailnet_survives_a_slow_lan_browser() {
    for with_lan in [false, true] {
        let fixture = Fixture::new();
        let browser = if with_lan {
            r#"case "$6" in _smb._tcp) printf '=;eth0;IPv4;NAS;_smb._tcp;local;nas.local;192.0.2.1;445;\n' ;; esac
exec /usr/bin/sleep 30"#
        } else { "exec /usr/bin/sleep 30" };
        fixture.script("avahi-browse", browser);
        fixture.script("tailscale", r#"printf '%s\n' '{"BackendState":"Running","Peer":{"ready":{"Online":true,"DNSName":"ready.tail.ts.net.","HostName":"ready"}}}'"#);
        let (tx, rx) = crate::backend::events::channel();
        let mut discovery = Discovery::default();
        let started = Instant::now();
        discovery.request_with_path("start", 1, tx, Some(fixture.root.clone()));
        let (ticket, reply) = result(&rx);
        let elapsed = started.elapsed();
        fixture.wait_gone();
        assert!(discovery.accept(ticket));
        eprintln!("REVIEW r2 completed tailnet: with_lan={with_lan}, elapsed={elapsed:?}, reply={}", jsondoc::render(&reply).trim());
        assert!(elapsed >= LIMIT && elapsed < Duration::from_secs(6), "{elapsed:?}");
        assert_eq!(field(&reply, "state"), "found", "a completed status must not disappear because another source times out");
        let rows = reply.get("rows").and_then(Json::as_array).unwrap();
        assert_eq!(rows.len(), 1 + usize::from(with_lan));
        assert!(rows.iter().any(|row| field(row, "host") == "ready.tail.ts.net"));
        if with_lan { assert!(rows.iter().any(|row| field(row, "host") == "nas.local")); }
        let rendered = jsondoc::render(&reply);
        assert!(rendered.contains("timed out after 3 seconds"));
        assert!(!rendered.contains("Tailscale status decode was cancelled or timed out"));
        assert_eq!(fixture.calls().lines().count(), 7);
    }
}

#[test]
fn decoded_tailnet_does_not_leak_into_reopened_dialog() {
    let fixture = Fixture::new();
    let status_file = fixture.root.join("status.json");
    fixture.dir.assert_contains(&status_file);
    std::fs::write(&status_file, r#"{"BackendState":"Running","Peer":{"old":{"Online":true,"DNSName":"old.tail.ts.net."}}}"#).unwrap();
    fixture.script("avahi-browse", "exec /usr/bin/sleep 30");
    fixture.script("tailscale", "exec /usr/bin/cat -- \"$root/status.json\"");
    let (tx, rx) = crate::backend::events::channel();
    let mut discovery = Discovery::default();
    discovery.request_with_path("start", 1, tx.clone(), Some(fixture.root.clone()));
    fixture.wait_calls(7);
    std::thread::sleep(Duration::from_millis(150));
    fixture.dir.assert_contains(&status_file);
    std::fs::write(&status_file, r#"{"BackendState":"Running","Peer":{"fresh":{"Online":true,"DNSName":"fresh.tail.ts.net."}}}"#).unwrap();
    discovery.request("stop", 1, tx.clone());
    let reopened = Instant::now();
    discovery.request_with_path("start", 2, tx, Some(fixture.root.clone()));
    loop {
        let (ticket, reply) = result(&rx);
        if !discovery.accept(ticket) { continue; }
        fixture.wait_gone();
        assert_eq!(field(&reply, "state"), "found");
        let rows = reply.get("rows").and_then(Json::as_array).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(field(&rows[0], "host"), "fresh.tail.ts.net");
        assert_eq!(fixture.calls().lines().count(), 14);
        assert!(reopened.elapsed() < Duration::from_secs(6));
        break;
    }
}
