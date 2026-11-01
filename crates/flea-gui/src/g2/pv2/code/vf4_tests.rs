use super::*;
use gpui::{black, px};

fn view(numbered: bool) -> CodeView {
    let ink = black();
    let mut view = CodeView::new(
        "look",
        Typeface {
            font: gpui::font("JetBrains Mono"),
            body: px(12.),
            caption: px(12.),
        },
        Palette {
            foreground: ink,
            muted: ink,
            muted_frame: ink,
            gutter: ink,
            inks: [ink; 7],
        },
        Metrics {
            inset: 10.,
            top: 11.,
            right: 10.,
            row: 17.,
            gap: 10.,
            state_mark: 16.,
        },
        (SharedString::from("file-text"), &[]),
    );
    view.numbered = numbered;
    view
}

#[test]
fn vf4_plain_text_keeps_the_two_cell_lead_without_a_gutter() {
    // Board filter draws notes.txt 23 px into its frame and ql-text at 14 + 2ch; both are the inset plus two advances.
    let advance = px(7.2);
    assert_eq!(view(false).text_left(advance), 24.);
    assert_eq!(view(false).gutter_right(advance), 10.);
    // A numbered view keeps the same two-cell lead past its gutter, up to the final whole-pixel rounding.
    let numbered = view(true);
    let lead = numbered.text_left(advance) - numbered.gutter_right(advance);
    assert!((lead - 2. * 7.2).abs() <= 0.5, "{lead}");
}
