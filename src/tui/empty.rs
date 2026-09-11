use std::time::Duration;

const SPIRAL: [&str; 10] = [
    "███████████",
    "█         █",
    "█ ███████ █",
    "█ █     █ █",
    "█ █ ███ █ █",
    "█ █ █ █ █ █",
    "█ █ █   █ █",
    "█ █ █████ █",
    "█ █       █",
    "█ █████████",
];
const PHRASES: [&str; 8] = [
    "NOTHING HERE YET",
    "A VERY TIDY DIRECTORY",
    "NOT A FILE IN SIGHT",
    "THIS FOLDER KEEPS ITS SECRETS",
    "QUIET IN HERE",
    "NO CLUTTER TO REPORT",
    "WAITING FOR SOMETHING TO LAND",
    "EMPTY, AND THAT IS FINE",
];
const ROTATE_MS: u128 = 2800;
fn dimensions(width: usize, height: usize, cell: Option<(usize, usize)>) -> (usize, usize) {
    let Some((cell_width, cell_height)) = cell.filter(|(w, h)| *w > 0 && *h > 0) else {
        // ponytail: terminals without pixel geometry retain the caption until measured cells are available.
        return (0, 0);
    };
    let source_width = SPIRAL[0].chars().count();
    let rows = SPIRAL.len().div_ceil(2).min(height.saturating_sub(2))
        .min(width * SPIRAL.len() * cell_width / (source_width * cell_height));
    let columns = (source_width * rows * cell_height + SPIRAL.len() * cell_width / 2)
        / (SPIRAL.len() * cell_width);
    if columns == 0 { (0, 0) } else { (columns, rows) }
}
pub fn line(y: usize, height: usize, width: usize, cell: Option<(usize, usize)>, elapsed: Duration) -> String {
    let phrase = PHRASES[(elapsed.as_millis() / ROTATE_MS) as usize % PHRASES.len()];
    let (columns, rows) = dimensions(width, height, cell);
    let block_height = if rows > 0 { rows + 2 } else { 1 };
    let start = height.saturating_sub(block_height) / 2;
    let text = if y >= start && y < start + rows {
        let source_width = SPIRAL[0].chars().count();
        // Each terminal cell carries two raster rows; measured cell pixels preserve the mark's aspect.
        let filled = |x: usize, half: usize| {
            let source_y = ((y - start) * 2 + half) * SPIRAL.len() / (rows * 2);
            SPIRAL[source_y].chars().nth(x * source_width / columns) == Some('█')
        };
        (0..columns).map(|x| match (filled(x, 0), filled(x, 1)) {
            (true, true) => '█', (true, false) => '▀', (false, true) => '▄', _ => ' ',
        }).collect::<String>()
    } else if y == start + block_height - 1 {
        phrase.into()
    } else {
        String::new()
    };
    let padding = width.saturating_sub(text.chars().count()) / 2;
    super::render::fit(&format!("{}{}", " ".repeat(padding), text), width)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spiral_uses_measured_cells_and_preserves_the_caption() {
        assert_eq!(dimensions(100, 60, Some((9, 20))), (12, 5));
        for cell in [(8, 16), (9, 20), (12, 28), (16, 16)] {
            for width in [5, 12, 100] {
                for height in [1, 3, 8, 60] {
                    let (columns, rows) = dimensions(width, height, Some(cell));
                    assert!(columns <= width && rows + usize::from(rows > 0) * 2 <= height);
                    if rows > 0 {
                        let actual = columns * cell.0 * SPIRAL.len();
                        let expected = SPIRAL[0].chars().count() * rows * cell.1;
                        assert!(actual.abs_diff(expected) <= cell.0 * SPIRAL.len() / 2);
                    }
                    for y in 0..height {
                        assert_eq!(line(y, height, width, Some(cell), Duration::ZERO).chars().count(), width);
                    }
                }
            }
        }
        assert_eq!(dimensions(100, 60, None), (0, 0));
        assert_eq!(line(2, 5, 40, None, Duration::ZERO).trim(), PHRASES[0]);
    }
    #[test]
    fn caption_rotates_at_oem_cadence() {
        assert_ne!(
            line(7, 10, 40, Some((9, 20)), Duration::ZERO),
            line(7, 10, 40, Some((9, 20)), Duration::from_millis(2800))
        );
        assert_eq!(
            line(7, 10, 40, Some((9, 20)), Duration::ZERO),
            line(7, 10, 40, Some((9, 20)), Duration::from_millis(1000))
        );
    }
}
