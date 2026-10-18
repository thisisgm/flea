use super::*;

fn wide_object() -> String {
    let mut input = String::from("{");
    for index in 0..90_000 {
        if index != 0 { input.push(','); }
        use std::fmt::Write;
        write!(input, "\"{index:05}\":{{}}").unwrap();
    }
    input.push('}');
    input
}

#[test]
fn cancellation_and_deadline_checks_interrupt_ignored_objects() {
    let input = format!(r#"{{"Ignored":{},"BackendState":"Running","Peer":{{}}}}"#, wide_object());
    let mut polls = 0;
    let result = decode(&input, &mut || { polls += 1; polls < 100 });
    assert_eq!(result, Err(stream::INTERRUPTED));
    assert_eq!(polls, 100);
    let deadline = std::time::Instant::now();
    assert_eq!(decode(&input, &mut || std::time::Instant::now() < deadline), Err(stream::INTERRUPTED));
    assert!(decode(&input, &mut || true).unwrap().is_empty());
}

#[test]
fn ignored_wide_objects_stay_linear_and_peer_fields_keep_last_values() {
    let input = format!(r#"{{"Ignored":{},"BackendState":"Stopped","BackendState":"Running","Peer":{{
        "one":{{"Online":false,"Online":true,"DNSName":"wrong.tail.ts.net.","DNSName":"right.tail.ts.net.","OS":"windows"}},
        "two":{{"Online":true,"DNSName":"removed.tail.ts.net."}},"two":{{}}
    }}}}"#, wide_object());
    let rows = decode(&input, &mut || true).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].address.uri, "smb://right.tail.ts.net/");
    let escaped = r#"{"BackendState":"Running","Peer":{"one":{"Onl\u0069ne":true,"DNSName":"pi.tail.ts.net.","HostName":"caf\u00e9"}}}"#;
    assert_eq!(decode(escaped, &mut || true).unwrap()[0].name, "café");
}

#[test]
fn invalid_or_excessive_ignored_data_is_refused_without_panicking() {
    for ignored in ["{\"a\":}", "[0,]", "{\"a\":true,}", "\"\\q\"", "\"\\uD80\"", "\"\\uGGGG\"", "01", "NaN", "true false", "true\u{000b}"] {
        let input = format!(r#"{{"Ignored":{ignored},"BackendState":"Running","Peer":{{}}}}"#);
        assert!(decode(&input, &mut || true).is_err(), "{input}");
    }
    for input in [
        format!(r#"{{"Ignored":"{}"}}"#, "a".repeat(9000)),
        format!(r#"{{"Ignored":{}0{}}}"#, "[".repeat(40), "]".repeat(40)),
        " ".repeat(1024 * 1024 + 1),
    ] {
        assert!(decode(&input, &mut || true).is_err());
    }
    let input = r#"{"BackendState":"Running","Ignored":{"x":[null,false,1.25e2,"quoted\"value"]},"Peer":{}}"#;
    assert!(decode(input, &mut || true).unwrap().is_empty());
    let surrogate = r#"{"BackendState":"Running","Ignored":"\uD800","Peer":{"one":{"Online":true,"DNSName":"pi.tail.ts.net.","HostName":"\uD800"}}}"#;
    assert_eq!(decode(surrogate, &mut || true).unwrap()[0].name, "\u{FFFD}");
    for at in 0..input.len() {
        assert!(decode(&input[..at], &mut || true).is_err(), "truncated at {at}");
    }
}
