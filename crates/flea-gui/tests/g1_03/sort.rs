use super::*;
use flea_gui::ipc::{navigation::NavigationAdapter, Surface};
use flea_gui::navigation::{
    folder_state::Order,
    sort::{self, DirSizes, Size},
    watch::Gate,
    Refusal,
};

fn order(key: &'static str, desc: bool) -> Order {
    Order { key, desc }
}

#[test]
fn a_resort_asks_the_backend_once_records_the_folder_and_lands_the_reordered_first_window() {
    let (mut pane, mut nav, mut sel) = settled("/f/base", 1000);
    pane.cursor = 40;
    sel.push(40);
    let fail = String::new;
    assert_eq!(
        nav.sort_column(&mut pane, "size", &mut sel, fail()),
        Ok(true)
    );
    assert_eq!(
        nav.take(),
        [
            Command::Sort {
                order: order("size", false),
                folders_first: true,
                group_by_kind: false,
                hidden_last: false
            },
            Command::Persist(
                r#"{"folderSorts":{"/f/base":{"key":"size","reverse":false}}}"#.into()
            ),
            Command::Window {
                start: 0,
                count: 320
            },
        ]
    );
    assert_eq!(
        (pane.cursor, sel.len(), nav.sort_mark()),
        (40, 0, "size:asc".into())
    );
    assert_eq!(nav.writer.borrow().sent, 1);
    let now = pane.now();
    pane.receive_at(
        Reply::Listed {
            path: "/f/base".into(),
            total: 1000,
        },
        now,
    )
    .unwrap();
    assert!(pane
        .receive_at(
            Reply::Rows {
                start: 0,
                rows: rows(0, 320, named),
                listing: 2
            },
            now
        )
        .unwrap());
    nav.after_reply(&mut pane, "/f/base", &mut sel);
    assert_eq!((pane.held.listing, pane.path()), (2, "/f/base"));
    assert_eq!(
        nav.sort_column(&mut pane, "size", &mut sel, fail()),
        Ok(true),
        "the sorted column reverses"
    );
    assert_eq!(nav.sort_mark(), "size:desc");
    nav.take();
    assert_eq!(nav.sort_reverse(&mut pane, &mut sel, fail()), Ok(true));
    assert_eq!(nav.sort_mark(), "size:asc");
    assert_eq!(nav.sort_next(&mut pane, &mut sel, fail()), Ok(true));
    assert_eq!(nav.sort_mark(), "mtime:asc");
    assert_eq!(
        nav.sort_column(&mut pane, "mode", &mut sel, fail()),
        Ok(false),
        "an unsupported column stays a label"
    );
    let commands = nav.take();
    assert!(
        nav.resort(&mut pane, order("mtime", false), &mut sel, fail()) == Ok(false),
        "the order already in force is not asked for"
    );
    assert!(nav.take().is_empty());
    assert!(
        commands
            .iter()
            .any(|command| matches!(command, Command::Persist(p) if p.contains("\"date\""))),
        "mtime is stored as date"
    );
}

#[test]
fn a_sort_waits_for_an_edit_and_is_refused_while_a_listing_is_out() {
    let (mut pane, mut nav, mut sel) = settled("/f/base", 40);
    nav.gate = Gate {
        editing: true,
        ..Gate::default()
    };
    assert_eq!(nav.sort_next(&mut pane, &mut sel, String::new()), Ok(false));
    assert!(nav.take().is_empty());
    nav.gate = Gate::default();
    assert_eq!(
        nav.apply_pending_sort(&mut pane, &mut sel, String::new()),
        Ok(true)
    );
    assert_eq!(nav.sort_mark(), "size:asc");
    nav.open(&mut pane, "/f/x").unwrap();
    assert_eq!(
        nav.sort_next(&mut pane, &mut sel, String::new()),
        Err(Refusal::Busy)
    );
}

#[test]
fn a_remembered_folder_relists_in_its_order_and_forgetting_returns_to_the_default() {
    let (mut pane, mut nav, mut sel) = settled("/f/base", 40);
    nav.sort_column(&mut pane, "kind", &mut sel, String::new())
        .unwrap();
    nav.open(&mut pane, "/f/x").unwrap();
    land(&mut pane, &mut nav, &mut sel, ("/f/x", 4, 0, 2), named);
    assert_eq!(nav.sort_mark(), "name:asc");
    nav.back(&mut pane).unwrap();
    let Command::List(listing) = nav.take().pop().unwrap() else {
        panic!()
    };
    assert_eq!(
        listing.order,
        order("kind", false),
        "the folder keeps its order across navigation"
    );
    land(&mut pane, &mut nav, &mut sel, ("/f/base", 40, 0, 3), named);
    nav.changed(&pane, "/f/base", 0);
    assert!(nav.fire_watch(&mut pane, &sel, 400));
    let Command::List(listing) = nav.take().pop().unwrap() else {
        panic!()
    };
    assert_eq!(listing.order.key, "kind", "a watched re-read keeps it");
    land(&mut pane, &mut nav, &mut sel, ("/f/base", 40, 0, 4), named);
    nav.forget_sort(&mut pane, String::new()).unwrap();
    let commands = nav.take();
    assert_eq!(
        commands[0],
        Command::Persist(r#"{"folderSorts":{"/f/base":null}}"#.into())
    );
    let Command::List(listing) = &commands[1] else {
        panic!()
    };
    assert_eq!(listing.order, Order::default());
}

#[test]
fn the_sort_mark_is_served_over_ipc_and_titles_draw_the_direction() {
    let (_, mut nav, _) = settled("/f/base", 4);
    nav.order = order("size", true);
    let adapter = NavigationAdapter { navigator: &nav };
    let surface = Surface {
        path: "/f/base",
        total: 4,
        cursor: 0,
        selected: &[],
    };
    let answer = surface.dispatch_to(
        flea::ipc::wire::Request::Call {
            target: "flea".into(),
            function: "sortMark".into(),
            arguments: vec![],
        },
        &[&adapter],
    );
    assert_eq!(answer.reply.stdout, "size:desc\n");
    assert_eq!(sort::title("Size", true), "Size \u{25be}");
    assert_eq!(sort::title("Name", false), "Name \u{25b4}");
}

#[test]
fn folder_sizes_ask_only_unasked_visible_folders_and_follow_their_rows_by_name() {
    let mut listing = rows(0, 6, named);
    for row in [1, 3, 4] {
        listing[row].dir = true;
    }
    let mut sizes = DirSizes::default();
    assert_eq!(sizes.plan(&listing, 0, 0, 5), [1, 3, 4]);
    sizes.applied(&[1, 3]);
    assert!(sizes.has_pending());
    assert_eq!(sizes.plan(&listing, 0, 0, 5), [4]);
    sizes.remember(
        1,
        Size {
            bytes: 96,
            partial: false,
        },
        64,
    );
    sizes.cancelled();
    assert!(!sizes.has_pending());
    assert_eq!(sizes.len(), 1);
    let mut moved = listing.clone();
    moved.insert(
        0,
        RowFacts {
            name: "a-new".into(),
            ..RowFacts::default()
        },
    );
    let carried = sizes.carried(&listing, 0, &moved, 0);
    assert_eq!(
        carried.size_for(2),
        Some(Size {
            bytes: 96,
            partial: false
        })
    );
    assert_eq!(carried.size_for(1), None);
    for row in 0..10 {
        sizes.remember(
            row,
            Size {
                bytes: row as u64,
                partial: true,
            },
            4,
        );
    }
    assert_eq!(sizes.len(), 4, "the cap bounds a policy bug");
    assert!(sort::wants_sizes("list", &[], "local", true));
    assert!(!sort::wants_sizes("list", &["size"], "local", true));
    assert!(!sort::wants_sizes("grid", &[], "local", true));
    assert!(!sort::wants_sizes("columns", &[], "network", true));
    assert!(!sort::wants_sizes("columns", &[], "local", false));
}

#[test]
fn a_sort_after_a_resize_lands_its_reordered_window_at_the_count_it_asked_for() {
    let (mut pane, mut nav, mut sel) = settled("/f/base", 1000);
    pane.set_height(900.);
    assert_eq!(nav.sort_next(&mut pane, &mut sel, String::new()), Ok(true));
    let count = nav
        .take()
        .iter()
        .find_map(|command| match command {
            Command::Window { count, .. } => Some(*count),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        count,
        pane.first_count(),
        "the window follows the live height"
    );
    let now = pane.now();
    pane.receive_at(
        Reply::Listed {
            path: "/f/base".into(),
            total: 1000,
        },
        now,
    )
    .unwrap();
    let landed = pane.receive_at(
        Reply::Rows {
            start: 0,
            rows: rows(0, count, named),
            listing: 2,
        },
        now,
    );
    assert_eq!(landed, Ok(true), "window count {count}");
}

#[test]
fn two_sorts_across_a_resize_each_land_at_the_count_they_asked_for() {
    let window_count = |nav: &mut Navigator| {
        nav.take()
            .iter()
            .find_map(|command| match command {
                Command::Window { count, .. } => Some(*count),
                _ => None,
            })
            .unwrap()
    };
    let (mut pane, mut nav, mut sel) = settled("/f/base", 1000);
    assert_eq!(nav.sort_next(&mut pane, &mut sel, String::new()), Ok(true));
    let first = window_count(&mut nav);
    pane.set_height(900.);
    assert_eq!(nav.sort_next(&mut pane, &mut sel, String::new()), Ok(true));
    let second = window_count(&mut nav);
    assert_ne!(first, second, "the height change moved the count");
    let now = pane.now();
    pane.receive_at(
        Reply::Listed {
            path: "/f/base".into(),
            total: 1000,
        },
        now,
    )
    .unwrap();
    let landed = pane.receive_at(
        Reply::Rows {
            start: 0,
            rows: rows(0, first, named),
            listing: 2,
        },
        now,
    );
    assert_eq!(
        landed,
        Ok(true),
        "first sort asked {first}, second asked {second}"
    );
    pane.receive_at(
        Reply::Listed {
            path: "/f/base".into(),
            total: 1000,
        },
        now,
    )
    .unwrap();
    let landed = pane.receive_at(
        Reply::Rows {
            start: 0,
            rows: rows(0, second, named),
            listing: 3,
        },
        now,
    );
    assert_eq!(landed, Ok(true), "second sort asked {second}");
}

#[test]
fn a_refused_sort_frees_its_count_so_the_next_sort_lands_at_its_own() {
    let (mut pane, mut nav, mut sel) = settled("/f/base", 1000);
    assert_eq!(nav.sort_next(&mut pane, &mut sel, String::new()), Ok(true));
    nav.take();
    let now = pane.now();
    assert!(!pane.fail("sort", "/f/base", "refused", 0, now));
    pane.set_height(900.);
    assert_eq!(nav.sort_next(&mut pane, &mut sel, String::new()), Ok(true));
    let count = pane.first_count();
    pane.receive_at(
        Reply::Listed {
            path: "/f/base".into(),
            total: 1000,
        },
        now,
    )
    .unwrap();
    let landed = pane.receive_at(
        Reply::Rows {
            start: 0,
            rows: rows(0, count, named),
            listing: 2,
        },
        now,
    );
    assert_eq!(landed, Ok(true), "sort asked {count}");
}

// A remote window stat that times out answers an error naming window and the listed path, and no rows.
#[test]
fn a_sort_whose_window_failed_leaves_no_count_for_the_next_sort() {
    let (mut pane, mut nav, mut sel) = settled("/f/base", 1000);
    assert_eq!(nav.sort_next(&mut pane, &mut sel, String::new()), Ok(true));
    let first = window_count(&mut nav);
    let now = pane.now();
    pane.receive_at(
        Reply::Listed {
            path: "/f/base".into(),
            total: 1000,
        },
        now,
    )
    .unwrap();
    assert!(!pane.fail("window", "/f/base", "timed out", 0, now));
    nav.after_failure(&mut pane, "window");
    pane.set_height(900.);
    assert_eq!(nav.sort_next(&mut pane, &mut sel, String::new()), Ok(true));
    let second = window_count(&mut nav);
    assert_ne!(first, second, "the height change moved the count");
    pane.receive_at(
        Reply::Listed {
            path: "/f/base".into(),
            total: 1000,
        },
        now,
    )
    .unwrap();
    let landed = pane.receive_at(
        Reply::Rows {
            start: 0,
            rows: rows(0, second, named),
            listing: 3,
        },
        now,
    );
    assert_eq!(
        landed,
        Ok(true),
        "first sort asked {first}, second asked {second}"
    );
}

#[test]
fn a_failed_sort_window_leaves_each_later_sort_its_own_count() {
    let (mut pane, mut nav, mut sel) = settled("/f/base", 1000);
    assert_eq!(nav.sort_next(&mut pane, &mut sel, String::new()), Ok(true));
    window_count(&mut nav);
    let now = pane.now();
    pane.receive_at(
        Reply::Listed {
            path: "/f/base".into(),
            total: 1000,
        },
        now,
    )
    .unwrap();
    assert!(!pane.fail("window", "/f/base", "timed out", 0, now));
    nav.after_failure(&mut pane, "window");
    assert_eq!(nav.sort_next(&mut pane, &mut sel, String::new()), Ok(true));
    let second = window_count(&mut nav);
    pane.receive_at(
        Reply::Listed {
            path: "/f/base".into(),
            total: 1000,
        },
        now,
    )
    .unwrap();
    let landed = pane.receive_at(
        Reply::Rows {
            start: 0,
            rows: rows(0, second, named),
            listing: 3,
        },
        now,
    );
    assert_eq!(
        landed,
        Ok(true),
        "the second sort lands at an unchanged height"
    );
    nav.after_reply(&mut pane, "/f/base", &mut sel);
    pane.set_height(900.);
    assert_eq!(nav.sort_next(&mut pane, &mut sel, String::new()), Ok(true));
    let third = window_count(&mut nav);
    pane.receive_at(
        Reply::Listed {
            path: "/f/base".into(),
            total: 1000,
        },
        now,
    )
    .unwrap();
    let landed = pane.receive_at(
        Reply::Rows {
            start: 0,
            rows: rows(0, third, named),
            listing: 4,
        },
        now,
    );
    assert_eq!(
        landed,
        Ok(true),
        "second sort asked {second}, third asked {third}"
    );
}
