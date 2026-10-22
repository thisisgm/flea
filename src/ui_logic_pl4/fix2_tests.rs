// Review round 2 probes, kept as regression tests: a plugin opens is checked after it decodes, and a remote one keeps no dot segment.
use super::plugin::parse_section;

#[test]
fn review2_a_file_opens_that_decodes_to_a_control_is_refused() {
    let line = r#"{"op":"section","rows":[{"id":"a","label":"a","opens":"file:///srv/a%0Ab"},{"id":"b","label":"b","opens":"file:///srv/x%E2%80%AEfdp.exe"},{"id":"c","label":"c","opens":"file:///srv/n%00ul"}]}"#;
    let rows = parse_section("p", line).unwrap();
    assert!(
        rows.is_empty(),
        "kept {:?}",
        rows.iter().map(|r| r.path.clone()).collect::<Vec<_>>()
    );
}

#[test]
fn a_remote_opens_with_a_dot_segment_or_a_decoded_control_is_refused() {
    let line = r#"{"op":"section","rows":[{"id":"a","label":"a","opens":"sftp://nas/srv/../etc"},{"id":"b","label":"b","opens":"smb://nas/%2e%2e/x"},{"id":"c","label":"c","opens":"sftp://nas/a%0Ab"},{"id":"d","label":"d","opens":"sftp://nas/srv/ok"}]}"#;
    let rows = parse_section("p", line).unwrap();
    assert_eq!(
        rows.iter().map(|r| r.uri.as_str()).collect::<Vec<_>>(),
        ["sftp://nas/srv/ok"],
        "only the clean remote row stands"
    );
}
