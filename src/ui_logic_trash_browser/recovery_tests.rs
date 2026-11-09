//! TrashView.qml's list reply: a stale select-all is dropped with its sentence, and a recovered journal says what it did.
use super::dates_tests::english;
use super::host::Host;
use super::rows::Reply;
use super::view::{Signal, View};
use super::view_tests::{row, Backend, ROW};
use crate::jsondoc;

fn opened() -> (Backend, Host) {
    let mut backend = Backend::with(vec![
        row("a.txt", "2026-10-05T09:00:00"),
        row("b.txt", "2026-10-04T09:00:00"),
    ]);
    let mut host = Host::new(View::new(ROW));
    host.view.set_viewport(0., 500.);
    host.open("");
    backend.pump(&mut host);
    host.view.signals();
    (backend, host)
}

// One refresh answered with the given trailing fields, as the backend's list reply carries them.
fn listed(backend: &mut Backend, host: &mut Host, fields: &str) -> Vec<Signal> {
    host.view.refresh();
    assert_eq!(host.drain().len(), 1, "refresh sends one list");
    let reply = Reply::read(jsondoc::parse(&format!(
        r#"{{"t":"trashbrowse","id":{},"op":"list","ok":true,"total":2,"start":0,"rows":[{},{}],"present":[]{fields}}}"#,
        host.view.request_id,
        row("a.txt", "2026-10-05T09:00:00"),
        row("b.txt", "2026-10-04T09:00:00")
    )).unwrap()).unwrap();
    host.view.receive(&reply, &english());
    backend.pump(host);
    host.view.signals()
}

fn said(signals: &[Signal]) -> Vec<(String, bool)> {
    signals
        .iter()
        .filter_map(|s| match s {
            Signal::Status { text, error } => Some((text.clone(), *error)),
            _ => None,
        })
        .collect()
}

fn results(signals: &[Signal]) -> Vec<(String, String, bool)> {
    signals
        .iter()
        .filter_map(|s| match s {
            Signal::Result {
                headline,
                detail,
                error,
            } => Some((headline.clone(), detail.clone(), *error)),
            _ => None,
        })
        .collect()
}

const CHANGED: &str = "Trash selection changed; select the items again.";

#[test]
fn noscene_a_stale_select_all_is_dropped_with_the_selection_changed_sentence() {
    for fields in [
        r#","selectionToken":5,"selectionCount":2,"selectionStale":true"#,
        r#","selectionToken":6,"selectionCount":2,"selectionStale":false"#,
    ] {
        let (mut backend, mut host) = opened();
        host.view.selection.all = true;
        host.view.selection.token = 5;
        host.view.selection.count = 2;
        let signals = listed(&mut backend, &mut host, fields);
        assert_eq!(
            said(&signals),
            [(CHANGED.to_string(), true)],
            "a stale or replaced select-all is refused once, as an error: {fields}"
        );
        assert!(
            !host.view.selection.all
                && host.view.selection.token == 0
                && host.view.selection.count == 0,
            "and the whole selection is dropped: {fields}"
        );
    }
    let (mut backend, mut host) = opened();
    host.view.selection.all = true;
    host.view.selection.token = 5;
    let signals = listed(
        &mut backend,
        &mut host,
        r#","selectionToken":5,"selectionCount":2,"selectionStale":false"#,
    );
    assert!(said(&signals).is_empty(), "a live select-all says nothing");
    assert!(
        host.view.selection.all && host.view.selection.count == 2,
        "and keeps its count"
    );
    let signals = listed(
        &mut backend,
        &mut host,
        r#","selectionToken":5,"selectionCount":2,"selectionStale":true"#,
    );
    assert_eq!(said(&signals), [(CHANGED.to_string(), true)]);
    let signals = listed(
        &mut backend,
        &mut host,
        r#","selectionToken":9,"selectionCount":2,"selectionStale":true"#,
    );
    assert!(
        said(&signals).is_empty(),
        "without a select-all no reply says it"
    );
}

#[test]
fn noscene_a_recovered_journal_says_its_count_and_a_failed_one_says_why() {
    let (mut backend, mut host) = opened();
    let signals = listed(
        &mut backend,
        &mut host,
        r#","recoveredCount":1,"recoveryErrors":[]"#,
    );
    assert_eq!(
        results(&signals),
        [(
            "Recovered 1 interrupted file operation".to_string(),
            String::new(),
            false
        )]
    );
    let signals = listed(
        &mut backend,
        &mut host,
        r#","recoveredCount":3,"recoveryErrors":[]"#,
    );
    assert_eq!(
        results(&signals),
        [(
            "Recovered 3 interrupted file operations".to_string(),
            String::new(),
            false
        )]
    );
    let signals = listed(
        &mut backend,
        &mut host,
        r#","recoveredCount":2,"recoveryErrors":["one failed","two failed"]"#,
    );
    assert_eq!(
        results(&signals),
        [(
            "Flea could not finish an interrupted file action".to_string(),
            "one failed\ntwo failed".to_string(),
            true
        )],
        "errors win over the count and carry every line"
    );
    let signals = listed(
        &mut backend,
        &mut host,
        r#","recoveredCount":0,"recoveryErrors":[]"#,
    );
    assert!(
        results(&signals).is_empty(),
        "nothing recovered says nothing"
    );
}
