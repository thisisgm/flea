//! Table geometry measured with the production text system, independent of character-count mocks.
use super::{style::Faces, worker::TextMeasure};
use flea::ui_logic_pv2::markdown::{
    self as md,
    markdown_flow::{Atom, Face, FormulaBox},
    markdown_layout::{self as layout, Assets, Counter, FigureState, Geometry, Item, Metrics},
};
use gpui::{px, TextRun, TextSystem};
use std::sync::Arc;

struct Fonts {
    text: Arc<TextSystem>,
    faces: Faces,
}

impl Fonts {
    fn new() -> Self {
        Self {
            text: Arc::new(TextSystem::new(
                gpui_platform::current_platform(true).text_system(),
            )),
            faces: Faces {
                family: "DejaVu Sans Mono".into(),
                symbol: "DejaVu Sans".into(),
            },
        }
    }

    fn width(&self, text: &str, face: Face, size: f32) -> f32 {
        let run = TextRun {
            len: text.len(),
            font: self.faces.font(face),
            color: crate::palette::Theme::default().foreground,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let shaper = gpui::WindowTextSystem::new(Arc::clone(&self.text));
        let line = shaper.layout_line(text, px(size), &[run], None);
        assert!(line.runs.iter().any(|run| !run.glyphs.is_empty()));
        f32::from(line.width)
    }

    fn table(&self, source: &str, column: bool, width: f32) -> Geometry {
        let words = md::Words::english();
        let parsed = md::blocks(
            source,
            &md::Ctx {
                dir: "/md-tables",
                chrome: "#181825",
                ink: "#c0caf5",
                words: &words,
            },
        );
        let mut measure = TextMeasure::new(Arc::clone(&self.text), self.faces.clone());
        let mut ctx = layout::Ctx {
            metrics: Metrics {
                body: if column { 13. } else { 14. },
                theme_body: 14.,
                caption: 12.,
                gap: 9.,
                hairline: 1.,
                row_pad_x: 14.,
                mark: 16.,
                column,
            },
            measure: &mut measure,
            assets: &NoAssets,
            counter: Counter::default(),
            remote: "%1",
        };
        layout::stack(&parsed, width, &mut ctx)
    }
}

struct NoAssets;
impl Assets for NoAssets {
    fn picture(&self, _: &str) -> Option<(f32, f32)> {
        None
    }
    fn figure(&self, _: usize) -> FigureState {
        FigureState::Waiting
    }
    fn formula(&self, _: usize) -> Option<FormulaBox> {
        None
    }
    fn code(&self, _: usize, _: usize) -> Option<Vec<(i64, String)>> {
        None
    }
}

fn items(geometry: &Geometry) -> &[Item] {
    match geometry.items.as_slice() {
        [Item::Table { items, .. }] => items,
        items => items,
    }
}

fn text(geometry: &Geometry, wanted: &str) -> (f32, f32, usize) {
    for item in items(geometry) {
        if let Item::Text { x, laid, .. } = item {
            for line in &laid.lines {
                for atom in &line.atoms {
                    if let Atom::Text { text, x: at, .. } = atom {
                        if text == wanted {
                            return (*x, *x + at, laid.lines.len());
                        }
                    }
                }
            }
        }
    }
    panic!("table did not retain the shaped text {wanted:?}");
}

const LEFT: &str = "abcdefghijklmnopqrstuvwxyzabcdef";
const CENTER: &str = "middlemiddlemiddlemiddle";
const RIGHT: &str = "12345678901234567890123456";

fn aligned_source() -> String {
    format!("| L | C | R |\n|:---|:---:|---:|\n| {LEFT} | {CENTER} | {RIGHT} |\n| a | mid | z |\n")
}

fn near(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.02, "{actual} != {expected}");
}

#[test]
fn md_tables_shaped_columns_preserve_unbroken_cell_widths() {
    let fonts = Fonts::new();
    for column in [false, true] {
        let size = if column { 13. } else { 14. };
        let geometry = fonts.table(&aligned_source(), column, 260.);
        let first = fonts.width(LEFT, Face::Bold, size).ceil() + 14.;
        let second = fonts.width(CENTER, Face::Bold, size).ceil() + 14.;
        near(text(&geometry, "C").0, first);
        near(text(&geometry, "R").0, first + second);
        assert!(
            matches!(geometry.items.as_slice(), [Item::Table { content, width, .. }] if content > width)
        );
    }
}

#[test]
fn md_tables_shaped_header_sets_its_column_width() {
    let fonts = Fonts::new();
    let header = "\u{0644}\u{0627} \u{0644}\u{0627}";
    let source = format!("| {header} | R |\n|---|---|\n| a | b |\n");
    for column in [false, true] {
        let size = if column { 13. } else { 14. };
        let geometry = fonts.table(&source, column, 260.);
        near(
            text(&geometry, "R").0,
            fonts.width(header, Face::Bold, size).ceil() + 14.,
        );
    }
}

#[test]
fn md_tables_shaped_alignment_keeps_left_center_and_right_edges() {
    let fonts = Fonts::new();
    for column in [false, true] {
        let size = if column { 13. } else { 14. };
        let geometry = fonts.table(&aligned_source(), column, 260.);
        let first = fonts.width(LEFT, Face::Bold, size).ceil() + 14.;
        let second = fonts.width(CENTER, Face::Bold, size).ceil() + 14.;
        let third = fonts.width(RIGHT, Face::Bold, size).ceil() + 14.;
        near(text(&geometry, LEFT).1, 0.);
        near(text(&geometry, "a").1, 0.);
        near(
            text(&geometry, "mid").1,
            first + ((second - 14. - fonts.width("mid", Face::Regular, size)) / 2.).round(),
        );
        near(
            text(&geometry, RIGHT).1 + fonts.width(RIGHT, Face::Regular, size),
            first + second + third - 14.,
        );
        near(
            text(&geometry, "z").1 + fonts.width("z", Face::Regular, size),
            first + second + third - 14.,
        );
    }
}

#[test]
fn md_tables_shaped_wrapping_breaks_at_spaces_before_splitting_words() {
    let fonts = Fonts::new();
    let word = "abcdefghijklmnopqr";
    let source = format!("| L | R |\n|:---|---:|\n| {word} {word} {word} | {RIGHT} |\n");
    for column in [false, true] {
        let geometry = fonts.table(&source, column, 260.);
        assert_eq!(text(&geometry, word).2, 3);
        assert_eq!(text(&geometry, RIGHT).2, 1);
    }
}

#[test]
fn md_tables_shaped_clusters_fit_the_space_the_painter_uses() {
    let fonts = Fonts::new();
    let cluster = "\u{0644}\u{0627}";
    let cell = format!("{cluster} {cluster}");
    let source = format!("| H |\n|---|\n| x x x x x x |\n| {cell} |\n");
    for column in [false, true] {
        let size = if column { 13. } else { 14. };
        let actual = fonts.width(&cell, Face::Regular, size);
        assert!(actual <= 28., "the real shaped clusters fit the 28px cell");
        let geometry = fonts.table(&source, column, 42.);
        assert_eq!(text(&geometry, &cell).2, 1);
    }
}

#[test]
fn md_tables_document_insets_match_the_two_board_surfaces() {
    for column in [false, true] {
        let tokens = super::Tokens {
            body: if column { 13. } else { 14. },
            theme_body: 14.,
            caption: 12.,
            gap: 9.,
            hairline: 1.,
            row_pad_x: 14.,
            row_pad_y: 7.,
            mark: 16.,
            state_mark: 40.,
            source_line: None,
            column,
        };
        assert_eq!(tokens.inset_y(), if column { 20. } else { 18. });
        assert_eq!(tokens.source_inset_y(), 16.);
    }
}
