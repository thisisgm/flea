//! tests/columnspeekgate.qml and tests/columnspeekorder.qml: a width, path or view step alone asks the parent peek, and an order change re-asks it.
use super::check::Checks;
use super::fixture::{drain_asks, reply_with, Pane, Producers, SETTINGS};
use flea_gui::columns::peeks::{join_names, peek_key};
use flea_gui::columns::Area;
use flea_gui::navigation::folder_state::Folders;
use std::rc::Rc;

/// The probes' stub pane: no rows, so the cursor never moves and every ask comes from the width, the path or the view.
fn stub(here: &str) -> Pane {
    let mut pane = Pane::at(here, Vec::new());
    pane.first = 35;
    pane
}

fn sync(area: &mut Area, pane: &Pane, producers: &mut Producers) {
    pane.with(|view| area.sync(&view, &mut producers.ports(), 0));
}

/// The stub pane's path changes with nothing else, which runs onPathChanged.
fn land(area: &mut Area, pane: &mut Pane, producers: &mut Producers, here: &str) {
    pane.here = here.into();
    pane.base = here.into();
    pane.with(|view| area.path_landed(&view, &mut producers.ports(), 0));
}

fn set_visible(area: &mut Area, pane: &Pane, producers: &mut Producers, visible: bool) {
    pane.with(|view| area.set_visible(visible, &view, &mut producers.ports(), 0));
}

/// stubBackend.peeked: a reply in wire form, echoing the pane's window size; order is "" or ",\"by\":...".
fn answer(
    area: &mut Area,
    pane: &Pane,
    producers: &mut Producers,
    path: &str,
    rows: &[&str],
    order: &str,
) {
    let rows: Vec<(&str, bool)> = rows.iter().map(|name| (*name, false)).collect();
    let reply = reply_with(path, &rows, false, false, pane.first, order);
    pane.with(|view| area.peeked(&reply, &view, &mut producers.ports(), 0));
}

fn asked(log: &[String], path: &str) -> bool {
    log.iter().any(|asked| asked == path)
}

#[test]
fn a_width_path_or_view_step_alone_asks_the_parent_peek() {
    let mut c = Checks::new("tests/columnspeekgate.qml");
    // The failure list, the phase timers, the early reports and the PASS or FAIL lines are this test's steps and finish.
    c.covers(&[
        89, 96, 98, 99, 100, 101, 102, 103, 109, 118, 129, 138, 149, 161, 172, 177, 186,
    ]);
    let none = Vec::<String>::new();
    let mut area = Area::new(800., 600., None, 0., SETTINGS);
    let mut producers = Producers::default();
    let mut pane = stub("/x/y");

    // Narrow first: two columns draw no ancestor, so no parent peek goes out.
    sync(&mut area, &pane, &mut producers);
    let mut log = drain_asks(&mut area);
    let label = "base at width 800 already asks /x, want no ancestor ask";
    c.ok(108, label, !asked(&log, "/x"));

    // The width step alone asks the parent; the cursor never moved.
    area.resize(1100., 600.);
    sync(&mut area, &pane, &mut producers);
    log.extend(drain_asks(&mut area));
    c.ok(
        117,
        "width 800 to 1100 asks no parent peek, want /x",
        asked(&log, "/x"),
    );
    answer(&mut area, &pane, &mut producers, "/x", &[], "");
    drain_asks(&mut area);

    // An empty path asks nothing, so the next step names its own parent.
    land(&mut area, &mut pane, &mut producers, "");
    c.eq(
        128,
        "empty path asks, want nothing",
        drain_asks(&mut area),
        none.clone(),
    );

    // The path step alone asks the parent; the cursor never moved.
    land(&mut area, &mut pane, &mut producers, "/x/y");
    let log = drain_asks(&mut area);
    c.ok(
        137,
        "path empty to /x/y asks no parent peek, want /x",
        asked(&log, "/x"),
    );
    answer(&mut area, &pane, &mut producers, "/x", &[], "");
    drain_asks(&mut area);

    // At the root no ancestor column is shown, so no peek goes out.
    land(&mut area, &mut pane, &mut producers, "/");
    c.eq(
        148,
        "path /x/y to / asks, want nothing",
        drain_asks(&mut area),
        none.clone(),
    );

    // A hidden view asks nothing, though the width shows the parent and the path names one.
    area.resize(1100., 600.);
    set_visible(&mut area, &pane, &mut producers, false);
    drain_asks(&mut area);
    land(&mut area, &mut pane, &mut producers, "/p/q");
    c.eq(
        160,
        "hidden view asks, want nothing",
        drain_asks(&mut area),
        none,
    );

    // Becoming visible re-asks the shown ancestors fresh from the path and width now in force.
    set_visible(&mut area, &pane, &mut producers, true);
    let log = drain_asks(&mut area);
    c.ok(169, "visible again asks, want /p", asked(&log, "/p"));
    c.finish();
}

/// Every ask the outbox holds as its path and "by/desc", the stub backend's peekLog form: "/0" for name order.
fn drain_orders(area: &mut Area) -> Vec<(String, String)> {
    let lines = std::mem::take(&mut area.outbox);
    lines
        .iter()
        .map(|line| {
            let doc = flea::jsondoc::parse(line).expect("peek line is JSON");
            let text = |key: &str| {
                doc.get(key)
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_owned()
            };
            let desc = doc.get("desc").and_then(|value| value.as_bool()) == Some(true);
            (text("path"), format!("{}/{}", text("by"), u8::from(desc)))
        })
        .collect()
}

fn asks_of(log: &[(String, String)], path: &str) -> Vec<String> {
    let of = log.iter().filter(|(asked, _)| asked == path);
    of.map(|(_, order)| order.clone()).collect()
}

/// ViewState.state as another window's sort or the Settings switch leaves it, read the way FolderSorts.peekOrder reads it.
fn write_state(pane: &mut Pane, state: &str) {
    let doc = flea::jsondoc::parse(state).expect("state is JSON");
    let folders = Folders::from_settings(&doc, Vec::new());
    pane.orders.clear();
    for path in ["/x", "/x/y", "/elsewhere"] {
        if let Some(order) = folders.peek_order(path) {
            pane.orders.insert(path.to_owned(), order);
        }
    }
}

/// area.rowsFor's names: the landing's stand-in first, else the rows the store holds.
fn drawn(area: &Area, pane: &Pane, path: &str) -> String {
    let key = peek_key(path, pane.hidden, pane.hidden_last);
    let held = area
        .peeks
        .rows(&key)
        .cloned()
        .unwrap_or_else(|| Rc::from([]));
    join_names(&area.landing.rows(path, &pane.here, held))
}

#[test]
fn an_order_change_re_asks_its_column_and_drops_the_reply_sorted_before_it() {
    let mut c = Checks::new("tests/columnspeekorder.qml");
    // The failure list, the phase timers, the early report and the PASS or FAIL lines are this test's steps and finish.
    c.covers(&[93, 122, 124, 125, 126, 132, 174, 179]);
    let mut area = Area::new(1100., 600., None, 0., SETTINGS);
    let mut producers = Producers::default();
    let mut pane = stub("/x/y");

    // The parent column's first ask goes out in name order and stays out while /x turns to size order.
    sync(&mut area, &pane, &mut producers);
    let mut log = drain_orders(&mut area);
    let first = vec!["/0".to_owned()];
    c.eq(
        131,
        "the first ask of /x is one name-order ask",
        asks_of(&log, "/x"),
        first,
    );
    let sized = r#"{"folderSorts":{"/x":{"key":"size","reverse":true}},"rememberSort":true}"#;
    write_state(&mut pane, sized);
    pane.with(|view| area.orders_changed(&view.lens));

    // The order change re-asks /x, the name-order reply landing after it drops, and the size-order reply is drawn.
    log.extend(drain_orders(&mut area));
    let label =
        "/x turned to size order with the path unchanged and its column asked, want a size/1 ask";
    c.ok(
        140,
        label,
        asks_of(&log, "/x").contains(&"size/1".to_owned()),
    );
    answer(&mut area, &pane, &mut producers, "/x", &["a", "b"], "");
    let label = "a name-order reply that lands after /x turned to size order is drawn as /x";
    c.ok(143, label, drawn(&area, &pane, "/x") != "a|b");
    answer(
        &mut area,
        &pane,
        &mut producers,
        "/x",
        &["b", "a"],
        r#","by":"size","desc":true"#,
    );
    let label = "the size-order reply of /x is not drawn, the column shows";
    c.eq(146, label, drawn(&area, &pane, "/x"), "b|a".to_owned());
    drain_orders(&mut area);
    let forget = r#"{"folderSorts":{"/x":{"key":"size","reverse":true}},"rememberSort":false}"#;
    write_state(&mut pane, forget);
    pane.with(|view| area.orders_changed(&view.lens));

    // Turning remembering off re-asks /x in name order, and the size-order rows stay drawn until that reply lands.
    let log = drain_orders(&mut area);
    let label = "turning folder sort memory off left /x in its remembered order, the column asked, want a name-order ask";
    c.ok(156, label, asks_of(&log, "/x").contains(&"/0".to_owned()));
    let label = "/x shows, while its name-order re-ask is out, want the size-order rows held";
    c.eq(158, label, drawn(&area, &pane, "/x"), "b|a".to_owned());
    answer(&mut area, &pane, &mut producers, "/x", &["a", "b"], "");
    let label = "the name-order reply of /x is not drawn, the column shows";
    c.eq(161, label, drawn(&area, &pane, "/x"), "a|b".to_owned());
    drain_orders(&mut area);
    let elsewhere = r#"{"folderSorts":{"/elsewhere":{"key":"size","reverse":false},"/x/y":{"key":"mtime","reverse":true}},"rememberSort":true}"#;
    write_state(&mut pane, elsewhere);
    pane.with(|view| area.orders_changed(&view.lens));

    // A sort written for a folder no column shows, or for the listed folder itself, asks no peek.
    let log = drain_orders(&mut area);
    let asks: Vec<String> = log
        .iter()
        .map(|(path, order)| format!("{path} {order}"))
        .collect();
    let label = "a sort written for folders no column shows asked, want nothing";
    c.eq(171, label, asks, Vec::new());
    c.finish();
}
