// The layout numbers the canvas-040 boards draw, read from their HTML. Quick Look is ql-markdown and its siblings; the
// preview column is cols-preview-markdown-facts and cols-preview-markdown-crlf. Where no board draws a case, the test
// pins the oracle's number and says so.
use super::markdown_flow::{Atom, Face, FormulaBox, Measure};
use super::markdown_layout as lay;
use super::markdown_layout::{Assets, Counter, FigureState, Geometry, Item, Metrics};
use super::*;

// Every character 0.6 em wide, as the board's monospace face is at its size.
struct Mono;

impl Measure for Mono {
    fn width(&mut self, text: &str, _face: Face, px: f32) -> f32 {
        text.chars().count() as f32 * 0.6 * px
    }

    fn extent(&mut self, _face: Face, px: f32) -> (f32, f32) {
        (0.8 * px, 0.2 * px)
    }
}

struct NoAssets;

impl Assets for NoAssets {
    fn picture(&self, _url: &str) -> Option<(f32, f32)> {
        None
    }

    fn figure(&self, _ordinal: usize) -> FigureState {
        FigureState::Waiting
    }

    fn formula(&self, _ordinal: usize) -> Option<FormulaBox> {
        None
    }

    fn code(&self, _fence: usize, _line: usize) -> Option<Vec<(i64, String)>> {
        None
    }
}

// Quick Look: the theme's 14 px body on its 24 px line, with the theme's gap of 9.
fn quick_look() -> Metrics {
    Metrics {
        body: 14.0,
        theme_body: 14.0,
        caption: 12.0,
        gap: 9.0,
        hairline: 1.0,
        row_pad_x: 12.0,
        mark: 16.0,
        column: false,
    }
}

// The preview column: the next size under the theme's 14 px body, so 13 px on a 22 px line.
fn column() -> Metrics {
    Metrics {
        body: 13.0,
        column: true,
        ..quick_look()
    }
}

// The document laid out at 600 px, as the stack draws it.
fn stacked(source: &str, metrics: Metrics) -> Geometry {
    let words = Words::english();
    let ctx = Ctx {
        dir: "/doc",
        chrome: "#181825",
        ink: "#c0caf5",
        words: &words,
    };
    let parsed = blocks(source, &ctx);
    let mut measure = Mono;
    let mut laid = lay::Ctx {
        metrics,
        measure: &mut measure,
        assets: &NoAssets,
        counter: Counter::default(),
        remote: "%1",
    };
    lay::stack(&parsed, 600.0, &mut laid)
}

// The y of each table rule, the faint wash under a row, in the order drawn.
fn rules(geometry: &Geometry) -> Vec<f32> {
    geometry
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Rule { y, wash: true, .. } => Some(*y),
            _ => None,
        })
        .collect()
}

#[test]
fn quick_look_h1_sits_on_the_board_32_px_line() {
    // ql-markdown.html: "Release notes" is 20 px on a 32 px line.
    assert_eq!(stacked("# Release notes\n", quick_look()).height, 32.0);
}

#[test]
fn quick_look_h2_has_8_px_above_it_and_the_24_px_body_line() {
    // ql-markdown.html: "Changes" is 15 px on the body's 24 px line, with margin-top 8.
    let geometry = stacked("# Release notes\n\n## Changes\n", quick_look());
    assert_eq!(geometry.height, 32.0 + 8.0 + 24.0);
}

#[test]
fn quick_look_table_is_a_28_px_header_then_29_px_rows() {
    // ql-markdown.html: the header cells are 28 px high and the body cells 29 px, the rule under each included.
    let geometry = stacked(
        "| Key | Value |\n|---|---|\n| width | 240 |\n| depth | 3 |\n",
        quick_look(),
    );
    assert_eq!(rules(&geometry), vec![27.0, 56.0, 85.0]);
    assert_eq!(geometry.height, 86.0);
}

#[test]
fn quick_look_fence_pads_8_px_above_and_below_its_two_20_px_lines() {
    // ql-markdown.html: the [preview] block has padding 8 px 12 px and line-height 20 px.
    let geometry = stacked("```\n[preview]\nwidth = 240\n```\n", quick_look());
    assert_eq!(geometry.height, 56.0);
}

#[test]
fn quick_look_paragraph_is_a_24_px_line() {
    // ql-markdown-crlf.html: the paragraph's line box is 24 px on the 14 px body.
    let geometry = stacked("A paragraph before the fences.\n", quick_look());
    assert_eq!(geometry.height, 24.0);
}

#[test]
fn the_preview_column_h1_is_a_28_px_line_with_8_px_under_before_its_table() {
    // cols-preview-markdown-facts.html: "Facts" is 20 px on a 28 px line with margin-bottom 8; the table's
    // margin-top 2 collapses into that 8. The rule under each 26 px row content sits 1 px below it.
    let geometry = stacked(
        "# Facts\n\n| Fact | Value |\n|---|---|\n| width | 240 |\n| depth | 3 |\n",
        column(),
    );
    assert_eq!(
        rules(&geometry),
        vec![36.0 + 26.0, 36.0 + 53.0, 36.0 + 80.0]
    );
    assert_eq!(geometry.height, 36.0 + 81.0);
}

#[test]
fn the_preview_column_table_rows_are_27_px() {
    // cols-preview-markdown-facts.html: every table cell is 27 px high, its rule included.
    let geometry = stacked(
        "| Fact | Value |\n|---|---|\n| width | 240 |\n| depth | 3 |\n",
        column(),
    );
    assert_eq!(rules(&geometry), vec![26.0, 53.0, 80.0]);
    assert_eq!(geometry.height, 81.0);
}

#[test]
fn the_preview_column_fence_pads_9_px_above_and_7_below_its_18_px_lines() {
    // cols-preview-markdown-crlf.html: the column's fence has padding 9 px 12 px 7 px and line-height 18 px.
    let geometry = stacked("```\n[preview]\nwidth = 240\n```\n", column());
    assert_eq!(geometry.height, 52.0);
}

#[test]
fn oracle_line_boxes_stay_where_no_board_draws_the_case() {
    // No board draws a level-3 heading or a column h2, so both keep the oracle's 1.7 times the size.
    assert_eq!(stacked("### Deep\n", quick_look()).height, 24.0);
    assert_eq!(stacked("## Changes\n", column()).height, 26.0);
}

fn first_line_atoms(geometry: &Geometry) -> Vec<Atom> {
    let Some(Item::Text { laid, .. }) = geometry
        .items
        .iter()
        .find(|item| matches!(item, Item::Text { .. }))
    else {
        panic!("no paragraph drawn");
    };
    laid.lines[0].atoms.clone()
}

fn chip_box(atoms: &[Atom], label: &str) -> (f32, f32, f32, f32) {
    let mut measure = Mono;
    let (x, look) = atoms
        .iter()
        .find_map(|atom| match atom {
            Atom::Text { x, text, look, .. } if text == label => Some((*x, look.clone())),
            _ => None,
        })
        .expect("the label is drawn");
    let glyphs = measure.width(label, look.face, look.px);
    let (ground, width) = atoms
        .iter()
        .find_map(|atom| match atom {
            Atom::Ground { x: at, width, .. } if *at <= x && x < at + width => Some((*at, *width)),
            _ => None,
        })
        .expect("the label sits on a chip ground");
    (ground, width, x, glyphs)
}

#[test]
fn a_kbd_chip_centres_its_label_with_the_board_pad_either_side() {
    // Quick Look keeps 4 px; HIG preview-column gives the column's shared code chip 3 px.
    for (metrics, pad) in [(quick_look(), 4.0), (column(), 3.0)] {
        let atoms = first_line_atoms(&stacked(
            "Press <kbd>Ctrl</kbd>+<kbd>C</kbd> to copy.\n",
            metrics,
        ));
        for label in ["Ctrl", "C"] {
            let (ground, width, x, glyphs) = chip_box(&atoms, label);
            assert_eq!(x - ground, pad, "left pad of {label}");
            assert_eq!(ground + width - (x + glyphs), pad, "right pad of {label}");
        }
    }
}

#[test]
fn a_kbd_chip_keeps_the_space_before_it_and_the_plus_between_the_chips() {
    // ql-markdown-html.html: "Press " keeps its space before the first chip, and "+" sits from one chip's edge to the next.
    let atoms = first_line_atoms(&stacked(
        "Press <kbd>Ctrl</kbd>+<kbd>C</kbd> to copy.\n",
        quick_look(),
    ));
    let (ctrl, ctrl_width, _, _) = chip_box(&atoms, "Ctrl");
    let (c, _, _, _) = chip_box(&atoms, "C");
    let press = atoms
        .iter()
        .find_map(|atom| match atom {
            Atom::Text { x, width, text, .. } if text == "Press " => Some((*x, *width)),
            _ => None,
        })
        .expect("the lead-in is drawn");
    let plus = atoms
        .iter()
        .find_map(|atom| match atom {
            Atom::Text { x, width, text, .. } if text == "+" => Some((*x, *width)),
            _ => None,
        })
        .expect("the plus is drawn");
    assert_eq!(
        press.0 + press.1,
        ctrl,
        "the space stays before the first chip"
    );
    assert_eq!(
        plus.0,
        ctrl + ctrl_width,
        "the plus starts where the first chip ends"
    );
    assert_eq!(
        c,
        plus.0 + plus.1,
        "the second chip starts where the plus ends"
    );
}

#[test]
fn back_to_back_kbd_chips_share_one_ground_with_the_pad_either_side() {
    // Touching chip pieces share one ground with the owning surface's padding on either side.
    for (metrics, pad) in [(quick_look(), 4.0), (column(), 3.0)] {
        let atoms = first_line_atoms(&stacked("Press <kbd>A</kbd><kbd>B</kbd> now.\n", metrics));
        let (ground, width, x, glyphs) = chip_box(&atoms, "AB");
        assert_eq!(x - ground, pad, "left pad of AB");
        assert_eq!(ground + width - (x + glyphs), pad, "right pad of AB");
    }
}
