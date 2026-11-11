use super::*;
use flea::ui_logic_print_convert::composition::Spacing;

#[test]
fn mono_raster_roles_keep_badges_models_and_grid_visible() {
    for text in [
        PALETTE,
        "background = \"#ffffff\"",
        "background = \"#000000\"",
    ] {
        let theme = Theme::with_depth(text, Depth::Mono);
        let palette = crate::tui::graphics::palette(&theme);
        for (consumer, ink) in [
            ("combine focus and model faces", palette.foreground),
            ("combine digits and model grid", palette.muted),
            ("model plate edge", palette.frame),
        ] {
            let ground = flea::ui_logic_model_preview::color::to_bytes(palette.ground);
            let ink = flea::ui_logic_model_preview::color::to_bytes(ink);
            assert_ne!(ink, ground, "{consumer} collapsed into its ground: {text}");
            assert_eq!(ink[0], ink[1], "Mono raster ink must be achromatic");
            assert_eq!(ink[1], ink[2], "Mono raster ink must be achromatic");
        }
    }
}

#[test]
fn sixteen_colour_raster_roles_decode_instead_of_collapsing_into_the_ground() {
    let palette = crate::tui::graphics::palette(&Theme::with_depth(PALETTE, Depth::Sixteen));
    let ground = flea::ui_logic_model_preview::color::to_bytes(palette.ground);
    for ink in [palette.foreground, palette.muted, palette.frame] {
        assert_ne!(flea::ui_logic_model_preview::color::to_bytes(ink), ground);
    }
}

#[test]
fn mono_disabled_combine_fields_dim_without_selection_marks() {
    let theme = Theme::with_depth(PALETTE, Depth::Mono);
    let mut m = opened("combine");
    for (layout, spacing) in [
        (Layout::Row, Spacing::None),
        (Layout::Column, Spacing::None),
        (Layout::Grid, Spacing::None),
        (Layout::Grid, Spacing::Small),
    ] {
        let f = m.combining.card.as_mut().unwrap();
        f.settings.layout = layout;
        f.settings.spacing = spacing;
        f.focus = 2;
        let g = combine::geometry(&m);
        let cells = attributes(&frame(&m, &theme), m.columns, m.height + 4);
        for field in g.fields.iter().filter(|f| matches!(f.focus, 2 | 5)) {
            for cell in &cells[field.rect.y][field.rect.x..field.rect.x + field.rect.width] {
                assert_eq!(cell.dim, !field.enabled, "layout {layout:?}");
                if !field.enabled {
                    assert!(!cell.reverse, "disabled selection cue");
                    assert_eq!(
                        cell.underline,
                        field.focus == 2 && field.chosen,
                        "focus cue"
                    );
                }
            }
        }
        let columns = g.fields.iter().find(|f| f.focus == 2).unwrap();
        assert_eq!(
            cells[columns.rect.y - 1][columns.rect.x].dim,
            layout != Layout::Grid
        );
        let background = g.fields.iter().find(|f| f.focus == 5).unwrap();
        assert_eq!(background.enabled, spacing != Spacing::None);
        assert_eq!(
            cells[background.rect.y - 1][background.rect.x].dim,
            !background.enabled
        );
    }
}

#[test]
fn mono_disabled_card_buttons_dim_keep_focus_and_reset_before_the_next_button() {
    let theme = Theme::with_depth(PALETTE, Depth::Mono);
    let card = render::cards::layout(60, 20, 40, 1, &[8, 8], 0);
    let buttons: Vec<_> = [false, true]
        .into_iter()
        .map(|enabled| render::cards::Button {
            label: "Test".into(),
            default: true,
            focused: true,
            enabled,
        })
        .collect();
    let mut stream = String::new();
    render::cards::buttons(&mut stream, &theme, &card, &buttons);
    let cells = attributes(&stream, 60, 20);
    for (i, &(x, y, width)) in card.buttons.iter().enumerate() {
        for cell in &cells[y][x..x + width] {
            assert_eq!(cell.dim, i == 0);
            assert!(cell.reverse, "button {i} lost its focus cue");
            assert!(!cell.underline);
        }
    }
}

#[test]
fn mono_disabled_convert_and_print_options_dim_keep_focus_without_leaking() {
    let theme = Theme::with_depth(PALETTE, Depth::Mono);
    for kind in ["convert", "print"] {
        let mut m = opened(kind);
        if kind == "convert" {
            let f = m.converting.card.as_mut().unwrap();
            f.choice = f
                .kind
                .formats()
                .iter()
                .position(|format| *format == "pdf")
                .unwrap();
            f.focus = 1;
            assert!(!f.option_enabled());
        } else {
            let f = m.printing.card.as_mut().unwrap();
            // Tab reaches Duplex from the old caps while the new printer's query is in flight.
            f.dropdown = false;
            f.caps.duplex = true;
            f.caps.color = false;
            f.query = Some(1);
            f.focus = 3;
        }
        let stream = frame(&m, &theme);
        let rows = render::frame_tests::grid(&stream, m.columns, m.height + 4);
        let cells = attributes(&stream, m.columns, m.height + 4);
        let mut checked = 0;
        for (y, row) in rows.iter().enumerate() {
            for (offset, _) in row.match_indices("[ ]") {
                let x = crate::tui::cells::text_width(&row[..offset]);
                assert!(cells[y][x].dim, "{kind}: checkbox is not dim");
                // Left to right, "[ ] " is four cells, so x + 4 is the label's first cell; only the first box has focus.
                assert_eq!(cells[y][x + 4].underline, checked == 0, "{kind}: focus cue");
                checked += 1;
            }
        }
        assert_eq!(checked, if kind == "convert" { 1 } else { 2 });
        let (y, x) = rows
            .iter()
            .enumerate()
            .find_map(|(y, row)| {
                row.find("Cancel")
                    .map(|x| (y, crate::tui::cells::text_width(&row[..x])))
            })
            .unwrap();
        assert!(!cells[y][x].dim, "{kind}: dim leaked into Cancel");
    }
}
