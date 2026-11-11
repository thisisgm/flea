use super::*;

#[test]
fn parses_every_value_kind_and_keeps_object_order() {
    let v =
        parse(r#"{"b":true,"n":null,"i":192,"f":1.0,"s":"x","a":[1,"two",false],"o":{"k":"v"}}"#)
            .expect("parse");
    let keys: Vec<&str> = v
        .as_object()
        .expect("object")
        .iter()
        .map(|(k, _)| k.as_str())
        .collect();
    assert_eq!(keys, ["b", "n", "i", "f", "s", "a", "o"]);
    assert_eq!(v.get("b").and_then(Json::as_bool), Some(true));
    assert_eq!(v.get("n"), Some(&Json::Null));
    assert_eq!(v.get("i").and_then(Json::as_f64), Some(192.0));
    assert_eq!(v.get("s").and_then(Json::as_str), Some("x"));
    assert_eq!(
        v.get("a").and_then(Json::as_array).map(<[Json]>::len),
        Some(3)
    );
    assert_eq!(
        v.get("o").and_then(|o| o.get("k")).and_then(Json::as_str),
        Some("v")
    );
}

#[test]
fn a_number_keeps_the_literal_it_was_written_with() {
    let v = parse(r#"{"width":192,"opacity":1.0}"#).expect("parse");
    assert_eq!(render(&v), "{\n  \"width\": 192,\n  \"opacity\": 1.0\n}\n");
}

// Rust's f64 parse takes literals JSON does not, and Json::Num keeps the literal, so render
// writes it straight back and the window's own JSON.parse refuses the file the settle wrote.
#[test]
fn a_number_only_rust_takes_is_refused_rather_than_written_back() {
    let mut written_back = Vec::new();
    for bad in [
        "1.", "1.e5", "5.e3", "0192", "-0192", "01", "00", "-.5", "+1", ".5", "1e", "0x1",
    ] {
        let doc = format!("{{\"a\":{}}}", bad);
        if let Ok(v) = parse(&doc) {
            written_back.push(format!("{} -> {}", bad, render(&v).replace('\n', "")));
        }
    }
    assert!(
        written_back.is_empty(),
        "written back verbatim: {:?}",
        written_back
    );
    for good in [
        "0", "-0", "192", "-12", "1.0", "0.5", "1e5", "1E5", "1e+5", "1.5e-3",
    ] {
        let v = parse(&format!("{{\"a\":{}}}", good))
            .unwrap_or_else(|e| panic!("{} must parse: {}", good, e));
        assert_eq!(
            render(&v),
            format!("{{\n  \"a\": {}\n}}\n", good),
            "the literal is written back as it was read"
        );
    }
}

#[test]
fn strings_survive_a_round_trip_with_every_escape_in_them() {
    let v = parse(r#"{"k":"a\"b\\c\nd\teAé\/f"}"#).expect("parse");
    assert_eq!(
        v.get("k").and_then(Json::as_str),
        Some("a\"b\\c\nd\te\u{41}\u{e9}/f")
    );
    assert_eq!(parse(&render(&v)).expect("reparse"), v);
}

// The whole sentence, not is_err(): a message that names no byte, or names the wrong one, is
// what this test's own name promises against and it could not see either before.
#[test]
fn malformed_input_is_an_error_naming_the_byte_it_is_about() {
    for (bad, message) in [
        ("", "the document ended before a value at byte 0"),
        // Byte 0 is any format string's own default, so the two messages this branch added
        // are each pinned at a nonzero offset as well.
        ("{\"a\":", "the document ended before a value at byte 5"),
        ("{", "a string was expected at byte 1"),
        ("{\"a\"}", "a colon was expected at byte 4"),
        ("{\"a\":}", "a value that is not JSON at byte 5"),
        ("[1,]", "a value that is not JSON at byte 3"),
        ("tru", "true expected at byte 0"),
        ("{\"a\":1}x", "trailing text at byte 7"),
        (
            "\"unterminated",
            "a string opened at byte 0 ran to the end of the document",
        ),
        (
            "{\"a\":\"unterminated",
            "a string opened at byte 5 ran to the end of the document",
        ),
        ("{\"a\":+5}", "a value that is not JSON at byte 5"),
        ("{\"a\":.5}", "a value that is not JSON at byte 5"),
        ("{\"a\":1.2.3}", "a value that is not JSON at byte 5"),
        ("{\"a\":inf}", "a value that is not JSON at byte 5"),
        ("{\"a\":\"\\q\"}", "an unknown escape at byte 7"),
        (
            "{\"a\":\"\\u12\"}",
            "a \\u escape that is not hex at byte 8",
        ),
        // u32::from_str_radix takes a leading + and JSON takes no sign at all, so "\u+041"
        // decoded to 'A' and a settle could write back what the window's JSON.parse refuses.
        (
            "{\"a\":\"\\u+041\"}",
            "a \\u escape that is not hex at byte 8",
        ),
        (
            "{\"a\":\"\\u\u{20ac}\u{20ac}\"}",
            "a \\u escape that is not hex at byte 8",
        ),
        ("{\"a\":\"\\u12", "a short \\u escape at byte 8"),
        ("{\"a\":\"\\", "an escape ran off the end at byte 7"),
    ] {
        assert_eq!(
            parse(bad).expect_err("should not parse"),
            message,
            "{} stopped elsewhere",
            bad
        );
    }
}

#[test]
fn a_document_nested_past_the_depth_limit_is_refused_rather_than_recursed() {
    let deep = format!("{}{}", "[".repeat(MAX_DEPTH + 1), "]".repeat(MAX_DEPTH + 1));
    assert!(parse(&deep).is_err());
    let ok = format!("{}{}", "[".repeat(MAX_DEPTH - 1), "]".repeat(MAX_DEPTH - 1));
    assert!(parse(&ok).is_ok());
}

#[test]
fn a_duplicate_key_keeps_the_last_one_the_way_a_json_reader_does() {
    let v = parse(r#"{"a":1,"a":2}"#).expect("parse");
    assert_eq!(v.as_object().expect("object").len(), 1);
    assert_eq!(v.get("a").and_then(Json::as_f64), Some(2.0));
}

#[test]
fn render_indents_nested_containers_and_ends_with_one_newline() {
    let v = parse(r#"{"o":{"a":[1,2]},"e":{},"l":[]}"#).expect("parse");
    assert_eq!(
        render(&v),
        "{\n  \"o\": {\n    \"a\": [\n      1,\n      2\n    ]\n  },\n  \"e\": {},\n  \"l\": []\n}\n"
    );
}
