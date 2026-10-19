use super::test_support::Fixture;
use super::*;

#[test]
fn valid_truecolor_and_indexed_output_becomes_cells_with_resets() {
    let fixture = Fixture::new();
    let grid = fixture
        .output(
            "\x1b[38;2;1;2;3;48;5;200m▀▄\x1b[0m\n\x1b[38;5;9;48;2;4;5;6m▀\x1b[39;49m \x1b[m\n"
                .as_bytes(),
            2,
            2,
        )
        .unwrap();
    assert_eq!((grid.columns, grid.rows), (2, 2));
    assert_eq!(
        grid.cells,
        vec![
            Cell {
                symbol: '▀',
                foreground: Color::Rgb(1, 2, 3),
                background: Color::Indexed(200)
            },
            Cell {
                symbol: '▄',
                foreground: Color::Rgb(1, 2, 3),
                background: Color::Indexed(200)
            },
            Cell {
                symbol: '▀',
                foreground: Color::Indexed(9),
                background: Color::Rgb(4, 5, 6)
            },
            Cell {
                symbol: ' ',
                foreground: Color::Default,
                background: Color::Default
            },
        ]
    );
}

#[test]
fn shorter_rows_are_padded_and_sgr_state_crosses_newlines() {
    let grid = Fixture::new()
        .output("\x1b[38;5;42m▀▄\n▀\n".as_bytes(), 3, 3)
        .unwrap();
    assert_eq!((grid.columns, grid.rows, grid.cells.len()), (2, 2, 4));
    assert_eq!(grid.cells[2].foreground, Color::Indexed(42));
    assert_eq!(
        grid.cells[3],
        Cell {
            symbol: ' ',
            foreground: Color::Default,
            background: Color::Default
        }
    );
}

macro_rules! rejects {
    ($name:ident, $bytes:expr) => {
        #[test]
        fn $name() {
            let mut bytes = "▀".as_bytes().to_vec();
            bytes.extend_from_slice($bytes);
            assert!(Fixture::new().output(&bytes, 2, 2).is_err());
        }
    };
}

rejects!(refuses_osc_clipboard, b"\x1b]52;c;ZGF0YQ==\x07");
rejects!(refuses_osc_hyperlink, b"\x1b]8;;https://example.org\x1b\\");
rejects!(refuses_dcs_sixel, b"\x1bPqdata\x1b\\");
rejects!(refuses_apc_kitty, b"\x1b_Ga=T;data\x1b\\");
rejects!(refuses_cursor_movement, "▀\x1b[1;1H".as_bytes());
rejects!(refuses_cursor_save, b"\x1b7");
rejects!(refuses_erase, b"\x1b[2J");
rejects!(refuses_repeat_cells, "▀\x1b[99b".as_bytes());
rejects!(refuses_private_csi, b"\x1b[?25l");
rejects!(refuses_truncated_escape, "▀\x1b".as_bytes());
rejects!(refuses_truncated_sgr, b"\x1b[38;2;1;2;3");
rejects!(refuses_incomplete_truecolor, b"\x1b[38;2;1;2m");
rejects!(refuses_out_of_range_truecolor, b"\x1b[48;2;0;0;256m");
rejects!(refuses_incomplete_indexed_color, b"\x1b[38;5m");
rejects!(refuses_out_of_range_indexed_color, b"\x1b[48;5;256m");
rejects!(refuses_unsupported_sgr, b"\x1b[1;7;31m");
rejects!(refuses_empty_sgr_parameter, b"\x1b[38;;5;1m");
rejects!(refuses_colon_or_intermediate_sgr, b"\x1b[38:2::1:2:3m");
rejects!(
    refuses_oversized_sgr,
    b"\x1b[00000000000000000000000000000000000000000000000000000000000000000000m"
);
rejects!(refuses_carriage_return, "▀\r▀".as_bytes());
rejects!(refuses_tabs_and_controls, b"\t\x07\x00\x7f");
rejects!(refuses_invalid_utf8, b"\xff");
rejects!(refuses_c1_control, "\u{009b}38;5;1m".as_bytes());
rejects!(refuses_wide_symbols, "界".as_bytes());
rejects!(refuses_combining_symbols, "▀\u{0301}".as_bytes());
rejects!(
    refuses_invisible_and_bidi_symbols,
    "\u{200b}\u{202e}".as_bytes()
);
rejects!(
    refuses_symbols_outside_horizontal_half_blocks,
    "▌█A".as_bytes()
);
rejects!(refuses_row_wider_than_rectangle, "▀▀▀\n".as_bytes());
rejects!(refuses_more_rows_than_rectangle, "▀\n▀\n▀\n".as_bytes());
rejects!(refuses_empty_rows, b"\n\n");

#[test]
fn refuses_empty_output_and_checks_byte_and_geometry_caps_without_a_decoder() {
    assert!(validate(b"", 2, 2).is_err());
    assert!(validate(&vec![b' '; OUTPUT_LIMIT + 1], 1, 1).is_err());
    for (columns, rows) in [
        (0, 1),
        (1, 0),
        (4097, 1),
        (1, 2049),
        (usize::MAX, usize::MAX),
    ] {
        assert!(geometry(columns, rows).is_err());
    }
    assert!(geometry(1, 1).is_ok());
    assert!(geometry(48, 14).is_ok());
}

#[test]
fn all_sgr_resets_restore_default_colors_on_subsequent_cells() {
    for reset in ["0", "", "39;49"] {
        let text = format!("\x1b[38;5;42;48;2;1;2;3m▀\x1b[{reset}m▄");
        let grid = validate(text.as_bytes(), 2, 1).unwrap();
        assert_eq!(grid.cells[0].foreground, Color::Indexed(42));
        assert_eq!(grid.cells[0].background, Color::Rgb(1, 2, 3));
        assert_eq!(grid.cells[1].foreground, Color::Default);
        assert_eq!(grid.cells[1].background, Color::Default);
    }
}

#[test]
fn each_control_escape_and_unsupported_sgr_is_refused_individually() {
    for control in (0u8..=31).filter(|c| ![b'\n', 27].contains(c)).chain([127]) {
        let mut bytes = "▀".as_bytes().to_vec();
        bytes.push(control);
        assert!(validate(&bytes, 2, 1).is_err(), "control {control}");
    }
    for control in '\u{0080}'..='\u{009f}' {
        assert!(validate(format!("▀{control}").as_bytes(), 2, 1).is_err());
    }
    for final_byte in (0x30u8..=0x7e).filter(|b| *b != b'[') {
        let mut bytes = "▀".as_bytes().to_vec();
        bytes.extend_from_slice(&[27, final_byte]);
        assert!(validate(&bytes, 2, 1).is_err());
    }
    for code in [1, 2, 3, 4, 8, 9, 22, 30, 31, 37, 40, 47, 90, 97, 100, 107] {
        assert!(
            validate(format!("▀\x1b[{code}m▄").as_bytes(), 2, 1).is_err(),
            "SGR {code}"
        );
    }
}
