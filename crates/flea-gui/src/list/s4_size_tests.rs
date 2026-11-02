//! INT-AUTHOR-A row 51: inspect the painted cells while every requested walk reply is withheld.
use super::*;

#[test]
fn s4_held_size_replies_keep_the_middle_dot_until_painted() {
    let mut cx = bench::real_cx();
    let handle = folders(&mut cx, "");
    classify(&mut cx, handle);
    settle(&mut cx);
    let command = request(&mut cx, handle);
    assert_eq!(command.get("c").and_then(Json::as_str), Some("dirsize"));
    let rows = command.get("rows").and_then(Json::as_array).unwrap();
    assert!(!rows.is_empty());
    let held: Vec<_> = (0..rows.len()).map(|_| next(&mut cx, handle)).collect();
    for row in ["0", "1", "2"] {
        assert_eq!(ask(&mut cx, handle, "rowSizeText", &[row]), "\u{b7}");
    }
    for reply in held {
        receive(&mut cx, handle, reply);
    }
    frame(&mut cx, handle);
    assert_eq!(ask(&mut cx, handle, "rowSizeText", &["0"]), "243 B");
    assert_eq!(ask(&mut cx, handle, "rowSizeText", &["1"]), ">244 B");
}
