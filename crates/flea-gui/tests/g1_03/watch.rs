use super::*;
use flea_gui::navigation::watch::{Gate, Watch, WATCH_MS};

#[test]
fn a_continuous_burst_is_absorbed_into_one_reread_per_deadline() {
    let mut watch = Watch::default();
    let mut fired = Vec::new();
    for at in (0..1000).step_by(50) {
        watch.changed("/f/base", "/f/base", at);
        if at < WATCH_MS {
            assert_eq!(
                watch.due,
                Some(WATCH_MS),
                "later notices never restart the deadline"
            );
        }
        if watch.fire(at, false) {
            fired.push(at);
        }
    }
    assert_eq!(
        fired,
        [WATCH_MS, 450 + WATCH_MS],
        "one re-read per absorbing deadline"
    );
    assert_eq!((watch.notices, watch.rereads), (20, 2));
    assert!(watch.stale);
    assert_eq!(watch.due, Some(900 + WATCH_MS));
}

#[test]
fn a_foreign_path_is_dropped_and_leaving_the_folder_drops_the_debt() {
    let mut watch = Watch::default();
    assert!(!watch.changed("/f/other", "/f/base", 0));
    assert_eq!((watch.stale, watch.due), (false, None));
    watch.changed("/f/base", "/f/base", 0);
    watch.clear();
    assert!(!watch.fire(1000, false));
    watch.settle(1000, false);
    assert_eq!(watch.due, None);
}

#[test]
fn a_held_press_defers_the_reread_and_its_release_pays_it_once() {
    let (mut pane, mut nav, sel) = settled("/f/base", 40);
    nav.gate = Gate {
        pointer_held: true,
        ..Gate::default()
    };
    assert!(nav.changed(&pane, "/f/base", 0));
    assert!(!nav.fire_watch(&mut pane, &sel, WATCH_MS));
    assert!(
        nav.watch.stale && nav.watch.due.is_none(),
        "the debt is kept, not dropped"
    );
    assert!(nav.take().is_empty());
    for owned in [
        Gate {
            menu_open: true,
            ..Gate::default()
        },
        Gate {
            editing: true,
            ..Gate::default()
        },
        Gate {
            dragging: true,
            ..Gate::default()
        },
        Gate {
            filter_typing: true,
            ..Gate::default()
        },
        Gate {
            selection_band: true,
            ..Gate::default()
        },
    ] {
        assert!(owned.owned());
    }
    nav.gate = Gate::default();
    nav.settle(&pane, 1000);
    assert_eq!(nav.watch.due, Some(1000 + WATCH_MS));
    assert!(nav.fire_watch(&mut pane, &sel, 1000 + WATCH_MS));
    assert_eq!(lists(&nav.take()), ["/f/base"]);
    assert_eq!(nav.watch.rereads, 1);
}

#[test]
fn a_debt_owed_by_the_folder_left_costs_the_folder_arrived_in_no_listing() {
    let (mut pane, mut nav, mut sel) = settled("/f/a", 40);
    nav.changed(&pane, "/f/a", 0);
    nav.open(&mut pane, "/f/b").unwrap();
    assert!(
        !nav.fire_watch(&mut pane, &sel, WATCH_MS),
        "a listing out holds the re-read"
    );
    assert!(land(
        &mut pane,
        &mut nav,
        &mut sel,
        ("/f/b", 4, 0, 2),
        named
    ));
    assert!(!nav.watch.stale);
    assert!(!nav.fire_watch(&mut pane, &sel, 10_000));
    assert_eq!(
        lists(&nav.take()),
        ["/f/b"],
        "only the navigation itself listed"
    );
}

#[test]
fn an_in_place_reread_keeps_the_rows_drawn_until_its_pair_lands() {
    let (mut pane, mut nav, mut sel) = settled("/f/base", 40);
    nav.changed(&pane, "/f/base", 0);
    assert!(nav.fire_watch(&mut pane, &sel, WATCH_MS));
    assert!(pane.swap.holding);
    assert_eq!(pane.held.total, 40, "held rows stay drawn");
    let Command::List(listing) = &nav.take()[0] else {
        panic!()
    };
    assert!(!listing.want_changed);
    assert!(land(
        &mut pane,
        &mut nav,
        &mut sel,
        ("/f/base", 41, 0, 2),
        named
    ));
    assert_eq!(pane.held.total, 41);
}
