//! tests/js/columnsback.js runPlan: the plan a navigation carries from the listing it leaves.
use super::check::Checks;
use super::columns_back::{leaving, list_rows, paths, DIR_FACTS, SOURCE, STATE};
use flea::protocol::RowFacts;
use flea_gui::columns::carry::{is_back, Leaving, ListOrder, Plan};
use flea_gui::columns::peeks::{Facts, PeekRow};

/// The seed a plan carries, as its row count, None when it seeds nothing.
fn seeds(pane: Leaving<'_>, asks: &[&str]) -> Option<usize> {
    Plan::new(&pane, &paths(asks)).and_then(|plan| plan.rows.map(|rows| rows.len()))
}

#[test]
fn a_plan_carries_the_clean_window_left() {
    let mut c = Checks::new(SOURCE);
    c.covers(&[8]);
    c.eq(
        53,
        "a Back names the parent of the shown path",
        is_back("/a/b/c", "/a/b"),
        true,
    );
    c.eq(
        54,
        "a Back from a trailing slash names it too",
        is_back("/a/b/c/", "/a/b"),
        true,
    );
    c.eq(
        55,
        "an Enter names no Back",
        is_back("/a/b", "/a/b/c"),
        false,
    );
    c.eq(
        56,
        "a rail hop two up names no Back",
        is_back("/a/b/c", "/a"),
        false,
    );
    c.eq(
        57,
        "staying at the root names no Back",
        is_back("/", "/"),
        false,
    );
    let rows = list_rows("c", 40);
    let base = leaving(&rows);
    let enter = Leaving {
        target: "/a/b/c/c0",
        ..base
    };
    c.eq(
        58,
        "an Enter carries the listing into its new ancestor slot",
        Plan::new(&enter, &paths(&["/a/b"])).map(|plan| plan.seed),
        Some("/a/b/c".to_owned()),
    );
    let plan = Plan::new(&base, &paths(&["/a"])).expect("a plan from a listing");
    c.eq(
        60,
        "a Back asks the incoming parent alone",
        plan.pre.clone(),
        paths(&["/a"]),
    );
    c.eq(
        61,
        "and carries the folder left as the seed",
        format!("{}>{}", plan.seed, plan.target),
        "/a/b/c>/a/b".to_owned(),
    );
    let seed = plan
        .rows
        .clone()
        .map(|rows| rows.to_vec())
        .unwrap_or_default();
    c.eq(
        62,
        "the seed is the first window, cut to the peek's own count",
        seed.len(),
        35,
    );
    let first = PeekRow {
        name: "c0".into(),
        dir: true,
        icon: "folder".into(),
    };
    c.eq(
        63,
        "in the peek's own shape, n, d and i alone",
        seed.first().cloned(),
        Some(first),
    );
    c.eq(
        64,
        "with the folder's device and writability",
        plan.facts,
        DIR_FACTS,
    );
    let short = list_rows("c", 3);
    let short_pane = Leaving {
        total: 3,
        ..leaving(&short)
    };
    c.eq(
        65,
        "a listing shorter than a peek's count carries all of it",
        seeds(short_pane, &["/a"]),
        Some(3),
    );
    c.eq(
        66,
        "a 4-column Back asks the two ancestors not yet shown",
        Plan::new(&base, &paths(&["/", "/a"])).map(|plan| plan.pre),
        Some(paths(&["/", "/a"])),
    );
    let scrolled = Leaving {
        held_start: 40,
        ..base
    };
    c.eq(
        67,
        "a scrolled window seeds nothing",
        seeds(scrolled, &["/a"]),
        None,
    );
    let narrowed = Leaving {
        narrowed: true,
        ..base
    };
    c.eq(
        68,
        "a filtered window seeds nothing",
        seeds(narrowed, &["/a"]),
        None,
    );
    c.eq(
        69,
        "a searched window seeds nothing",
        seeds(narrowed, &["/a"]),
        None,
    );
    let twenty = list_rows("c", 20);
    c.eq(
        70,
        "a window short of the peek's count seeds nothing",
        seeds(leaving(&twenty), &["/a"]),
        None,
    );
    let dated = Leaving {
        order: ListOrder {
            by: "mtime",
            desc: true,
            ..STATE
        },
        facts: Facts {
            dev: 0,
            writable: true,
        },
        ..base
    };
    c.eq(
        71,
        "a date order seeds nothing",
        seeds(dated, &["/a"]),
        None,
    );
    let mixed = Leaving {
        order: ListOrder {
            folders_first: false,
            ..STATE
        },
        ..base
    };
    c.eq(
        72,
        "folders mixed in seed nothing",
        seeds(mixed, &["/a"]),
        None,
    );
    let grouped = Leaving {
        order: ListOrder {
            group_by_kind: true,
            ..STATE
        },
        ..base
    };
    c.eq(
        73,
        "a kind grouping seeds nothing",
        seeds(grouped, &["/a"]),
        None,
    );
    let own = Leaving {
        tag: "size1",
        ..base
    };
    c.eq(
        74,
        "a folder that remembers its own order seeds nothing",
        seeds(own, &["/a"]),
        None,
    );
    let named = Leaving {
        tag: "name0",
        ..base
    };
    c.eq(
        75,
        "a remembered name order still seeds",
        seeds(named, &["/a"]),
        Some(35),
    );
    let mut link_target = list_rows("c", 40);
    link_target[3] = RowFacts {
        name: "ln".into(),
        dir: false,
        mode: 33188,
        icon: "text-x-generic".into(),
        link: Some("../x".into()),
        real_modes: true,
        ..RowFacts::default()
    };
    c.eq(
        78,
        "a window holding a target link seeds nothing",
        seeds(leaving(&link_target), &["/a"]),
        None,
    );
    let mut link_mode = list_rows("c", 40);
    link_mode[5] = RowFacts {
        name: "ln".into(),
        dir: false,
        mode: 41471,
        icon: "text-x-generic".into(),
        real_modes: true,
        ..RowFacts::default()
    };
    c.eq(
        81,
        "a window holding a symlink mode seeds nothing",
        seeds(leaving(&link_mode), &["/a"]),
        None,
    );
    let mut exe = list_rows("c", 40);
    exe[4] = RowFacts {
        name: "run".into(),
        dir: false,
        mode: 33261,
        icon: "application-x-executable".into(),
        real_modes: true,
        ..RowFacts::default()
    };
    c.eq(
        84,
        "a window holding an executable seeds nothing",
        seeds(leaving(&exe), &["/a"]),
        None,
    );
    c.finish();
}
