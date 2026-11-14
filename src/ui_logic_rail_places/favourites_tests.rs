// Typed checks for the shared Favorites writer, ported from the Favourites.qml cases in tests/qml.
use super::favourites::*;
use super::places::stringify;
use super::testing::record;

fn two() -> Favourites {
    let mut favs = Favourites::default();
    favs.records = vec![
        record("Docs", "/home/gm/Docs"),
        record("Isos", "smb://nas/isos"),
    ];
    favs
}

#[test]
fn fx462_unavailable_inspections_retry_without_discarding_the_diagnostic() {
    let mut favs = two();
    favs.statuses.insert(0, "unavailable (NotFound)".into());
    favs.statuses.insert(1, String::new());
    assert_eq!(
        favs.inspect(&[0, 1]).as_deref(),
        Some(r#"{"op":"inspect","indices":[0]}"#),
        "a failed availability check must not cache out the next explicit retry",
    );
    assert_eq!(favs.statuses[&0], "unavailable (NotFound)");
    assert!(favs.inspect(&[0]).is_none(), "one inspection at a time");
    let answer = format!(
        r#"{{"statuses":[{{"index":0,"record":{},"error":""}}]}}"#,
        stringify(&favs.records[0])
    );
    assert_eq!(favs.inspected(Some(0), &answer), None);
    assert!(favs.statuses[&0].is_empty());
    assert!(
        favs.inspect(&[0, 1]).is_none(),
        "healthy rows keep their cache"
    );
}

#[test]
fn every_edit_but_add_names_the_records_it_was_decided_against() {
    let records = two().records;
    let add = Operation::Add {
        label: "Work".into(),
        path: "/w".into(),
    }
    .wire(&records);
    assert_eq!(add, r#"{"op":"add","record":{"label":"Work","path":"/w"}}"#);
    let remove = Operation::Remove { index: 1 }.wire(&records);
    assert!(
        remove.starts_with(r#"{"op":"remove","index":1,"expected":[{"#),
        "{remove}"
    );
    let moved = Operation::Move { index: 0, to: 1 }.after(&records);
    assert_eq!(
        moved,
        [records[1].clone(), records[0].clone()],
        "a move predicts the new order"
    );
    let renamed = Operation::Rename {
        index: 0,
        label: "D".into(),
    }
    .after(&records);
    assert_eq!(
        renamed[0],
        record("D", "/home/gm/Docs"),
        "a rename predicts the new label"
    );
    assert_eq!(
        Operation::Remove { index: 9 }.after(&records),
        records,
        "an index off the end predicts nothing"
    );
}

#[test]
fn one_write_runs_at_a_time_and_its_exit_decides_the_sentence() {
    let mut favs = two();
    assert!(favs.apply(&Operation::Remove { index: 0 }).is_ok());
    assert_eq!(
        favs.apply(&Operation::Remove { index: 1 }),
        Err(Failure::Busy),
        "a second write is refused while one runs"
    );
    assert!(
        !favs.records_changed(favs.records.clone()),
        "the writer's own change asks nobody to re-read"
    );
    let failed = favs.finish(
        Some(1),
        "",
        "flea: favourites changed since read\nmore",
        None,
    );
    assert_eq!(
        failed.failure,
        Some(Failure::NotSavedBecause(
            "favourites changed since read".into()
        )),
        "the first CLI line, prefix dropped"
    );
    assert!(!failed.committed && !favs.busy);
    favs.apply(&Operation::Remove { index: 0 }).unwrap();
    assert_eq!(
        favs.finish(None, "", "", None).failure,
        Some(Failure::NotSaved),
        "a timeout with nothing said is not saved"
    );
    favs.apply(&Operation::Remove { index: 0 }).unwrap();
    let unread = favs.finish(Some(0), "garbage", "", None);
    assert_eq!(
        (unread.failure, unread.committed),
        (Some(Failure::SavedUnread), true),
        "exit zero committed even unread"
    );
    favs.apply(&Operation::Remove { index: 0 }).unwrap();
    let predicted = Operation::Remove { index: 0 }.after(&two().records);
    let printed = format!(
        r#"{{"places":{{"favourites":{}}}}}"#,
        stringify(&crate::jsondoc::Json::Arr(predicted.clone()))
    );
    let done = favs.finish(Some(0), &printed, "", None);
    assert_eq!(favs.records, predicted, "the printed state is adopted");
    assert_eq!(
        done,
        Finished {
            failure: None,
            committed: true,
            external_change: false
        },
        "the predicted records landed"
    );
    assert!(
        favs.records_changed(Vec::new()),
        "a change nobody here made asks for a re-read"
    );
}

#[test]
fn inspection_lands_only_on_records_that_still_read_the_same() {
    let mut favs = two();
    let ask = favs.inspect(&[0, 1]).expect("two unchecked rows");
    assert_eq!(ask, r#"{"op":"inspect","indices":[0,1]}"#);
    assert_eq!(favs.inspect(&[0, 1]), None, "one inspection runs at a time");
    let answer = format!(
        r#"{{"statuses":[{{"index":0,"record":{},"error":""}},{{"index":1,"record":{},"error":"unreachable"}}]}}"#,
        stringify(&favs.records[0]),
        stringify(&record("Moved", "/elsewhere"))
    );
    assert_eq!(favs.inspected(Some(0), &answer), None);
    assert_eq!(
        favs.statuses.get(&0).map(String::as_str),
        Some(""),
        "a matching record takes its status"
    );
    assert!(
        !favs.statuses.contains_key(&1),
        "a record that changed under it takes nothing"
    );
    assert_eq!(
        favs.inspect(&[0, 1]).as_deref(),
        Some(r#"{"op":"inspect","indices":[1]}"#),
        "only unchecked rows are asked again"
    );
    assert_eq!(
        favs.inspected(Some(0), r#"{"statuses":[{"index":-1}]}"#),
        Some(Failure::InspectUnread)
    );
    favs.inspect(&[1]);
    assert_eq!(
        favs.inspected(Some(2), "x"),
        None,
        "a bad exit lands nothing and says nothing"
    );
    let many: Vec<usize> = (0..INSPECT_MAX + 10).collect();
    let mut wide = Favourites::default();
    let asked = wide.inspect(&many).unwrap();
    let named = crate::jsondoc::parse(&asked).ok().and_then(|doc| {
        doc.get("indices")
            .and_then(|i| i.as_array())
            .map(<[_]>::len)
    });
    assert_eq!(
        named,
        Some(INSPECT_MAX),
        "one inspection names at most the CLI's cap"
    );
}

#[test]
fn a_menu_remove_checks_the_row_it_opened_on() {
    let mut favs = two();
    assert_eq!(
        favs.remove_at(0, "/home/gm/Other"),
        Err(Failure::ChangedReopen),
        "a rebuilt row asks to reopen"
    );
    assert!(favs.remove_at(0, "/home/gm/Docs").is_ok());
    let mut keyed = two();
    let key = format!("favourite:1:{}", stringify(&keyed.records[1]));
    assert!(
        keyed.remove_keyed(&key).is_ok(),
        "the older key removes while its record stands"
    );
    let mut stale = two();
    assert_eq!(
        stale.remove_keyed(r#"favourite:1:{"label":"x"}"#),
        Err(Failure::ChangedReopen)
    );
    assert_eq!(
        stale.remove_keyed("favourite:x:{}"),
        Err(Failure::ChangedReopen)
    );
    assert_eq!(stale.remove_keyed("place:1"), Err(Failure::ChangedReopen));
    assert_eq!(Failure::NotSavedBecause("x".into()).args(), ["x"]);
    assert_eq!(Failure::Busy.key(), "places.favorite.busy");
}
