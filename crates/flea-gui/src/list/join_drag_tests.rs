//! Ipc.qml dragPaneGeometry over the live list join.
use super::{ask, frame, open};
use crate::list::bench;

#[test]
fn drag_pane_geometry_reads_the_painted_rows_of_the_named_pane() {
    let mut cx = bench::real_cx();
    let handle = open(&mut cx);
    frame(&mut cx, handle);
    let total: usize = ask(&mut cx, handle, "total", &[]).trim().parse().unwrap();
    let reply = ask(&mut cx, handle, "dragPaneGeometry", &["0", "1"]);
    let json = flea::jsondoc::parse(reply.trim()).unwrap();
    let text = |path: &[&str]| {
        path.iter()
            .try_fold(&json, |value, key| value.get(key))
            .and_then(flea::jsondoc::Json::as_str)
            .unwrap()
            .to_owned()
    };
    let number = |path: &[&str]| {
        path.iter()
            .try_fold(&json, |value, key| value.get(key))
            .and_then(flea::jsondoc::Json::as_f64)
            .unwrap()
    };
    assert_eq!(text(&["view"]), "list");
    assert_eq!(number(&["total"]), total as f64);
    assert_eq!(text(&["area"]), ask(&mut cx, handle, "listAreaRect", &[]));
    assert_eq!(
        text(&["folder", "rect"]),
        ask(&mut cx, handle, "rowRect", &["1"])
    );
    assert_eq!(
        text(&["folder", "name"]),
        ask(&mut cx, handle, "visibleRowName", &["1"])
    );
    assert_eq!(number(&["last", "index"]), (total - 1) as f64);
    assert_eq!(
        json.get("focused").and_then(flea::jsondoc::Json::as_bool),
        Some(true)
    );
    for args in [["0", "-1"], ["0", "100000"], ["5", "0"], ["-1", "0"]] {
        assert_eq!(ask(&mut cx, handle, "dragPaneGeometry", &args), "{}");
    }
}
