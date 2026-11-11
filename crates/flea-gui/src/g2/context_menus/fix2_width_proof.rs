#[gpui::test]
fn rev_u06_r2_wrapped_row_width_matches_cardsize_observer(cx: &mut TestAppContext) {
    let (menu, _, state) = rows_with_every_trailing_mix(cx);
    let state = jsondoc::parse(state.trim()).unwrap();
    let rows = state.get("rows").and_then(Json::as_array).unwrap();
    let needed = rows
        .iter()
        .filter(|row| row.get("separator").and_then(Json::as_bool) != Some(true))
        .map(parts_sum)
        .fold(0., f64::max);
    let menu = menu.borrow();
    let card = menu.card.as_ref().unwrap();
    let expected = f64::from(flea::ui_logic_context_menus::fit::MENU_W)
        .max(needed)
        .min(f64::from(WIDE.w))
        .round();
    println!(
        "R2_CARDSIZES observed={} token={} needed={} expected={expected}",
        card.frame.w,
        flea::ui_logic_context_menus::fit::MENU_W,
        needed
    );
    assert!(
        (f64::from(card.frame.w) - expected).abs() <= 1.,
        "tests/ui-card-layout.sh:231-233 width formula fails on wrapped Extract row"
    );
}
