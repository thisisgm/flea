use super::*;
use flea::jsondoc::{self, Json};
use flea_gui::tabs::state::{pack, restore_plan, Open, Plan, Remember, Tab};

fn plan_of(paths: &str, index: &str) -> Json {
    jsondoc::parse(&format!(
        r#"{{"startIn":"last","lastTabs":{{"paths":{paths},"index":{index}}}}}"#
    ))
    .unwrap()
}

fn plan(paths: &[&str], index: usize) -> Plan {
    Plan {
        paths: paths.iter().map(|path| path.to_string()).collect(),
        index,
    }
}

fn state(text: &str) -> Json {
    jsondoc::parse(text).unwrap()
}

#[test]
fn the_startup_plan_reopens_every_tab_in_order_only_for_last_folder() {
    let several = restore_plan(&plan_of(r#"["/a","/b","/c"]"#, "1"), "").unwrap();
    assert_eq!(
        several.paths,
        ["/a", "/b", "/c"],
        "several tabs reopen in order"
    );
    assert_eq!(several.index, 1, "with the remembered tab current");
    assert_eq!(
        restore_plan(&state(r#"{"startIn":"home"}"#), ""),
        None,
        "Home starts without a plan"
    );
    assert_eq!(
        restore_plan(&state(r#"{"startIn":"folder","startFolder":"/a"}"#), ""),
        None,
        "and so does Chosen folder"
    );
    assert_eq!(
        restore_plan(&plan_of(r#"["/a","/b"]"#, "1"), "/tmp/asked"),
        None,
        "a path named on the command line outranks the strip"
    );
    let gone = restore_plan(&plan_of(r#"["/gone","/b"]"#, "0"), "").unwrap();
    assert_eq!(
        gone.paths,
        ["/gone", "/b"],
        "a missing folder stays in the plan"
    );
    assert_eq!(
        restore_plan(&plan_of("[]", "0"), ""),
        None,
        "an empty strip falls back to the single start"
    );
    assert_eq!(
        restore_plan(&state(r#"{"startIn":"last","lastPath":"/a"}"#), ""),
        None,
        "and so does an old file holding only lastPath"
    );

    let many: Vec<String> = (0..11).map(|n| format!("\"/t{n}\"")).collect();
    let capped = restore_plan(&plan_of(&format!("[{}]", many.join(",")), "10"), "").unwrap();
    assert_eq!(capped.paths.len(), 9, "past the cap nine tabs reopen");
    let front: Vec<String> = (0..9).map(|n| format!("/t{n}")).collect();
    assert_eq!(capped.paths, front, "in the order they were remembered");
    assert_eq!(
        capped.index, 8,
        "with the index clamped to the last survivor"
    );
    assert_eq!(
        restore_plan(&plan_of(r#"["/a","/b"]"#, "-1"), "")
            .unwrap()
            .index,
        0,
        "a negative index clamps to the first"
    );
    assert_eq!(
        restore_plan(&plan_of(r#"["/a","/b"]"#, r#""1""#), "")
            .unwrap()
            .index,
        0,
        "and a non-number reads as the first"
    );
    assert_eq!(
        restore_plan(&plan_of(r#"["/a","/b"]"#, "1.5"), "")
            .unwrap()
            .index,
        0,
        "and so does a fraction"
    );
    assert_eq!(
        restore_plan(&plan_of(r#"["/a",7]"#, "0"), ""),
        None,
        "while a non-string entry voids the whole strip"
    );
    assert_eq!(
        restore_plan(&plan_of(r#"["/a",""]"#, "0"), ""),
        None,
        "and so does an empty one"
    );
    let late: Vec<String> = (0..10)
        .map(|n| format!("\"/t{n}\""))
        .chain(["7".to_string()])
        .collect();
    assert_eq!(
        restore_plan(&plan_of(&format!("[{}]", late.join(",")), "0"), ""),
        None,
        "even past the cap, where no tab of it would reopen"
    );
}

#[test]
fn the_saved_schema_round_trips_unchanged_through_the_writer_and_the_reader() {
    for written in [
        plan(&["/a"], 0),
        plan(&["/a", "/b", "/c"], 2),
        plan(
            &[
                "/tmp/a\rb",
                "/tmp/a\nb",
                "/tmp/quote\"and\\slash",
                "/tmp/tab\there",
                "/tmp/caf\u{e9}",
                "smb://nas/isos",
            ],
            4,
        ),
    ] {
        let text = written.to_json();
        let value = jsondoc::parse(&text).unwrap();
        assert_eq!(
            value
                .get("paths")
                .and_then(Json::as_array)
                .map(<[Json]>::len),
            Some(written.paths.len()),
            "{text} carries every path"
        );
        let keys: Vec<&str> = match &value {
            Json::Obj(pairs) => pairs.iter().map(|(key, _)| key.as_str()).collect(),
            _ => Vec::new(),
        };
        assert_eq!(
            keys,
            ["paths", "index"],
            "{text} holds exactly the two keys the saved schema takes, in its order"
        );
        let wrapped = state(&format!(r#"{{"startIn":"last","lastTabs":{text}}}"#));
        assert_eq!(
            restore_plan(&wrapped, "").as_ref(),
            Some(&written),
            "{text} reads back as the strip it wrote"
        );
    }
    assert_eq!(
        plan(&["/a", "/b"], 1).to_json(),
        r#"{"paths":["/a","/b"],"index":1}"#,
        "the bytes are the shipped writer's own"
    );
}

#[test]
fn a_restored_tab_starts_exactly_like_a_tab_opened_fresh_at_that_folder() {
    let mut fresh = Side::new(Fake::at("/here"));
    fresh.pane.view = "grid";
    fresh.pane.hidden = true;
    fresh.pane.cursor = 7;
    fresh.pane.order = order("size", true);
    let restored = fresh.state.restore_items(&fresh.pane, &["/here".into()]);
    fresh.state.open_new(&mut fresh.pane, None);
    let opened = fresh.state.tabs.as_ref().unwrap().items[1].clone();
    assert_ne!(
        restored[0].identity, opened.identity,
        "restored and fresh tabs have distinct identities"
    );
    let stripped = |tab: &Tab| Tab {
        identity: 0,
        ..tab.clone()
    };
    assert_eq!(
        stripped(&restored[0]),
        stripped(&opened),
        "a restored snapshot equals a freshly opened tab's state"
    );
    assert_eq!(
        fresh.state.current(),
        1,
        "and the fresh tab is the new current one"
    );

    let mut pane = Side::new(Fake::at("/a"));
    let items = pane
        .state
        .restore_items(&pane.pane, &["/a".into(), "/b".into()]);
    pane.state.tabs = Some(pack(items, 0, None));
    pane.state.select_at(&mut pane.pane, 1);
    assert_eq!(
        pane.pane.listed,
        ["/b"],
        "a restored tab lists its folder on first visit"
    );
    assert_eq!(pane.state.current(), 1, "and becomes current");

    let mut start = Side::new(Fake::at("/b"));
    start
        .state
        .restore(&start.pane, &plan(&["/a", "/b", "/c"], 1));
    assert_eq!(
        (start.paths(), start.state.current()),
        (vec!["/a".to_string(), "/b".into(), "/c".into()], 1),
        "a startup plan becomes the strip, with its remembered tab current"
    );
    assert!(
        start.pane.listed.is_empty(),
        "and the window's own first listing is the current tab's, so the strip lists nothing more"
    );
}

fn lag_open(pane: &mut Fake, path: &str, _: Open) {
    pane.busy = true;
    pane.listing_path = path.into();
    pane.drop_path = path.into();
    pane.listed.push(path.into());
}

fn land(pane: &mut Fake) {
    pane.path = pane.listing_path.clone();
    pane.drop_path = pane.path.clone();
    pane.busy = false;
    pane.listing_path.clear();
}

#[test]
fn what_a_strip_change_writes_names_the_folder_being_opened_and_the_live_current_tab() {
    let mut lag = Side::new(Fake::at("/a"));
    lag.pane.drop_path = "/a".into();
    lag.pane.on_open = Some(lag_open);
    let items = lag
        .state
        .restore_items(&lag.pane, &["/a".into(), "/b".into()]);
    lag.state.tabs = Some(pack(items, 0, None));
    lag.state.select_at(&mut lag.pane, 1);
    assert_eq!(
        lag.state.remembered(&lag.pane),
        plan(&["/a", "/b"], 1),
        "a switch before landing already names the folder being opened"
    );
    land(&mut lag.pane);
    assert_eq!(
        lag.state.remembered(&lag.pane),
        plan(&["/a", "/b"], 1),
        "and after landing it still does"
    );

    let mut strip = Side::new(Fake::at("/a"));
    let items = strip
        .state
        .restore_items(&strip.pane, &["/a".into(), "/b".into()]);
    strip.state.tabs = Some(pack(items, 0, None));
    strip.act("tabNext");
    assert_eq!(
        strip.state.remembered(&strip.pane),
        plan(&["/a", "/b"], 1),
        "a switch is remembered with its new current tab"
    );
    let mut nested = Side::new(Fake::at("/tabs/alpha"));
    let items = nested
        .state
        .restore_items(&nested.pane, &["/tabs".into(), "/tabs/alpha".into()]);
    nested.state.tabs = Some(pack(items, 1, None));
    nested.state.select_at(&mut nested.pane, 0);
    assert_eq!(
        nested.state.remembered(&nested.pane),
        plan(&["/tabs", "/tabs/alpha"], 0),
        "a switch records the new tab's own path once the pane has moved"
    );
    strip.act("tabNew");
    assert_eq!(
        strip.state.remembered(&strip.pane),
        plan(&["/a", "/b", "/b"], 2),
        "an opened tab joins the remembered strip"
    );
    strip.state.move_current(&mut strip.pane, -1);
    assert_eq!(
        strip.state.remembered(&strip.pane),
        plan(&["/a", "/b", "/b"], 1),
        "a reorder is remembered in its new order"
    );

    let single = Side::new(Fake::at("/x"));
    assert_eq!(
        single.state.remembered(&single.pane),
        plan(&["/x"], 0),
        "a pane with no tabs remembers its one folder"
    );
    let mut multi = Side::new(Fake::at("/two-moved"));
    multi.state.tabs = Some(pack(vec![Tab::at("/one"), Tab::at("/two")], 1, None));
    assert_eq!(
        multi.state.remembered(&multi.pane),
        plan(&["/one", "/two-moved"], 1),
        "the current tab is remembered live, not as its stale snapshot"
    );
    let mut searching = Side::new(Fake::at("/"));
    searching.pane.search_mode = "results";
    searching.pane.search_from = "/home/gm/Work".into();
    assert_eq!(
        searching.state.remembered(&searching.pane),
        plan(&["/home/gm/Work"], 0),
        "a tab left in search results remembers where the user was"
    );
}

#[test]
fn a_later_switch_supersedes_the_landing_an_earlier_one_still_owes() {
    let mut side = Side::new(Fake::at("/a").with_rows(|i| row(&format!("row-{i}"), false)));
    let mut b = Tab::at("/b");
    b.cursor = 5;
    b.order = order("size", true);
    let mut c = Tab::at("/c");
    c.cursor = 2;
    side.state.tabs = Some(pack(vec![Tab::at("/a"), b, c], 0, None));
    side.state.select_at(&mut side.pane, 1);
    side.state.select_at(&mut side.pane, 2);
    let pending = side.state.tabs.as_ref().unwrap().pending.clone();
    assert_eq!(
        (pending.cursor, pending.order),
        (Some(2), Some(Order::default())),
        "the newer switch owns the landing, not the one it superseded"
    );
    assert_eq!(
        side.pane.listed,
        ["/b", "/c"],
        "and each switch asked for its own folder"
    );
    assert!(
        side.pane.opened.iter().all(|open| open.supersede),
        "each asking as a navigation, so the earlier listing gives way"
    );
    side.state.apply_pending(&mut side.pane);
    assert_eq!((side.pane.cursor, side.pane.sorted.len()), (2, 0), "and the landing restores the newer tab's cursor in its own order, never the superseded tab's sort");

    let mut located = Side::new(Fake::at("/r").with_rows(|_| None));
    located.pane.can_locate = true;
    let payload =
        flea_gui::tabs::transfer::payload("222", "first", "/tmp/first", "list", "far.txt");
    located
        .state
        .receive_tab(&mut located.pane, &payload, None, "111");
    located.state.apply_pending(&mut located.pane);
    assert_eq!(
        located.pane.located,
        ["/tmp/first/far.txt"],
        "a landing that needs a locate asks for it"
    );
    located.state.select_at(&mut located.pane, 0);
    located.pane.cursor = 3;
    located.state.located(
        &mut located.pane,
        "/tmp/first/far.txt",
        "/tmp/first",
        Some(9),
    );
    assert_eq!(
        located.pane.cursor, 3,
        "a switch away supersedes the locate it left out, so its late answer moves nothing"
    );
}

#[test]
fn the_remember_writer_owes_only_changed_keys_and_never_writes_a_repeat() {
    let mut remember = Remember::seeded(&state(
        r#"{"lastPath":"/a","lastTabs":{"paths":["/a"],"index":0}}"#,
    ));
    assert_eq!(
        remember.owed("/a", &plan(&["/a"], 0), false, false),
        None,
        "the values already saved owe nothing"
    );
    assert_eq!(
        remember
            .owed("/b", &plan(&["/b"], 0), false, false)
            .as_deref(),
        Some(r#"{"lastPath":"/b","lastTabs":{"paths":["/b"],"index":0}}"#),
        "a navigation writes both keys together"
    );
    assert_eq!(
        remember
            .owed("/b", &plan(&["/a", "/b"], 1), false, false)
            .as_deref(),
        Some(r#"{"lastTabs":{"paths":["/a","/b"],"index":1}}"#),
        "a strip change on the same folder writes only lastTabs"
    );
    for _ in 0..60 {
        assert_eq!(
            remember.owed("/b", &plan(&["/a", "/b"], 1), false, false),
            None,
            "a repeated frame writes nothing"
        );
    }
    assert_eq!(remember.writes, 2, "so a minute of redraws costs no write");
    assert_eq!(
        remember.owed("/c", &plan(&["/c"], 0), true, false),
        None,
        "a Dual window writes nothing"
    );
    assert_eq!(
        remember.owed("/c", &plan(&["/c"], 0), false, true),
        None,
        "nor does a listing still out"
    );
    assert_eq!(
        remember.owed("", &plan(&[""], 0), false, false),
        None,
        "nor a pane with no folder yet"
    );
    let odd = Remember::seeded(&state(r#"{"lastTabs":{"paths":["/a"],"index":0.5}}"#)).owed(
        "/a",
        &plan(&["/a"], 0),
        false,
        false,
    );
    assert!(
        odd.is_some_and(|patch| patch.contains("lastTabs")),
        "a stored value the schema refuses is replaced"
    );
}
