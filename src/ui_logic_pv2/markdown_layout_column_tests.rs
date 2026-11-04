// A column table after blocks no board draws keeps the oracle's gap: PreviewMarkdown.qml spaces every block
// by boardBlockGap 6, and the column only changes the body size. After a picture the oracle draws 12, the
// picture's own 6 plus the gap.
use super::markdown_flow::{Face, FormulaBox, Measure};
use super::markdown_layout as lay;
use super::markdown_layout::{Assets, Counter, FigureState, Geometry, Metrics};
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

// The preview column: the next size under the theme's 14 px body, so 13 px on a 22 px line.
fn column() -> Metrics {
    Metrics {
        body: 13.0,
        theme_body: 14.0,
        caption: 12.0,
        gap: 9.0,
        hairline: 1.0,
        row_pad_x: 12.0,
        mark: 16.0,
        column: true,
    }
}

// The document laid out at 600 px, as the stack draws it.
fn stacked(source: &str, metrics: Metrics) -> Geometry {
    stacked_at(source, metrics, 600.0)
}

fn stacked_at(source: &str, metrics: Metrics, width: f32) -> Geometry {
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
    lay::stack(&parsed, width, &mut laid)
}

const TABLE: &str = "| A | B |\n|---|---|\n| c | d |\n";

// The heights of the previous block alone, the table alone, and the two together, so the test isolates the
// gap above the table from every block height.
fn around(prev_source: &str) -> (f32, f32, f32) {
    let prev = stacked(prev_source, column()).height;
    let table = stacked(TABLE, column()).height;
    let combined = stacked(&format!("{prev_source}\n{TABLE}"), column()).height;
    (prev, table, combined)
}

#[test]
fn column_table_after_a_paragraph_keeps_the_oracle_6_px_gap() {
    // A 22 px paragraph line, then the oracle's 6 px, then the 54 px table.
    let (prev, table, combined) = around("Some text.\n");
    assert_eq!(prev, 22.0);
    assert_eq!(table, 54.0);
    assert_eq!(combined, prev + 6.0 + table);
}

#[test]
fn column_table_after_a_list_keeps_the_oracle_6_px_gap() {
    let (prev, table, combined) = around("- one\n- two\n");
    assert_eq!(combined, prev + 6.0 + table);
}

#[test]
fn column_table_after_a_quote_keeps_the_oracle_6_px_gap() {
    let (prev, table, combined) = around("> quoted\n");
    assert_eq!(combined, prev + 6.0 + table);
}

#[test]
fn column_table_after_headings_keeps_the_oracle_6_px_gap() {
    // Levels 2 to 6 keep the oracle's line boxes in the column, each followed by the 6 px gap.
    for level in 2..=6u8 {
        let (prev, table, combined) = around(&format!("{} Title\n", "#".repeat(level as usize)));
        assert_eq!(combined, prev + 6.0 + table, "h{level} then a table");
    }
}

#[test]
fn column_table_after_a_remote_box_keeps_the_oracle_6_px_gap() {
    let (prev, table, combined) = around("![alt](https://cdn.example.com/x.png)\n");
    assert_eq!(combined, prev + 6.0 + table);
}

#[test]
fn column_table_after_a_picture_keeps_the_oracle_12_px_gap() {
    // The picture lends its own 6 px tail, then the oracle's 6 px gap.
    let (prev, table, combined) = around("![alt](bands.png)\n");
    assert_eq!(combined, prev + 6.0 + 6.0 + table);
}

#[test]
fn the_preview_column_h1_holds_8_px_under_before_a_paragraph() {
    // cols-preview-markdown-crlf.html: "CRLF notes" is 20 px on a 28 px line with margin-bottom 8, over the
    // 22 px paragraph "A paragraph before the fences."
    let geometry = stacked("# CRLF notes\n\nA paragraph before the fences.\n", column());
    assert_eq!(geometry.height, 28.0 + 8.0 + 22.0);
}

#[test]
fn vf_more_column_paragraph_keeps_and_on_the_first_line_at_board_width() {
    // The 276 px frame leaves 234 px after its border and two 20 px insets.
    let geometry = stacked_at(
        "A **bold** claim, `inline code` and a [link](https://example.test).\n",
        column(),
        234.0,
    );
    let lines: Vec<String> = geometry
        .items
        .iter()
        .filter_map(|item| match item {
            lay::Item::Text { laid, .. } => Some(
                laid.lines
                    .iter()
                    .map(|line| {
                        line.atoms
                            .iter()
                            .filter_map(|atom| match atom {
                                super::markdown_flow::Atom::Text { text, .. } => {
                                    Some(text.as_str())
                                }
                                _ => None,
                            })
                            .collect::<String>()
                    })
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .flatten()
        .collect();
    assert_eq!(lines, ["A bold claim, inline code and", "a link."]);
}
