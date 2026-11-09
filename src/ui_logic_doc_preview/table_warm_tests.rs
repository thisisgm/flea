use super::*;

// tests/quicklook-tablewarm.qml's sequence, counted rather than timed.
#[test]
fn table_warm_builds_once_on_the_first_rested_table() {
    let mut warm = TableWarm::default();
    assert!(
        !warm.rested("/d/note.md", false, false),
        "a rested document with no table never warms"
    );
    assert!(
        warm.rested("/d/t1.md", true, false),
        "the first rested table asks"
    );
    assert!(
        warm.answered("/d/t1.md", "/d/t1.md", false),
        "and warms once"
    );
    assert!(
        !warm.rested("/d/t2.md", true, false),
        "a second rested table does not warm again"
    );
    assert!(
        !warm.rested("/d/note.md", false, false),
        "a later note leaves the warm as it was"
    );
    assert_eq!((warm.warms, warm.warmed), (1, true));
}

#[test]
fn table_warm_never_under_a_held_key_or_after_space() {
    // A held key never calls `rested`; that rule is pv1's join test, so this case starts at the rest after it.
    let mut held = TableWarm::default();
    assert!(
        held.rested("/d/t2.md", true, false),
        "the rest after a held key warms"
    );
    assert!(held.answered("/d/t2.md", "/d/t2.md", false));
    assert_eq!(held.warms, 1);

    let mut opened = TableWarm::default();
    assert!(
        !opened.rested("/d/t1.md", true, true),
        "a table answered after Space never asks for a warm"
    );
    assert_eq!((opened.warms, opened.warmed), (0, false));

    let mut spaced = TableWarm::default();
    assert!(spaced.rested("/d/t1.md", true, false));
    assert!(
        !spaced.answered("/d/t1.md", "/d/t1.md", true),
        "a warm answered after Space builds nothing"
    );
    assert_eq!((spaced.warms, spaced.warmed), (0, false));
}

#[test]
fn table_warm_an_overtaken_warm_leaves_the_next_rested_table_to_warm() {
    let mut warm = TableWarm::default();
    let mut asks = 0;
    asks += usize::from(warm.rested("/d/t2.md", true, false));
    assert!(
        !warm.answered("/d/t2.md", "/d/quote.md", false),
        "the cursor overtook the warm, so its answer is dropped"
    );
    asks += usize::from(warm.rested("/d/quote.md", true, false));
    assert!(warm.answered("/d/quote.md", "/d/quote.md", false));
    assert_eq!((asks, warm.warms, warm.warmed), (2, 1, true));
    assert!(
        !warm.answered("/d/quote.md", "/d/quote.md", false),
        "a repeated answer builds nothing"
    );
}
