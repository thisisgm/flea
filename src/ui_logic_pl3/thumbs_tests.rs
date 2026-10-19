//! tests/trash-thumbs.qml's scheduling checks over a scripted backend; the drawn-image checks run in flea-gui's g2::pl3 tests.
use super::host::Host;
use super::rows::TrashRow;
use super::thumbs::{viewport, Held, Mode, Policy, Slot, Thumbs, CAP, SETTLE_FIRST_MS, SETTLE_LATER_MS};
use super::view::{Signal, View};
use super::view_tests::{Backend, ROW};
use crate::jsondoc::{self, Json};

fn image(index: usize) -> String {
    format!(r#"{{"uri":"trash:///image-{index}.png","original":"/original/image-{index}.png","directory":false,"deleted":"2026-10-05T09:00:00","icon":"image-x-generic","identity":"row-{index}","thumbnail":{{"path":"/fixture/Trash/files/image-{index}.png","identity":"backing-{index}","provider":"l1:{index}","mtime":5}}}}"#)
}

fn fixture() -> Vec<String> {
    let mut rows = vec![image(0), r#"{"uri":"trash:///notes.txt","original":"/original/notes.txt","directory":false,"icon":"text-x-generic","thumbnail":null}"#.to_string()];
    rows.extend((2..50).map(image));
    rows
}

/// Pumps the backend and fires every settle timer the view asked for, until both are quiet.
fn run(backend: &mut Backend, host: &mut Host) {
    loop {
        backend.pump(host);
        let settles: Vec<u64> = host.view.signals().into_iter().filter_map(|s| match s { Signal::Settle { generation, .. } => Some(generation), _ => None }).collect();
        if settles.is_empty() {
            return;
        }
        for generation in settles {
            host.view.settled(generation);
        }
    }
}

fn path(m: &Json) -> &str {
    m.get("path").and_then(Json::as_str).unwrap_or("")
}

fn asks_for(backend: &Backend, name: &str) -> usize {
    backend.commands("trashthumb").iter().filter(|m| path(m).ends_with(name)).count()
}

fn cancelled(backend: &Backend, id: u64) -> bool {
    backend.commands("trashthumbcancel").iter().any(|m| m.get("ids").and_then(Json::as_array).unwrap_or(&[]).iter().any(|v| v.as_f64() == Some(id as f64)))
}

fn index_of(m: &Json) -> usize {
    path(m).rsplit("image-").next().and_then(|t| t.strip_suffix(".png")).and_then(|n| n.parse().ok()).unwrap()
}

fn id(m: &Json) -> u64 {
    m.get("id").and_then(Json::as_f64).unwrap() as u64
}

fn opened() -> (Backend, Host) {
    let mut backend = Backend::with(fixture());
    backend.hold_ops = vec!["restore", "delete"];
    let mut host = Host::new(View::new(ROW));
    host.view.set_viewport(0., 124.);
    host.open("");
    run(&mut backend, &mut host);
    (backend, host)
}

#[test]
fn trash_thumbs_ask_once_per_visible_image_row() {
    let (mut backend, mut host) = opened();
    let asks: Vec<Json> = backend.commands("trashthumb").into_iter().cloned().collect();
    assert!(!asks.is_empty(), "the open view asks for its visible images");
    assert_eq!(asks_for(&backend, "image-0.png"), 1, "image row asks once");
    assert!(!asks.iter().any(|m| m.get("original").and_then(Json::as_str).unwrap().ends_with("notes.txt")), "text row never asks");
    let last = (124. / ROW).ceil() as usize - 1;
    assert!(asks.iter().all(|m| index_of(m) <= last), "only viewport rows ask");
    assert_eq!(asks_for(&backend, "image-49.png"), 0, "far rows never ask");
    assert_eq!(host.view.thumbs.file_for(0, host.view.policy), "", "no image source before reply");
    assert_eq!(asks[0].get("cacheOnly"), Some(&Json::Bool(false)), "a local row asks the decoder");
    assert!(host.view.thumbnailed(id(&asks[0]), "/fixture/thumb.png"), "the reply is taken");
    assert_eq!(host.view.thumbs.file_for(0, host.view.policy), "/fixture/thumb.png", "reply sets the decoded image");
    host.view.settled(host.view.thumbs.settle());
    assert_eq!(asks_for(&backend, "image-0.png"), 1, "answered image still asks once");
    let stale = id(backend.commands("trashthumb").into_iter().find(|m| path(m).ends_with("image-2.png")).unwrap());
    host.view.choose(2, super::selection::Extend::No);
    host.view.restore(false);
    run(&mut backend, &mut host);
    assert!(cancelled(&backend, stale), "restore cancels its pending request");
    assert_eq!(host.view.thumbs.file_for(0, host.view.policy), "", "restore clears its image");
    backend.release(&mut host);
    run(&mut backend, &mut host);
    assert!(!host.view.busy() && host.view.total == 49, "the restore finished and the row left");
    assert!(!host.view.thumbnailed(stale, "/fixture/thumb.png"), "removed row's stale reply is dropped");
    assert_eq!(host.view.thumbs.file_for(2, host.view.policy), "", "and draws nothing");
    assert_eq!(asks_for(&backend, "image-2.png"), 1, "removed backing never asks again");
}

#[test]
fn trash_thumbs_scroll_delete_hide_and_close_cancel() {
    let (mut backend, mut host) = opened();
    let total = host.view.total;
    host.view.send("window", View::browse().number("start", 0).number("count", total));
    run(&mut backend, &mut host);
    let pending: Vec<u64> = (0..total).filter(|i| host.view.thumbs.slot(*i) == Some(&Slot::Asked)).filter_map(|i| host.view.thumbs.ticket(i).map(|t| t.id)).collect();
    assert!(!pending.is_empty(), "scroll proof has pending visible rows");
    assert!(host.view.first == 0 && host.view.rows.len() == total && host.view.row_at(48).is_some(), "scroll destination metadata is already held");
    let windows = backend.ops("window").len();
    let last = total - 1;
    host.view.choose(last as isize, super::selection::Extend::No);
    run(&mut backend, &mut host);
    assert_eq!(backend.ops("window").len(), windows, "scroll does not refresh metadata");
    assert!(pending.iter().all(|id| cancelled(&backend, *id)), "scroll-out cancels pending visible rows");
    assert_eq!(asks_for(&backend, "image-49.png"), 1, "new viewport asks for its final row");
    let stale = host.view.thumbs.ticket(last).unwrap().id;
    assert!(host.view.thumbnailed(stale, "/fixture/thumb.png"));
    host.view.send("delete", View::browse().number("token", 1));
    run(&mut backend, &mut host);
    assert_eq!(host.view.thumbs.file_for(last, host.view.policy), "", "permanent deletion clears its image immediately");
    assert!(cancelled(&backend, stale), "permanent deletion drops its request id");
    backend.release(&mut host);
    run(&mut backend, &mut host);
    assert!(!host.view.busy() && host.view.total == total - 1, "the delete finished");
    assert!(!host.view.thumbnailed(stale, "/fixture/thumb.png"), "a stale reply after deletion is refused");
    assert_eq!(host.view.thumbs.file_for(last, host.view.policy), "", "permanently removed row's stale reply is dropped");
    let first = host.view.first.max(host.view.span().unwrap().0);
    let ticket = host.view.thumbs.ticket(first).expect("the viewport asked again").id;
    let asked = backend.commands("trashthumb").len();
    host.view.set_shown(false);
    run(&mut backend, &mut host);
    assert!(cancelled(&backend, ticket), "hiding the parent cancels its viewport");
    assert_eq!(host.view.thumbs.recorded(), 0, "hiding the parent clears its image state");
    host.view.set_viewport(host.view.scroll_y + ROW, 124.);
    run(&mut backend, &mut host);
    assert_eq!(backend.commands("trashthumb").len(), asked, "hidden viewport never asks");
    host.view.set_shown(true);
    run(&mut backend, &mut host);
    let first = host.view.span().unwrap().0;
    let ticket = host.view.thumbs.ticket(first).expect("a shown viewport asks again").id;
    host.close();
    run(&mut backend, &mut host);
    assert!(cancelled(&backend, ticket), "close cancels its viewport");
}

#[test]
fn trash_thumbs_policy_identity_cap_and_settle() {
    let (mut backend, mut host) = opened();
    assert_eq!(host.view.thumbs.delay_ms(), SETTLE_LATER_MS, "after the first batch every settle is the longer one");
    host.view.set_policy(Policy { mode: Mode::Off, ..Policy::default() });
    run(&mut backend, &mut host);
    assert_eq!(host.view.thumbs.tickets(), 0, "thumbnails off asks nothing and cancels every ticket");
    assert_eq!(host.view.thumbs.delay_ms(), SETTLE_FIRST_MS, "a reset settles at the first delay again");
    let before = backend.commands("trashthumb").len();
    host.view.set_policy(Policy { usb: false, ..Policy::default() });
    run(&mut backend, &mut host);
    assert!(backend.commands("trashthumb").len() > before, "a policy back on asks again");
    let usb = Backend::with(vec![image(0).replace(r#""icon""#, r#""storageClass":"usb","icon""#)]);
    let (mut backend, mut host) = (usb, Host::new(View::new(ROW)));
    host.view.set_policy(Policy { usb: false, ..Policy::default() });
    host.view.set_viewport(0., 124.);
    host.open("");
    run(&mut backend, &mut host);
    let ask = backend.commands("trashthumb")[0].clone();
    assert_eq!(ask.get("cacheOnly"), Some(&Json::Bool(true)), "a switched-off store asks the cache only");
    assert!(host.view.thumbnailed(id(&ask), ""));
    assert_eq!(host.view.thumbs.slot(0), Some(&Slot::CacheMiss), "an empty cache answer is a miss and keeps the glyph");
    let unclassified = Backend::with(vec![image(0).replace(r#""icon""#, r#""storageClass":null,"icon""#)]);
    let (mut backend, mut host) = (unclassified, Host::new(View::new(ROW)));
    host.view.set_viewport(0., 124.);
    host.open("");
    run(&mut backend, &mut host);
    assert!(backend.commands("trashthumb").is_empty(), "a row on an unclassified store never asks");
    let (mut backend, mut host) = opened();
    let ticket = host.view.thumbs.ticket(0).unwrap().id;
    backend.live[0] = backend.live[0].replace("backing-0", "backing-0b");
    host.view.send("window", View::browse().number("start", 0).number("count", 4));
    run(&mut backend, &mut host);
    assert!(cancelled(&backend, ticket), "a row whose backing changed cancels its ticket");
    assert!(!host.view.thumbnailed(ticket, "/fixture/thumb.png"), "and its old reply is refused");
    assert_eq!(asks_for(&backend, "image-0.png"), 2, "the new backing asks once");
    let stale = host.view.thumbs.settle();
    host.view.set_viewport(ROW, 124.);
    run(&mut backend, &mut host);
    host.view.drain();
    host.view.settled(stale);
    assert!(host.view.drain().is_empty(), "an outdated settle generation asks nothing");
    assert_eq!(viewport(0., ROW, 124., 0), None, "an empty Trash has no viewport");
    assert_eq!(viewport(15., ROW, 124., 49), Some((0, 4)), "a partly shown bottom row is in the viewport");
    assert_eq!(viewport(0., ROW, 124., 2), Some((0, 1)), "the viewport stops at the last row");
    let rows: Vec<TrashRow> = (0..CAP + 60).map(|i| TrashRow::read(&jsondoc::parse(&image(i)).unwrap()).unwrap()).collect();
    let held = Held { first: 0, rows: &rows, total: rows.len() };
    let mut thumbs = Thumbs::default();
    let mut first_id = None;
    for batch in rows.chunks(60).enumerate().map(|(n, chunk)| (n * 60, n * 60 + chunk.len() - 1)) {
        let (generation, _) = thumbs.scroll(Some(batch), true).unwrap();
        thumbs.request(generation, held, Some(batch), Policy::default(), true);
        let ids: Vec<u64> = (batch.0..=batch.1).map(|i| thumbs.ticket(i).unwrap().id).collect();
        first_id.get_or_insert(ids[0]);
        thumbs.drain();
        for id in &ids {
            assert!(thumbs.receive(*id, "/fixture/thumb.png", held, true), "a visible answer is taken");
        }
    }
    assert_eq!(thumbs.recorded(), CAP, "answered rows are capped");
    assert_eq!(thumbs.file_for(0, Policy::default()), "", "the oldest answer is evicted first");
    assert_eq!(thumbs.file_for(rows.len() - 1, Policy::default()), "/fixture/thumb.png", "the newest answer stays");
    assert!(thumbs.drain().iter().any(|line| jsondoc::parse(line).unwrap().get("ids").and_then(Json::as_array).unwrap_or(&[]).iter().any(|v| v.as_f64() == first_id.map(|id| id as f64))), "an evicted row's ticket is cancelled");
}
