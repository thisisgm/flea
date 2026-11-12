//! The native picture must reuse the window's faces without a process-wide SVG font scan.
use super::raster;
use crate::{g2::file_clipboard::carry, list::bench, list::Palette, row::marks::Mark};

/// The `<text x>` raster draws the name at; the marks end left of it.
const NAME_LEFT: usize = 47;

#[test]
fn first_drag_never_loads_the_svg_system_font_database() {
    const CHILD: &str = "FLEA_FIRST_DRAG_FONT_TEST";
    if std::env::var_os(CHILD).is_none() {
        let test = concat!(
            module_path!(),
            "::first_drag_never_loads_the_svg_system_font_database"
        )
        .strip_prefix("flea_gui::")
        .unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", test, "--nocapture"])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "fresh-process drag test failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("FIRST_DRAG_FONTS_OK"));
        return;
    }
    let mut cx = bench::real_cx();
    let root = crate::g2::drag_drop::mounted_tests::Root::new("first-drag-fonts");
    let (handle, _) = crate::g2::drag_drop::mounted_tests::open(&mut cx, root.src());
    handle
        .update(&mut cx, |shell, window, cx| {
            let font = shell.active_typeface().font;
            let palette =
                Palette::inherited(&window.text_style(), crate::palette::Theme::default());
            let renderer = cx.svg_renderer();
            assert_eq!(renderer.system_font_database_load_count(), 0);
            for name in ["alpha.txt", "日本語.txt", "مرحبا.txt", "café.txt"] {
                let flock = carry::Flock {
                    marks: vec![Mark::FileText],
                    name: name.into(),
                };
                for _ in 0..2 {
                    let image = raster(&flock, &font, palette, window, cx).unwrap();
                    let width = image.width() as usize;
                    let left = NAME_LEFT * window.scale_factor().ceil() as usize;
                    let mut pixels = image.rgba().as_chunks::<4>().0.iter().enumerate();
                    assert!(
                        pixels.any(|(i, p)| i % width >= left && p[3] > 0),
                        "{name} must draw name glyphs right of the marks"
                    );
                    assert_eq!(
                        renderer.system_font_database_load_count(),
                        0,
                        "drawing {name} must not scan system fonts, even on its first drag"
                    );
                }
            }
        })
        .unwrap();
    println!("FIRST_DRAG_FONTS_OK");
}
