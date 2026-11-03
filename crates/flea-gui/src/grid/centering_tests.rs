//! Grid pictures and marks sit on the whole pixel v0.3.12's anchors centre them on.
use super::{deliver, first_settle, open, read, step, thumbed};
use gpui::TestAppContext;

// A flat PNG of one size, written where the decode can read it, as a thumbnailer's cache file is.
fn flat_png(name: &str, width: u32, height: u32) -> String {
    let dir = centering_dir();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(".flea-test-sandbox"), "").unwrap();
    let path = dir.join(name);
    image::RgbaImage::from_pixel(width, height, image::Rgba([43, 88, 118, 255]))
        .save(&path)
        .unwrap();
    path.to_string_lossy().into_owned()
}

fn centering_dir() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("flea-grid-centering-{}", std::process::id()))
}

// Qt's anchors.horizontalCenter with alignWhenCentered: an odd width's centre is rounded up to a whole pixel.
fn qt_half(width: f32) -> f32 {
    let whole = width as i32;
    if whole % 2 == 1 {
        (width + 1.) / 2.
    } else {
        width / 2.
    }
}

#[gpui::test]
fn pictures_and_marks_sit_on_the_whole_pixel_qt_centres_them_on_in_odd_and_even_cells(
    cx: &mut TestAppContext,
) {
    // The HIG grid board's literal, independent of either rounding helper: a 64 px slot 42 px into a 147 px cell.
    assert_eq!(crate::grid::geometry::centred_x(147., 64.), 42.);
    assert_eq!(qt_half(147.) - qt_half(64.), 42.);
    let (handle, mut lines) = open(cx, "{}");
    first_settle(cx, handle, &mut lines);
    // Cache-edge thumbnails stand in for larger originals, so each fills the 64 px slot.
    deliver(cx, handle, &thumbed(0, &flat_png("square.png", 256, 256)));
    deliver(cx, handle, &thumbed(1, &flat_png("wide.png", 256, 144)));
    cx.run_until_parked();
    step(cx, handle);
    let mut parities = [false; 2];
    for width in [1100., 1101., 1102., 1103., 1200., 1201.] {
        {
            let visual = gpui::VisualTestContext::from_window(handle.into(), cx);
            visual.simulate_resize(gpui::size(gpui::px(width), gpui::px(700.)));
        }
        step(cx, handle);
        read(cx, handle, |shell| {
            let grid = shell.panes[0].as_ref().unwrap().grid.as_ref().unwrap();
            let state = grid.state.borrow();
            for (row, size) in [(0, (64., 64.)), (1, (64., 36.)), (2, (64., 64.))] {
                let tile = state.drawn_row(row).expect("the row is a drawn tile");
                let cell = tile.cell;
                parities[cell.width as usize % 2] = true;
                // Row 2 keeps its kind mark, which fills the slot the picture would.
                let drawn = if row == 2 {
                    Some(tile.slot)
                } else {
                    tile.picture
                };
                let drawn = drawn.unwrap_or_else(|| panic!("{width}: row {row} draws its picture"));
                assert_eq!((drawn.width, drawn.height), size, "{width}: row {row}");
                assert_eq!(
                    (drawn.x, drawn.y),
                    (
                        cell.x + qt_half(cell.width) - qt_half(size.0),
                        tile.slot.y + ((64. - size.1) / 2.).floor()
                    ),
                    "{width}: row {row} in a {} px cell sits where v0.3.12's anchors put it",
                    cell.width
                );
                assert_eq!(
                    (drawn.x.fract(), drawn.y.fract()),
                    (0., 0.),
                    "{width}: row {row} on whole pixels"
                );
            }
        });
    }
    assert_eq!(
        parities,
        [true, true],
        "odd and even cell widths are both covered"
    );
    let dir = centering_dir();
    if dir.is_absolute() && dir.join(".flea-test-sandbox").is_file() {
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
