//! TrashHost.qml and TrashMonitor.qml: menus, action routing, the 30 day sweep and the count read.
use super::dates::{day_number, parse_ms, DAY_MS};
use super::dates_tests::english;
use super::host::{self, Host, Item, Swept};
use super::monitor::{self, Count};
use super::rows::Reply;
use super::view::{Signal, View};
use super::view_tests::{row, Backend, ROW};
use crate::jsondoc;

fn ids(items: &[Item]) -> Vec<String> {
    items.iter().map(|i| match i { Item::Entry(e) => format!("{}{}{}", e.id, if e.disabled { "-" } else { "" }, if e.danger { "!" } else { "" }), Item::Separator => "|".into() }).collect()
}

fn reply(text: &str) -> Reply {
    Reply::read(jsondoc::parse(text).unwrap()).unwrap()
}

#[test]
fn trash_menus_hold_their_rows_and_states() {
    assert_eq!(ids(&host::trash_menu(3, false)), ["open", "|", "restoreAll", "|", "emptyTrash!"]);
    assert_eq!(ids(&host::trash_menu(0, false)), ["open", "|", "restoreAll-", "|", "emptyTrash-!"], "an empty Trash disables both whole-Trash rows");
    assert_eq!(ids(&host::trash_menu(3, true)), ["open", "|", "restoreAll-", "|", "emptyTrash-!"], "Empty Trash… disables in place while busy");
    assert_eq!(ids(&host::selection_menu(false, &[])), ["restoreTrashSelection", "|", "delete!"]);
    assert_eq!(ids(&host::selection_menu(false, &["delete".into()])), ["restoreTrashSelection"], "a hidden row takes its separator with it");
    assert_eq!(ids(&host::selection_menu(true, &["restoreTrashSelection".into()])), ["delete-!"], "no separator leads");
    assert_eq!(ids(&host::sheet_menu(0, 2, false)), ["restoreTrashSelection-", "|", "delete-!", "open", "|", "restoreAll", "|", "emptyTrash!"],
        "the sheet disables the selection rows with nothing marked");
    assert!(host::sheet_action("settings") && host::sheet_action("quit"), "the sheet lists the forwarded window keys");
    for own in ["cursorDown", "cursorUp", "cursorFirst", "cursorLast", "toggleSelect", "selectAll", "deletePermanently", "menu", "escape"] {
        assert!(host::sheet_action(own), "EY-310: the sheet lists {own}, a key Trash answers itself");
    }
    for foreign in ["copy", "paste", "rename", "trash", "open", "parent", "filter", "viewGrid"] {
        assert!(!host::sheet_action(foreign), "the sheet leaves out {foreign}, which Trash does not answer");
    }
}

#[test]
fn trash_host_routes_actions_and_opens_with_the_first_one() {
    let mut backend = Backend::with(vec![row("a.txt", "2026-10-05T09:00:00"), row("b.txt", "2026-10-04T09:00:00")]);
    backend.hold_ops = vec!["prepare"];
    let mut host = Host::new(View::new(ROW));
    host.view.set_viewport(0., 300.);
    host.action("emptyTrash");
    backend.pump(&mut host);
    assert!(host.view.opened, "an action on a closed Trash opens it");
    let prepare = backend.ops("prepare")[0].clone();
    assert_eq!(prepare.get("emptyTrash"), Some(&jsondoc::Json::Bool(true)), "and runs the action once the listing answers");
    assert!(backend.ops("summary").is_empty(), "an opening action replaces the landing and its summary");
    backend.release(&mut host);
    assert!(host.view.confirmation.as_ref().is_some_and(|c| c.count.is_some()), "the prepared snapshot fills the card");
    host.action("restoreAll");
    host.close();
    assert!(host.drain().is_empty() && host.view.opened, "an open card takes no action and no close");
    host.view.confirmed(None);
    backend.pump(&mut host);
    host.close();
    backend.pump(&mut host);
    assert!(!host.view.opened, "close works once the card is answered");
}

#[test]
fn trash_sweep_runs_once_a_day_and_records_nothing_on_failure() {
    let i18n = english();
    let now = parse_ms("2026-10-18T12:00:00").unwrap();
    let old = "2026-09-01T09:00:00";
    let mut host = Host::new(View::new(ROW));
    host.sweep(now, false, 0);
    assert!(!host.sweeping(), "the sweep is opt in");
    host.sweep(now, true, day_number(now));
    assert!(!host.sweeping(), "and runs once a day");
    host.sweep(now, true, day_number(now - DAY_MS));
    let lines = host.drain();
    assert!(lines.len() == 1 && lines[0].contains(r#""op":"list""#) && lines[0].contains(r#""recover":false"#), "a sweep lists without recovery");
    let listed = format!(r#"{{"t":"trashbrowse","id":1,"op":"list","ok":true,"total":2,"start":0,"rows":[{},{}]}}"#, row("old.txt", old), row("new.txt", "2026-10-17T09:00:00"));
    assert_eq!(host.receive(&reply(&listed), &i18n, now), Some(Swept::Continue));
    let lines = host.drain();
    assert!(lines[0].contains(r#""uris":["trash:///old.txt"]"#) && lines[0].contains(r#""identities":["id-old.txt"]"#), "only the expired row is prepared, by identity");
    let prepared = reply(r#"{"t":"trashbrowse","id":2,"op":"prepare","ok":true,"token":5,"count":1}"#);
    assert_eq!(host.receive(&prepared, &i18n, now), Some(Swept::Continue));
    assert!(host.drain()[0].contains(r#""token":5,"op":"delete","id":3"#), "a counted review deletes by its token");
    let deleted = reply(r#"{"t":"trashbrowse","id":3,"op":"delete","ok":true,"done":1,"failed":0}"#);
    assert_eq!(host.receive(&deleted, &i18n, now), Some(Swept::Finished { day: day_number(now), deleted: true }), "a finished sweep records today");
    host.sweep(now, true, day_number(now - DAY_MS));
    host.drain();
    let failed = reply(r#"{"t":"trashbrowse","id":4,"op":"list","ok":false,"error":"gio failed"}"#);
    assert_eq!(host.receive(&failed, &i18n, now), Some(Swept::Failed), "a failed sweep records nothing");
    host.sweep(now, true, day_number(now - DAY_MS));
    host.drain();
    let fresh = format!(r#"{{"t":"trashbrowse","id":5,"op":"list","ok":true,"total":1,"start":0,"rows":[{}]}}"#, row("new.txt", "2026-10-17T09:00:00"));
    assert_eq!(host.receive(&reply(&fresh), &i18n, now), Some(Swept::Finished { day: day_number(now), deleted: false }), "nothing expired finishes at once");
    host.sweep(now, true, day_number(now - DAY_MS));
    host.open("");
    let lines = host.drain();
    assert!(lines[1].contains(r#""op":"cancel""#) && lines[2].contains(r#""op":"list""#) && !host.sweeping(), "opening Trash stops the sweep first");
}

#[test]
fn rail_empty_trash_stops_the_sweep_and_reviews_with_the_view_closed() {
    let i18n = english();
    let now = parse_ms("2026-10-18T12:00:00").unwrap();
    let mut host = Host::new(View::new(ROW));
    host.sweep(now, true, day_number(now - DAY_MS));
    host.drain();
    host.empty_from_rail();
    let lines = host.drain();
    assert!(lines.len() == 2 && lines[0].contains(r#""op":"cancel","id":2"#) && lines[1].contains(r#""emptyTrash":true"#) && lines[1].contains(r#""op":"prepare","id":3"#), "the sweep is cancelled first and the review numbers past it: {lines:?}");
    assert!(!host.sweeping() && !host.view.opened && host.view.confirming);
    let late = reply(r#"{"t":"trashbrowse","id":1,"op":"list","ok":true,"total":0,"start":0,"rows":[]}"#);
    assert_eq!(host.receive(&late, &i18n, now), None);
    assert_eq!(host.view.pending_op, "prepare", "a late sweep reply never answers the review");
    host.sweep(now, true, day_number(now - DAY_MS));
    assert!(!host.sweeping(), "no sweep starts under the card");
    let prepared = reply(r#"{"t":"trashbrowse","id":3,"op":"prepare","ok":true,"all":true,"token":7,"count":2,"bytes":9}"#);
    host.receive(&prepared, &i18n, now);
    assert!(host.view.confirmation.is_some() && !host.view.opened);
    host.view.confirmed(None);
    assert!(host.drain()[0].contains(r#""op":"cancel","id":4"#));
    host.receive(&reply(r#"{"t":"trashbrowse","id":4,"op":"cancel","ok":true}"#), &i18n, now);
    assert!(!host.view.busy(), "the closed view takes its own cancel answer");
    host.sweep(now, true, day_number(now - DAY_MS));
    assert!(host.sweeping(), "the next sweep is not blocked once the review is answered");
}

// TR2-c: what Empty Trash's sweep of unlisted entries leaves on disk joins the finished line, as an error.
#[test]
fn empty_trash_says_how_many_leftover_entries_it_could_not_remove() {
    let i18n = english();
    for (done, failed, leftover, headline) in [
        (2, 0, 0, "Deleted 2 of 2 items"),
        (
            2,
            0,
            1,
            "Deleted 2 of 2 items · 1 leftover entry could not be removed",
        ),
        (
            1,
            1,
            3,
            "Deleted 1 of 2 items · 1 failed · 3 leftover entries could not be removed",
        ),
    ] {
        let mut backend = Backend::with(vec![
            row("a.txt", "2026-10-05T09:00:00"),
            row("b.txt", "2026-10-04T09:00:00"),
        ]);
        backend.hold_ops = vec!["delete"];
        let mut host = Host::new(View::new(ROW));
        host.view.set_viewport(0., 300.);
        host.action("emptyTrash");
        backend.pump(&mut host);
        host.view.confirmed(Some(7));
        backend.pump(&mut host);
        let id = backend.held[0]
            .get("id")
            .and_then(jsondoc::Json::as_f64)
            .unwrap() as u64;
        host.view.signals();
        let failures = if failed > 0 {
            r#"{"uri":"trash:///b.txt","name":"b.txt","error":"busy"}"#
        } else {
            ""
        };
        let finished = reply(&format!(
            r#"{{"t":"trashbrowse","id":{id},"op":"delete","ok":true,"done":{done},"failed":{failed},"failures":[{failures}],"skipped":0,"cancelled":false,"leftover":{leftover}}}"#
        ));
        host.receive(&finished, &i18n, 0);
        let results: Vec<(String, bool)> = host
            .view
            .signals()
            .into_iter()
            .filter_map(|s| match s {
                Signal::Result {
                    headline, error, ..
                } => Some((headline, error)),
                _ => None,
            })
            .collect();
        assert_eq!(
            results,
            [(headline.to_string(), failed + leftover > 0)],
            "{leftover} left over"
        );
    }
}

#[test]
fn trash_monitor_reads_one_count_at_a_time() {
    assert_eq!(monitor::parse_count("standard::name: trash\n  trash::item-count: 12\n"), Some(12));
    for bad in ["", "trash::item-count:", "trash::item-count: -1", "trash::item-count: 1x", "item-count: 3"] {
        assert_eq!(monitor::parse_count(bad), None, "{bad:?} is no count");
    }
    let mut count = Count::default();
    assert!(count.refresh(), "the first refresh reads");
    assert!(!count.refresh() && !count.refresh(), "refreshes during a read coalesce");
    let read = count.finished(0, false, "trash::item-count: 4");
    assert!(!read.failed && read.again && count.ready && count.count == 4, "a good read stores the count and runs once more");
    assert!(count.refresh());
    let read = count.finished(0, true, "trash::item-count: 9");
    assert!(read.failed && !read.again && count.count == 4, "a timed out read keeps the last count");
    assert!(count.refresh());
    assert!(count.finished(1, false, "trash::item-count: 9").failed, "a failed exit is a failed read");
    let argv = monitor::owned_command(77, &monitor::QUERY);
    assert_eq!(&argv[..4], ["setpriv", "--pdeathsig", "TERM", "--"], "the helper dies with the UI");
    assert_eq!(&argv[8..], ["77", "gio", "info", "--attributes=trash::item-count", "trash:///"]);
}

#[test]
fn trash_row_menu_rows_read_in_title_case() {
    let i18n = english();
    let labels: Vec<String> = host::selection_menu(false, &[]).iter().filter_map(|i| match i { Item::Entry(e) => Some(i18n.tr(e.label_key)), Item::Separator => None }).collect();
    assert_eq!(labels, ["Restore", "Delete Permanently…"], "EY-365: both rows in the menus' title case, the ellipsis because a card follows");
}
