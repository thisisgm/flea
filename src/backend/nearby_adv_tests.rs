use super::*;
use crate::backend::{nearbyrows, nearbytool::SIGNALLED_UNREAPED};

fn wait_file(path: &std::path::Path) {
    let deadline = Instant::now() + LIMIT;
    while !path.exists() && Instant::now() < deadline { std::thread::sleep(STEP); }
    assert!(path.exists(), "helper did not reach {}", path.display());
}

#[test]
fn adv_e_f1_group_signal_precedes_reaping_and_kills_a_grandchild() {
    let fixture = Fixture::new();
    fixture.script("avahi-browse", r#"(
    /usr/bin/sleep 30 &
    printf '%s\n' "$!" > "$root/grandchild"
    wait
) &
while [ ! -f "$root/grandchild" ]; do /usr/bin/sleep 0.01; done
exit 0"#);
    let mut tool = Tool::spawn(fixture.root.join("avahi-browse").to_str().unwrap(), &[], Some("_smb._tcp"), None).unwrap();
    wait_file(&fixture.root.join("grandchild"));
    SIGNALLED_UNREAPED.with(|seen| seen.set(Some(false)));
    let deadline = Instant::now() + LIMIT;
    while !tool.ended() && Instant::now() < deadline {
        tool.poll();
        std::thread::sleep(STEP);
    }
    let unreaped = SIGNALLED_UNREAPED.with(|seen| seen.replace(None)).unwrap();
    let (_, problem) = tool.finish(false);
    fixture.wait_gone();
    let pid = std::fs::read_to_string(fixture.root.join("grandchild")).unwrap();
    if let Ok(stat) = std::fs::read_to_string(format!("/proc/{}/stat", pid.trim())) {
        let state = stat.rsplit_once(") ").unwrap().1.chars().next().unwrap();
        assert!(matches!(state, 'Z' | 'X'), "grandchild survived: {stat}");
    }
    assert!(problem.is_none(), "{problem:?}");
    assert!(unreaped, "group signal used a leader PID that had already been reaped");
}

#[test]
fn adv_e_f3_timeout_sentence_uses_the_shared_limit() {
    let fixture = Fixture::new();
    fixture.script("avahi-browse", "exec /usr/bin/sleep 30");
    let mut tool = Tool::spawn(fixture.root.join("avahi-browse").to_str().unwrap(), &[], Some("_smb._tcp"), None).unwrap();
    fixture.wait_calls(1);
    let (_, problem) = tool.finish(true);
    fixture.wait_gone();
    assert_eq!(problem.unwrap(), format!("Discovery tool timed out after {} seconds.", LIMIT.as_secs_f64()));
}

#[test]
fn adv_e_f4_avahi_fields_cannot_change_the_address_structure() {
    for host in ["user@nas.local", "nas.local/path", "nas.local:445", "2001:db8::1", "bad_host.local", "host..local", "-host", "host-"] {
        let line = format!("=;eth0;IPv4;NAS;_smb._tcp;local;{host};192.0.2.1;445;\n");
        assert!(nearbyrows::avahi(&line, "_smb._tcp").is_empty(), "invalid host accepted: {host}");
    }
    for port in ["445@evil.local", "445/path", "445:22", "", "0", "65536", "+445", " 445"] {
        let line = format!("=;eth0;IPv4;NAS;_smb._tcp;local;nas.local;192.0.2.1;{port};\n");
        assert!(nearbyrows::avahi(&line, "_smb._tcp").is_empty(), "invalid port accepted: {port}");
    }
    for host in ["NAS-1.local.", "192.0.2.1", "[2001:db8::1]"] {
        let line = format!("=;eth0;IPv4;NAS;_smb._tcp;local;{host};192.0.2.1;00445;\n");
        let rows = nearbyrows::avahi(&line, "_smb._tcp");
        assert_eq!(rows.len(), 1, "{host}");
        assert_eq!(rows[0].address.host, host.trim_end_matches('.').to_ascii_lowercase());
        assert_eq!(rows[0].address.port, 445);
        assert!(rows[0].address.user.is_empty());
        assert_eq!(rows[0].address.path, "/");
    }
}

#[test]
fn adv_e_f4_tailnet_dns_cannot_inject_uri_fields() {
    for host in ["user@nas.ts.net", "nas.ts.net/path", "nas.ts.net:22", "bad_host.ts.net"] {
        let input = format!(r#"{{"BackendState":"Running","Peer":{{"one":{{"Online":true,"DNSName":"{host}"}}}}}}"#);
        assert!(nearbyrows::tailscale(&input).unwrap().is_empty(), "invalid DNS name accepted: {host}");
    }
}

#[test]
fn adv_e_f5_looking_state_is_explicit_even_with_blank_copy() {
    let reply = jsondoc::parse(&line(1, "looking", "", &[], &[])).unwrap();
    assert_eq!(field(&reply, "message"), "");
    assert_eq!(field(&reply, "state"), "looking");
}

#[test]
fn adv_e_f5_empty_state_is_explicit_even_with_blank_copy() {
    let reply = jsondoc::parse(&line(1, "empty", "", &[], &[])).unwrap();
    assert_eq!(field(&reply, "message"), "");
    assert_eq!(field(&reply, "state"), "empty");
}

#[test]
fn adv_e_f6_deadline_drains_late_stdout_and_stderr() {
    let fixture = Fixture::new();
    fixture.script("avahi-browse", r#"while [ ! -f "$root/write" ]; do /usr/bin/sleep 0.01; done
printf '=;eth0;IPv4;Late NAS;_smb._tcp;local;late.local;192.0.2.1;445;\n'
printf 'late diagnostic\n' >&2
: > "$root/ready"
exec /usr/bin/sleep 30"#);
    let mut tool = Tool::spawn(fixture.root.join("avahi-browse").to_str().unwrap(), &[], Some("_smb._tcp"), None).unwrap();
    fixture.wait_calls(1);
    tool.poll();
    let release = fixture.root.join("write");
    fixture.dir.assert_contains(&release);
    std::fs::write(release, b"release late writer\n").unwrap();
    wait_file(&fixture.root.join("ready"));
    let deadline = Instant::now() + STEP;
    std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
    let (output, problem) = tool.finish(true);
    fixture.wait_gone();
    assert_eq!(nearbyrows::avahi(&output, "_smb._tcp").len(), 1, "final pipe bytes were lost: {output:?}");
    assert!(problem.unwrap().contains("late diagnostic"));
}
