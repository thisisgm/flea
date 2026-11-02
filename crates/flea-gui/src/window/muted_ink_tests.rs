//! EY-402, EY-407, EY-408: the toolbar's view marks and the preview column's facts pick v0.3.12's roles on light and dark themes.
use crate::{list::Palette, palette::Theme};

// catppuccin-latte and sunset as Omarchy ships them: the two themes the inks were caught on.
const LATTE: &str = "background='#eff1f5'\nforeground='#4c4f69'\naccent='#1e66f5'\ndark_background='#e3e4e8'\nmuted='#acb0be'";
const SUNSET: &str = "background='#1c120d'\nforeground='#c6b395'\naccent='#d28a41'\ndark_background='#150d0a'\nmuted='#68544b'";

fn themes() -> [(&'static str, Theme, Palette); 2] {
    [("catppuccin-latte", LATTE), ("sunset", SUNSET)].map(|(name, text)| {
        let theme = Theme::from_text(text);
        (
            name,
            theme,
            Palette::inherited(&gpui::TextStyle::default(), theme),
        )
    })
}

/// EY-402: ChromeModes.qml rests the current view's mark in the foreground and the other three in the muted frame.
#[test]
fn ey402_only_the_current_view_mark_rests_in_the_foreground() {
    use crate::path::mark::resting_ink;
    for (name, theme, palette) in themes() {
        assert_ne!(
            theme.muted_frame, theme.metadata_frame,
            "{name}: the fixture needs two inks"
        );
        assert_eq!(
            resting_ink(true, true, &palette),
            theme.foreground,
            "{name}: current"
        );
        assert_eq!(
            resting_ink(true, false, &palette),
            theme.muted_frame,
            "{name}: other modes"
        );
        assert_eq!(
            resting_ink(false, false, &palette),
            theme.muted_frame.opacity(0.55),
            "{name}: ChromeButton.qml dims a disabled mark's muted frame"
        );
    }
    let (_, _, latte) = &themes()[0];
    assert!(
        resting_ink(true, false, latte).l > resting_ink(true, true, latte).l,
        "on a light theme the other modes read lighter than the current one"
    );
}

/// EY-407, EY-408: FactsTable.qml's names in Theme.color.muted, PreviewFactsLine.qml's line in the foreground.
#[test]
fn ey407_facts_names_are_muted_and_the_facts_line_is_foreground() {
    use super::{label_ink, line_ink};
    for (name, theme, palette) in themes() {
        assert_eq!(label_ink(&palette), theme.muted, "{name}: fact names");
        assert_eq!(
            line_ink(&palette),
            theme.foreground,
            "{name}: the facts line"
        );
    }
}

/// EY-414: TabBar.qml draws an inactive tab's label muted and its close mark in the muted frame; the current tab in the foreground.
#[test]
fn ey414_the_quiet_caption_and_mark_roles_are_muted_and_muted_frame() {
    for (name, theme, palette) in themes() {
        assert_eq!(
            palette.caption(true),
            theme.foreground,
            "{name}: lit caption"
        );
        assert_eq!(palette.caption(false), theme.muted, "{name}: quiet caption");
        assert_eq!(palette.mark(true), theme.foreground, "{name}: lit mark");
        assert_eq!(palette.mark(false), theme.muted_frame, "{name}: quiet mark");
    }
    let (_, _, latte) = &themes()[0];
    assert!(
        latte.caption(false).l > latte.caption(true).l,
        "an inactive tab reads lighter on Latte"
    );
    assert!(
        latte.mark(false).l > latte.caption(false).l,
        "its close mark lighter still"
    );
}

/// EY-439: waits stay notices and failures stay errors; the Starting route is pinned in g2/pl2/bridge_line_tests.rs.
#[test]
fn ey439_waits_stay_notices_and_failures_stay_errors() {
    use crate::g2::pl2::result::{say_role, SayRole};
    for key in ["network.another_opening", "network.unmounted"] {
        assert_eq!(say_role(key), SayRole::Notice, "{key}");
    }
    for key in [
        "network.bridge_failed_timeout",
        "network.invalid_address",
        "network.no_answer",
        "network.unmount_failed",
    ] {
        assert_eq!(say_role(key), SayRole::Error, "{key}");
    }
}
