use crate::tui::{
    action_key_tests::listing,
    contents_tests::{enter, land},
    layout,
    render::frame_tests::plain,
    theme::Theme,
    wire,
};
use std::time::Duration;

// While Find's results are shown, a receipt or refusal takes the query's place in the footer until it clears.
#[test]
fn a_message_during_find_results_is_drawn_in_the_footer() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "default");
    let gen = enter(&mut m, &mut w, "needle");
    land(&mut m, &mut w, gen, 51, &["one.txt", "two.txt"]);
    let theme = Theme::from_text("");
    assert!(plain(&layout::footer(&m, &theme, 120, Duration::ZERO)).contains("Find “needle”"));
    m.say_key("tui.actions.only_a_folder_opens_in_a_new_tab");
    let footer = plain(&layout::footer(&m, &theme, 120, Duration::ZERO));
    assert!(
        footer.contains("Only a folder opens in a new tab"),
        "{footer}"
    );
    assert!(!footer.contains("Find “needle”"), "{footer}");
    m.message.clear();
    assert!(plain(&layout::footer(&m, &theme, 120, Duration::ZERO)).contains("Find “needle”"));
    wire::finish(w, reader);
}
