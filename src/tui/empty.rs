use super::cells::{centered, fit};
use std::time::Duration;

const PHRASES: [&str; 3] = ["NOTHING HERE YET", "A VERY TIDY FOLDER", "QUIET IN HERE"];
const ROTATE_MS: u128 = 2800;
pub fn notice(y: usize, height: usize, width: usize, mark: &str, headline: &str) -> String {
    let start = height.saturating_sub(3) / 2;
    if y == start { centered(mark, width) }
    else if y == start + 1 { centered(headline, width) }
    else { fit("", width) }
}
pub fn line(y: usize, height: usize, width: usize, marks: bool, elapsed: Duration) -> String {
    let phrase = PHRASES[(elapsed.as_millis() / ROTATE_MS).min(2) as usize];
    notice(y, height, width, if marks { super::icons::FOLDER } else { "›" }, phrase)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn caption_has_three_beats_and_rests_on_the_last() {
        for (millis, expected) in [(0, PHRASES[0]), (2799, PHRASES[0]), (2800, PHRASES[1]), (5599, PHRASES[1]), (5600, PHRASES[2]), (280000, PHRASES[2])] {
            assert_eq!(line(13, 27, 51, true, Duration::from_millis(millis)).trim(), expected);
        }
    }
    #[test]
    fn empty_notice_matches_board_cells_and_stays_inside_every_pane() {
        assert_eq!(line(12, 27, 51, true, Duration::ZERO).trim(), super::super::icons::FOLDER);
        for width in [1, 5, 12, 51] {
            for height in [1, 3, 27] {
                for y in 0..height { assert_eq!(super::super::cells::text_width(&line(y, height, width, false, Duration::ZERO)), width); }
            }
        }
        assert_eq!(line(12, 27, 51, false, Duration::ZERO).trim(), "›");
    }
}
