use super::*;
use std::path::Path;

fn image_pages(dir: &TestDir, path: &Path) -> usize {
    tool(dir, vec!["magick".into(), "--".into(), path.display().to_string(), "-format".into(), "%n\n".into(), "info:".into()]).lines().count()
}

#[test]
fn print_review_round2_pdf_has_one_page_per_selected_image() {
    if !ready(&["magick", "pdfinfo", "pdftoppm"]) { return; }
    let d = TestDir::new("print-review-pdf-selection");
    let red = ppm(&d, "red.ppm", 10, 10, "255 0 0\n");
    let blue = ppm(&d, "blue.ppm", 10, 10, "0 0 255\n");
    let green = ppm(&d, "green.ppm", 10, 10, "0 255 0\n");
    let tiff = d.join("two-frames.tiff");
    tool(&d, vec!["magick".into(), "--".into(), red.display().to_string(), "--".into(), blue.display().to_string(), tiff.display().to_string()]);
    assert_eq!(image_pages(&d, &tiff), 2);
    let retained = d.join("retained.tiff");
    tool(&d, convertbatch::image_argv(std::slice::from_ref(&tiff), &retained, true));
    assert_eq!(image_pages(&d, &retained), 2, "existing TIFF targets keep every frame");
    let mut counts = Vec::new();
    for (n, selected) in [vec![tiff.clone()], vec![tiff.clone(), green]].iter().enumerate() {
        let output = d.join(&format!("selected-{n}.pdf"));
        tool(&d, convertbatch::image_argv(selected, &output, false));
        let info = tool(&d, vec!["pdfinfo".into(), "--".into(), output.display().to_string()]);
        counts.push(info.lines().find_map(|line| line.strip_prefix("Pages:").and_then(|count| count.trim().parse::<usize>().ok())).unwrap());
    }
    println!("PDF counts for one / two selected images: {counts:?}; TIFF control: 2 frames");
    assert_eq!(counts, [1, 2], "PDF requires one page per selected image in selection order");
    for (page, color) in [(1, "255,0,0"), (2, "0,255,0")] {
        let prefix = d.join(&format!("selected-page-{page}"));
        tool(&d, convertbatch::pdf_argv(&d.join("selected-1.pdf"), &prefix, page, "png"));
        let pixel = tool(&d, vec!["magick".into(), "--".into(), prefix.with_extension("png").display().to_string(), "-format".into(), "%[pixel:p{0,0}]".into(), "info:".into()]);
        assert!(pixel.contains(color), "PDF page {page} must be the first frame of its selected input: {pixel}");
    }
    assert_eq!(image_pages(&d, &tiff), 2, "the source remains intact");
}

#[test]
fn print_review_multipage_tiff_preserves_every_page() {
    if !ready(&["magick", "pdfinfo"]) { return; }
    let d = TestDir::new("print-review-tiff");
    let first = ppm(&d, "red.ppm", 10, 10, "255 0 0\n");
    let second = ppm(&d, "blue.ppm", 10, 10, "0 0 255\n");
    let source = d.join("two-pages.tiff");
    tool(&d, vec!["magick".into(), "--".into(), first.display().to_string(), "--".into(), second.display().to_string(), source.display().to_string()]);
    assert_eq!(image_pages(&d, &source), 2);
    let legacy = d.join("legacy.tiff");
    tool(&d, crate::backend::convert::argv(&source, &legacy, true));
    assert_eq!(image_pages(&d, &legacy), 2, "the shipped converter preserves both pages");
    let output = d.join("converted.tiff");
    tool(&d, convertbatch::image_argv(std::slice::from_ref(&source), &output, true));
    assert_eq!(image_pages(&d, &output), 2, "Convert must preserve every TIFF page");
    let converted_pdf = d.join("converted.pdf");
    tool(&d, convertbatch::image_argv(std::slice::from_ref(&source), &converted_pdf, false));
    let page_word = tool(&d, vec!["pdfinfo".into(), "--".into(), converted_pdf.display().to_string()]);
    assert!(page_word.lines().any(|line| line.strip_prefix("Pages:").is_some_and(|count| count.trim() == "1")), "PDF contributes one page for this selected TIFF");
    let png = d.join("fanout.png");
    tool(&d, convertbatch::image_argv(std::slice::from_ref(&source), &png, true));
    let destination = d.join("two-pages (converted).png");
    let pages = convertbatch::image_results(&png, &destination, &source).unwrap();
    assert_eq!(pages.len(), 2, "single-page targets publish every page");
    for (n, ((staged, dest), color)) in pages.iter().zip(["255,0,0", "0,0,255"]).enumerate() {
        assert_eq!(dest, &d.join(&format!("two-pages (converted) (page {}).png", n + 1)));
        let pixel = tool(&d, vec!["magick".into(), "--".into(), staged.display().to_string(), "-format".into(), "%[pixel:p{0,0}]".into(), "info:".into()]);
        assert!(pixel.contains(color), "page {}: {pixel}", n + 1);
        d.assert_contains(staged);
        d.assert_contains(dest);
        printtool::publish(staged, dest, &std::sync::atomic::AtomicBool::new(false)).unwrap();
    }
    assert_eq!(image_pages(&d, &source), 2, "source keeps both pages");
    println!("TIFF retains 2 pages; PDF uses frame zero; PNG publishes both pages in red/blue order; source retains 2 pages");
}

#[test]
fn print_review_combine_pixels_match_board_gutters() {
    if !ready(&["magick", "montage"]) { return; }
    let d = TestDir::new("print-review-gutters");
    let colors = [[255, 0, 0, 255], [0, 0, 255, 255], [0, 255, 0, 255], [255, 0, 255, 255]];
    let inputs: Vec<_> = ["255 0 0\n", "0 0 255\n", "0 255 0\n", "255 0 255\n"].iter().enumerate()
        .map(|(n, color)| ppm(&d, &format!("source-{n}.ppm"), 100, 100, color)).collect();
    let mut case = 0;
    for (layout, count, columns, rows) in [(Layout::Row, 2, 2, 1), (Layout::Column, 2, 1, 2), (Layout::Grid, 4, 2, 2), (Layout::Grid, 3, 2, 2)] {
        for (spacing, gap) in [(Spacing::Small, 3), (Spacing::Large, 8), (Spacing::None, 0)] {
            for background in [Background::White, Background::Black, Background::Transparent] {
                let settings = Settings { layout, spacing, background, columns: 2 };
                let scaled: Vec<_> = inputs[..count].iter().enumerate().map(|(n, input)| {
                    let output = d.join(&format!("tile-{case}-{n}.png"));
                    tool(&d, combine::scale_argv(input, &output, (100, 100), &settings, n == 0));
                    output
                }).collect();
                let output = d.join(&format!("output-{case}.png"));
                for command in combine::join_commands(&scaled, &output, (100, 100), &settings) { tool(&d, command); }
                let width = columns * 100 + (columns + 1) * gap;
                let height = rows * 100 + (rows + 1) * gap;
                assert_eq!(crate::backend::imagesize::dimensions(&output), Some((width as u32, height as u32)), "{layout:?} {spacing:?}: even outside gutters and one gap");
                let raw = d.join(&format!("pixels-{case}.rgba"));
                d.assert_contains(&raw);
                tool(&d, vec!["magick".into(), "--".into(), output.display().to_string(), "-depth".into(), "8".into(), format!("rgba:{}", raw.display())]);
                let pixels = std::fs::read(raw).unwrap();
                assert_eq!(pixels.len(), width * height * 4);
                for y in 0..height {
                    for x in 0..width {
                        let tile = x.checked_sub(gap).zip(y.checked_sub(gap)).and_then(|(x, y)| {
                            let (column, row) = (x / (100 + gap), y / (100 + gap));
                            (column < columns && row < rows && x % (100 + gap) < 100 && y % (100 + gap) < 100)
                                .then_some(row * columns + column).filter(|n| *n < count)
                        });
                        let pixel = &pixels[(y * width + x) * 4..][..4];
                        if let Some(tile) = tile {
                            assert_eq!(pixel, colors[tile], "{layout:?} {spacing:?}: tile {tile} at ({x},{y})");
                        } else {
                            match background {
                                Background::White => assert_eq!(pixel, [255; 4], "white gutter at ({x},{y})"),
                                Background::Black => assert_eq!(pixel, [0, 0, 0, 255], "black gutter at ({x},{y})"),
                                Background::Transparent => assert_eq!(pixel[3], 0, "transparent gutter at ({x},{y})"),
                            }
                        }
                    }
                }
                case += 1;
            }
        }
    }
    assert_eq!(case, 36);
    println!("36 compositions checked pixel by pixel: gaps and outside gutters 0, 3, 8 px; dimensions 100 to 224 px per axis");
}
