use crate::tui::{
    actions, input::Key, keymap::Map, model::Model, render::frame_tests::model, wire,
};

// A 1,000-line text preview with the keys on it, in the given preset.
fn focused(preset: &str) -> Model {
    let mut m = model();
    m.preset = preset.into();
    m.preview_body = (1..=1000).map(|n| format!("line {n}\n")).collect();
    crate::tui::layout::prepare(&mut m);
    m.preview_focus = true;
    m
}
fn press(m: &mut Model, keys: &[Key], w: &mut wire::Wire) {
    for key in keys {
        actions::key(m, key, &Map::load(), w).unwrap();
        crate::tui::layout::prepare(m);
    }
}

#[test]
fn a_focused_text_preview_pages_and_jumps_with_the_listing_keys() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = focused("default");
    let page = crate::tui::pane::rows(&m);
    let limit = crate::tui::layout::preview_scroll_limit(&m);
    assert!(page > 1 && limit > 2 * page, "page {page}, limit {limit}");
    press(&mut m, &[Key::named("PageDown", "")], &mut w);
    assert_eq!(m.preview_scroll, page, "PageDown moves one page");
    press(
        &mut m,
        &[Key::named("PageDown", ""), Key::named("PageUp", "")],
        &mut w,
    );
    assert_eq!(m.preview_scroll, page);
    press(&mut m, &[Key::named("End", "")], &mut w);
    assert_eq!(m.preview_scroll, limit, "End reaches the last line");
    press(&mut m, &[Key::named("PageDown", "")], &mut w);
    assert_eq!(
        m.preview_scroll, limit,
        "a page past the end stays at the end"
    );
    press(&mut m, &[Key::named("Home", "")], &mut w);
    assert_eq!(m.preview_scroll, 0);
    press(&mut m, &[Key::character('G', "")], &mut w);
    assert_eq!(m.preview_scroll, limit, "G is the listing's last row");
    press(&mut m, &[Key::character('g', "")], &mut w);
    assert_eq!(m.preview_scroll, 0, "g is the listing's first row");
    assert!(m.preview_focus && !m.quit);
    wire::finish(w, reader);
}

#[test]
fn tab_space_and_q_answer_in_a_focused_text_preview() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = focused("default");
    press(
        &mut m,
        &[Key::named("Tab", ""), Key::named("Tab", "shift")],
        &mut w,
    );
    assert!(
        m.preview_focus,
        "plain Tab and Shift+Tab traverse a preview's controls and stay, as parity.tsv binds them"
    );
    press(&mut m, &[Key::named("Tab", "ctrl")], &mut w);
    assert!(
        !m.preview_focus,
        "Ctrl+Tab hands the keys back to the listing"
    );
    m.preview_focus = true;
    press(&mut m, &[Key::character(' ', "")], &mut w);
    assert!(m.quicklook, "Space opens Quick Look on the previewed file");
    press(&mut m, &[Key::character(' ', "")], &mut w);
    assert!(
        !m.quicklook && m.preview_focus,
        "Space closes it again and the preview keeps the keys"
    );
    press(&mut m, &[Key::character('q', "")], &mut w);
    assert!(
        m.quit,
        "q quits from a focused preview as it does from the listing"
    );
    wire::finish(w, reader);
}

#[test]
fn prefixed_and_armed_jumps_work_in_a_focused_text_preview() {
    let (mut w, reader) = wire::echo_wire();
    for preset in ["vim", "yazi"] {
        let mut m = focused(preset);
        let limit = crate::tui::layout::preview_scroll_limit(&m);
        press(&mut m, &[Key::character('G', "")], &mut w);
        assert_eq!(m.preview_scroll, limit, "{preset}: G");
        press(&mut m, &[Key::character('g', "")], &mut w);
        assert_eq!(m.preview_scroll, limit, "{preset}: a lone g only arms");
        press(&mut m, &[Key::character('g', "")], &mut w);
        assert_eq!(m.preview_scroll, 0, "{preset}: g g");
        assert!(m.preview_focus, "{preset}: the preview keeps the keys");
    }
    wire::finish(w, reader);
}
