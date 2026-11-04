//! The automatic sweep cannot consume an old reply or overlap a view request.
use super::{
    dates,
    host::{Host, Swept},
    rows::Reply,
    view::View,
};
use crate::jsondoc;

fn reply(id: u64, op: &str) -> Reply {
    Reply::read(
        jsondoc::parse(&format!(
            r#"{{"t":"trashbrowse","id":{id},"op":"{op}","ok":true,"rows":[],"total":0}}"#
        ))
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn pb_trash_sweep_drops_stale_replies_and_keeps_requests_distinct() {
    let mut host = Host::new(View::new(31.));
    let now = dates::parse_ms("2026-10-26T12:00:00Z").unwrap();
    let catalog = crate::i18n::test_catalogs::english();
    host.sweep(now, true, 0);
    host.drain();
    assert_eq!(
        host.receive(&reply(99, "list"), &catalog, now),
        Some(Swept::Continue)
    );
    assert!(host.sweeping());
    assert!(host.drain().is_empty());
    host.open("");
    let requests = host.drain();
    let ids: Vec<_> = requests
        .iter()
        .map(|line| {
            jsondoc::parse(line)
                .unwrap()
                .get("id")
                .and_then(|id| id.as_f64())
                .unwrap() as u64
        })
        .collect();
    assert_eq!(ids.len(), 2);
    assert!(ids[0] < ids[1], "cancel and open have distinct request ids");
    host.receive(&reply(1, "list"), &catalog, now);
    assert_eq!(host.view.pending_op, "list");
    assert!(host.view.rows.is_empty());
}

#[test]
fn pb_trash_sweep_never_overlaps_a_closed_views_pending_work() {
    let mut host = Host::new(View::new(31.));
    host.open("");
    host.close();
    host.drain();
    host.sweep(2_000_000_000_000, true, 0);
    assert!(!host.sweeping());
    assert!(host.drain().is_empty());
}
