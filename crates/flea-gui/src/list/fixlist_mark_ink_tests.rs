//! CR-O1-F3 and CR-O1-F4: a dual pane rests its marks in the muted frame and lifts only the current pane's cursor, on two themes.
use super::{element::Shown, picker::mark_ink, ListView, Palette, RowState, RowStates};
use crate::palette::Theme;

const LATTE: &str = "background='#eff1f5'\nforeground='#4c4f69'\naccent='#1e66f5'\ndark_background='#e3e4e8'\nmuted='#acb0be'";
const SUNSET: &str = "background='#1c120d'\nforeground='#c6b395'\naccent='#d28a41'\ndark_background='#150d0a'\nmuted='#68544b'";

fn shown(state: RowState) -> Shown {
    Shown {
        index: 0,
        view: 0,
        prepared: None,
        clip: None,
        state,
        bold_match: false,
        picture: None,
    }
}

fn pane(current: bool) -> RowStates {
    RowStates {
        focused: current,
        other_pane: !current,
        ..RowStates::default()
    }
}

#[test]
fn fixlist_crcmpo1_f3_f4_dual_marks_rest_in_the_muted_frame() {
    let rest = RowState::default();
    let cursor = RowState {
        cursor: true,
        ..RowState::default()
    };
    let marked = RowState {
        selected: true,
        ..RowState::default()
    };
    let hovered = RowState {
        hovered: true,
        hover_lit: true,
        ..RowState::default()
    };
    let target = RowState {
        drop_target: true,
        ..RowState::default()
    };
    for (name, text) in [("catppuccin-latte", LATTE), ("sunset", SUNSET)] {
        let palette = Palette::inherited(&gpui::TextStyle::default(), Theme::from_text(text));
        assert_ne!(
            palette.muted_frame, palette.metadata_frame,
            "{name}: the fixture needs two inks"
        );
        let dual = ListView {
            dual: true,
            ..ListView::default()
        };
        // F3: a rest row in either pane draws the muted frame, below the divider's weight.
        for current in [true, false] {
            assert_eq!(
                mark_ink(&shown(rest), &dual, palette, &pane(current)),
                palette.muted_frame,
                "{name}: rest {current}"
            );
            assert_eq!(
                mark_ink(&shown(marked), &dual, palette, &pane(current)),
                palette.muted_frame,
                "{name}: marked {current}"
            );
            // Row.qml:178's dual arm lifts nothing but the current cursor, so a hovered row and a drop target stay muted.
            for (what, state) in [("hovered", hovered), ("drop target", target)] {
                assert_eq!(
                    mark_ink(&shown(state), &dual, palette, &pane(current)),
                    palette.muted_frame,
                    "{name}: {what} {current}"
                );
            }
        }
        // F4: the other pane's cursor stays in the muted frame on its grey rung; the current pane's cursor lifts, menu or not.
        assert_eq!(
            mark_ink(&shown(cursor), &dual, palette, &pane(false)),
            palette.muted_frame,
            "{name}: other pane cursor"
        );
        assert_eq!(
            mark_ink(&shown(cursor), &dual, palette, &pane(true)),
            palette.foreground,
            "{name}: current pane cursor"
        );
        // The pane side decides, never the keyboard: a menu borrowing the current pane's keyboard keeps its cursor lifted.
        let behind_menu = RowStates {
            focused: false,
            ..RowStates::default()
        };
        assert_eq!(
            mark_ink(&shown(cursor), &dual, palette, &behind_menu),
            palette.foreground,
            "{name}: current pane cursor behind a menu"
        );
        // A single pane keeps Row.qml's metadata frame at rest and the foreground on a lifted row.
        let single = ListView::default();
        assert_eq!(
            mark_ink(&shown(rest), &single, palette, &pane(true)),
            palette.metadata_frame,
            "{name}: single rest"
        );
        assert_eq!(
            mark_ink(&shown(cursor), &single, palette, &pane(false)),
            palette.foreground,
            "{name}: single cursor"
        );
    }
}
