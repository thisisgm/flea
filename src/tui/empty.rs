use super::cells::{centered, fit};
use std::time::Duration;

#[cfg(test)]
pub(super) const PHRASES: [&str; 3] = ["NOTHING HERE YET", "A VERY TIDY FOLDER", "QUIET IN HERE"];
pub(super) const ROTATE_MS: u128 = 2800;
// The Codicons search mark, the one the window's no-match notice draws.
const SEARCH: &str = "\u{ea6d}";
pub fn notice(y: usize, height: usize, width: usize, mark: &str, headline: &str) -> String {
    let start = height.saturating_sub(3) / 2;
    if y == start { centered(mark, width) }
    else if y == start + 1 { centered(headline, width) }
    else { fit("", width) }
}
// A filter that matches nothing shows the search mark over its sentence, in the empty notice's place.
pub fn no_match(y: usize, height: usize, width: usize, marks: bool, headline: &str) -> String {
    notice(y, height, width, if marks { SEARCH } else { "›" }, headline)
}
// Whether a frame of this size draws the empty caption, the one test the frame and the loop's wake share.
pub(super) fn shown(m: &super::model::Model, columns: usize, lines: usize) -> bool {
    columns >= 12 && lines > super::render::CHROME_ROWS && m.total == 0 && m.pending.is_none() && super::trashview::settled(m)
}
// Captions loop while the empty folder is shown; each appearance supplies a fresh elapsed clock.
#[cfg(test)]
pub fn line(y: usize, height: usize, width: usize, marks: bool, elapsed: Duration) -> String {
    localized(y, height, width, marks, elapsed, &crate::i18n::test_catalogs::english())
}
pub fn localized(y: usize, height: usize, width: usize, marks: bool, elapsed: Duration, i18n: &crate::i18n::I18n) -> String {
    let keys = ["pane.empty.initial", "pane.empty.beat_one", "pane.empty.rest"];
    let phrase = i18n.tr(keys[(elapsed.as_millis() / ROTATE_MS % keys.len() as u128) as usize]);
    notice(y, height, width, if marks { super::icons::FOLDER } else { "›" }, &phrase)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn captions_loop_three_cycles_and_stay_fixed_between_beats() {
        let mut last = line(13, 27, 51, true, Duration::ZERO);
        let cycle_ms = PHRASES.len() as u64 * ROTATE_MS as u64;
        let sample_ms = ROTATE_MS as u64 / 4;
        for cycle in 0..3 {
            let mut paints = 0;
            for step in 1..=cycle_ms / sample_ms {
                let millis = cycle * cycle_ms + step * sample_ms;
                let row = line(13, 27, 51, true, Duration::from_millis(millis));
                assert_eq!(row.trim(), PHRASES[(millis as u128 / ROTATE_MS) as usize % PHRASES.len()]);
                if row != last {
                    assert_eq!(millis as u128 % ROTATE_MS, 0, "no terminal paint between beats");
                    paints += 1;
                }
                last = row;
            }
            assert_eq!(paints, PHRASES.len());
            eprintln!("TUI_CAPTION_CYCLE cycle={} caption_paints={paints} idle_paints=0", cycle + 1);
        }
        assert_eq!(line(13, 27, 51, true, Duration::ZERO).trim(), PHRASES[0]);
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
    // The board centres the search mark at row 14 and NOTHING MATCHES zzz at row 15 of a 27-row pane 51 cells wide.
    #[test]
    fn no_match_notice_draws_the_search_mark_over_its_sentence() {
        assert_eq!(no_match(12, 27, 51, true, "NOTHING MATCHES zzz"), format!("{}\u{ea6d}{}", " ".repeat(25), " ".repeat(25)));
        assert_eq!(no_match(13, 27, 51, true, "NOTHING MATCHES zzz"), format!("{}NOTHING MATCHES zzz{}", " ".repeat(16), " ".repeat(16)));
        assert_eq!(no_match(12, 27, 51, false, "x").trim(), "›");
        assert_eq!(no_match(14, 27, 51, true, "x").trim(), "");
    }
}
