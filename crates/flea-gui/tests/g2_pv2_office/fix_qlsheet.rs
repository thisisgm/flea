//! FIX-QLSHEET proofs: a header that is its column's widest cell draws whole.
use super::*;

// EY-59: Difference is the widest cell in its column, so the header draws whole, not cut.
#[gpui::test]
fn pv2_csv_header_counts_toward_the_column_width(cx: &mut TestAppContext) {
    let (view, sent) = table(cx, true, 902., 625.);
    view.update(cx, |v, _, cx| v.set_path("/d/budget.csv", cx))
        .unwrap();
    serve(cx, view, &sent, &budget());
    let head = read(cx, view, |v| {
        v.head
            .iter()
            .map(|c| (c.text.to_string(), c.cut, c.w, v.feed.widths.clone()))
            .collect::<Vec<_>>()
    });
    assert_eq!(
        head[3].0, "Difference",
        "the Difference header draws whole: {head:?}"
    );
    assert!(!head[3].1, "the Difference header is not cut: {head:?}");
}
