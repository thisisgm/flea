//! Legend sizes: text size 14 draws the board's sizes, and a larger text size scales every size and floor with it.
use super::legend_sizes;

fn near(got: (f32, f32), want: (f32, f32)) -> bool {
    (got.0 - want.0).abs() < 1e-3 && (got.1 - want.1).abs() < 1e-3
}

#[test]
fn text_size_14_keeps_the_boards_legends() {
    // A letter at a third of the unit, a name at a quarter floored at 7, a shifted legend at a fifth floored at 8.
    assert!(near(legend_sizes("A", 1., 40., 1.), (12., 8.8)));
    assert!(near(legend_sizes("A", 1., 28., 1.), (8.4, 8.)));
    assert!(near(legend_sizes("TAB", 1.5, 40., 1.), (9.6, 8.8)));
    // The smallest unit squeezes a long name to its floor.
    assert!(near(legend_sizes("CAPS", 1.75, 28., 1.), (7., 8.)));
}

#[test]
fn a_larger_text_size_scales_the_legends_and_their_floors() {
    // Text size 20's caption is 17: the unit grows to 56, and every legend with it.
    let scale = 17. / 12.;
    let (letter, shifted) = legend_sizes("A", 1., 56., scale);
    assert!((letter - 16.8).abs() < 1e-3, "a letter at {letter}");
    assert!(
        (shifted - 12.32).abs() < 1e-3,
        "a shifted legend at {shifted}"
    );
    // The smallest unit at that text size keeps its floors in step: never the 7 and 8 of text size 14.
    let (name, shifted) = legend_sizes("CAPS", 1.75, 40., scale);
    assert!((name - 7. * scale).abs() < 1e-3, "a name at {name}");
    assert!(
        (shifted - 8. * scale).abs() < 1e-3,
        "a shifted legend at {shifted}"
    );
}
