use super::Observations;

fn assert_open(observed: &Observations, early: bool, landed: bool) {
    let state = flea::jsondoc::parse(&observed.open_json(true)).unwrap();
    assert_eq!(
        state
            .get("revealedEarly")
            .and_then(flea::jsondoc::Json::as_bool),
        Some(early)
    );
    assert_eq!(
        state.get("landed").and_then(flea::jsondoc::Json::as_bool),
        Some(landed)
    );
}

#[test]
fn early_reveal_remains_observable_after_landing_until_close_or_replacement() {
    let mut observed = Observations::default();
    assert_open(&observed, false, false);
    observed.opened(10, false);
    assert_open(&observed, false, false);
    observed.revealed(30, false, false, true);
    assert_open(&observed, true, false);
    observed.revealed(70, true, true, false);
    assert_open(&observed, true, true);
    assert_eq!(observed.reveal_caps, 1);
    assert_eq!(observed.swap.shown_ms, 40);
    observed.closed(80);
    assert_open(&observed, false, false);
    observed.opened(90, true);
    assert_open(&observed, false, true);
    observed.opened(100, false);
    assert_open(&observed, false, false);
}

#[test]
fn landing_observation_survives_a_later_unready_reveal() {
    let mut observed = Observations::default();
    observed.opened(0, true);
    observed.revealed(10, false, false, true);
    assert_open(&observed, true, true);
    observed.revealed(20, true, true, false);
    assert_open(&observed, true, true);
}

#[test]
fn copy_and_close_hover_remain_independent_of_reveal_lifecycle() {
    let mut observed = Observations::default();
    observed.opened(0, false);
    for (copy, close) in [(false, false), (true, false), (false, true), (true, true)] {
        observed.copy_hovered = copy;
        observed.close_hovered = close;
        observed.revealed(10, false, false, true);
        assert_eq!(
            (observed.copy_hovered, observed.close_hovered),
            (copy, close)
        );
        observed.revealed(20, true, true, false);
        assert_eq!(
            (observed.copy_hovered, observed.close_hovered),
            (copy, close)
        );
    }
    observed.closed(30);
    assert!(!observed.copy_hovered && !observed.close_hovered);
    observed.copy_hovered = true;
    observed.close_hovered = true;
    observed.opened(40, true);
    assert!(!observed.copy_hovered && !observed.close_hovered);
}

#[test]
fn drawable_interim_does_not_land_or_count_as_an_early_reveal() {
    let mut observed = Observations::default();
    observed.opened(0, false);
    observed.revealed(10, true, false, false);
    assert_open(&observed, false, false);
    assert!(!observed.swap.holding);
    observed.revealed(20, true, true, false);
    assert_open(&observed, false, true);
    observed.revealed(30, true, false, false);
    assert_open(&observed, false, true);
    assert_eq!(observed.reveal_caps, 0);
}
