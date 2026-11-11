use super::*;
use crate::backend::{combine::{self, Background, Layout, Settings, Spacing}, convertbatch, printtool};

#[path = "print_review_tests.rs"]
mod review;

#[path = "convert_codec_real_tests.rs"]
mod codecs;

fn installed(name: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).any(|p| p.join(name).metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
}

fn ready(tools: &[&str]) -> bool {
    let missing: Vec<_> = tools.iter().filter(|name| !installed(name)).copied().collect();
    if missing.is_empty() { return true; }
    writeln!(std::io::stderr(), "SKIP print-convert real tools: {} not installed", missing.join(", ")).ok();
    false
}

fn tool(dir: &TestDir, argv: Vec<String>) -> String {
    // Only these known fixtures use the raw argv, so CLI compatibility is proved even where bwrap cannot run.
    for arg in &argv {
        if arg.starts_with('/') { dir.assert_contains(std::path::Path::new(arg.trim_end_matches("[0]"))); }
    }
    printtool::run(&argv, &AtomicBool::new(false), printtool::TOOL_WAIT).unwrap_or_else(|e| panic!("{argv:?}: {}", e.message))
}

fn ppm(dir: &TestDir, name: &str, width: usize, height: usize, pixel: &str) -> PathBuf {
    dir.file(name, &format!("P3\n{width} {height}\n255\n{}", pixel.repeat(width * height)))
}

fn pdf(dir: &TestDir) -> PathBuf {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R 5 0 R 7 0 R] /Count 3 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 72 72] /Contents 4 0 R >>",
        "<< /Length 0 >>\nstream\n\nendstream",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 72 72] /Contents 6 0 R >>",
        "<< /Length 0 >>\nstream\n\nendstream",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 72 72] /Contents 8 0 R >>",
        "<< /Length 0 >>\nstream\n\nendstream",
    ];
    let mut body = "%PDF-1.4\n".to_string();
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(body.len());
        body.push_str(&format!("{} 0 obj\n{object}\nendobj\n", i + 1));
    }
    let xref = body.len();
    body.push_str("xref\n0 9\n0000000000 65535 f \n");
    for offset in offsets { body.push_str(&format!("{offset:010} 00000 n \n")); }
    body.push_str(&format!("trailer\n<< /Size 9 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n"));
    dir.file("report.pdf", &body)
}

#[test]
fn print_real_magick_and_montage_match_the_layout_geometry() {
    if !ready(&["magick", "montage"]) { return; }
    let d = TestDir::new("print-real-combine");
    let a = ppm(&d, "first.ppm", 150, 100, "255 0 0\n");
    let b = ppm(&d, "second.ppm", 100, 200, "0 0 255\n");
    for (index, layout) in [Layout::Row, Layout::Column, Layout::Grid].iter().enumerate() {
        let settings = Settings { layout: *layout, spacing: Spacing::Small, background: Background::White, columns: 2 };
        let size = combine::shared_size(*layout, &[(150, 100), (100, 200)]).unwrap();
        let scaled: Vec<_> = [&a, &b].iter().enumerate().map(|(n, input)| {
            let out = d.join(&format!("scaled-{index}-{n}.png"));
            tool(&d, combine::scale_argv(input, &out, size, &settings, n == 0));
            out
        }).collect();
        let expected = match layout { Layout::Row => (209, 106), Layout::Column => (106, 276), Layout::Grid => (312, 208) };
        for format in ["png", "jpg", "webp"] {
            let out = d.join(&format!("combined-{index}.{format}"));
            for command in combine::join_commands(&scaled, &out, size, &settings) { tool(&d, command); }
            assert_eq!(crate::backend::imagesize::dimensions(&out).unwrap(), expected);
        }
        if ready(&["pdftoppm"]) {
            let out = d.join(&format!("combined-{index}.pdf"));
            for command in combine::join_commands(&scaled, &out, size, &settings) { tool(&d, command); }
            assert!(std::fs::read(&out).unwrap().starts_with(b"%PDF"));
            // ImageMagick writes the composite as one 72 dpi page, so a 72 dpi render returns its pixels.
            let prefix = d.join(&format!("combined-{index}-page"));
            tool(&d, vec!["pdftoppm".into(), "-r".into(), "72".into(), "-f".into(), "1".into(), "-singlefile".into(), "-png".into(), "--".into(), out.to_string_lossy().into_owned(), prefix.to_string_lossy().into_owned()]);
            assert_eq!(crate::backend::imagesize::dimensions(&prefix.with_extension("png")), Some(expected));
        }
    }
    let out = d.join("converted.png");
    tool(&d, convertbatch::image_argv(&[a], &out, true));
    assert_eq!(crate::backend::imagesize::dimensions(&out), Some((150, 100)));
}

#[test]
fn print_real_pdftoppm_names_and_renders_each_codec_at_150_dpi() {
    if !ready(&["pdftoppm", "pdfinfo"]) { return; }
    let d = TestDir::new("print-real-pdf");
    let input = pdf(&d);
    let info = tool(&d, vec!["pdfinfo".into(), "--".into(), input.to_string_lossy().into_owned()]);
    assert!(info.lines().any(|l| l.strip_prefix("Pages:").is_some_and(|s| s.trim() == "3")));
    for format in ["png", "jpg", "tiff"] {
        let prefix = d.join(&format!("page-{format}"));
        tool(&d, convertbatch::pdf_argv(&input, &prefix, 3, format));
        let output = prefix.with_extension(if format == "tiff" { "tif" } else { format });
        let bytes = std::fs::read(&output).unwrap();
        assert!(!bytes.is_empty());
        if format != "tiff" { assert_eq!(crate::backend::imagesize::dimensions(&output), Some((150, 150))); }
    }
    if ready(&["magick"]) {
        let webp = d.join("page.webp");
        tool(&d, convertbatch::image_argv(&[d.join("page-png.png")], &webp, false));
        assert_eq!(crate::backend::imagesize::dimensions(&webp), Some((150, 150)));
    }
}

#[test]
fn print_real_images_to_pdf_keep_one_page_per_image_in_selection_order() {
    if !ready(&["magick", "pdfinfo", "pdftoppm"]) { return; }
    let d = TestDir::new("print-real-image-pdf");
    let first = ppm(&d, "first.ppm", 10, 10, "255 0 0\n");
    let second = ppm(&d, "second.ppm", 10, 10, "0 0 255\n");
    let output = d.join("combined.pdf");
    tool(&d, convertbatch::image_argv(&[first.clone(), second.clone()], &output, false));
    assert!(std::fs::read(&output).unwrap().starts_with(b"%PDF"));
    let info = tool(&d, vec!["pdfinfo".into(), "--".into(), output.to_string_lossy().into_owned()]);
    assert!(info.lines().any(|l| l.strip_prefix("Pages:").is_some_and(|s| s.trim() == "2")));
    for (page, color) in [(1, "255,0,0"), (2, "0,0,255")] {
        let prefix = d.join(&format!("page-{page}"));
        tool(&d, convertbatch::pdf_argv(&output, &prefix, page, "png"));
        let pixel = tool(&d, vec!["magick".into(), "--".into(), prefix.with_extension("png").to_string_lossy().into_owned(), "-format".into(), "%[pixel:p{0,0}]".into(), "info:".into()]);
        assert!(pixel.contains(color), "page {page}: {pixel}");
    }
    assert!(std::fs::read_to_string(first).unwrap().starts_with("P3"));
    assert!(std::fs::read_to_string(second).unwrap().starts_with("P3"));
}

#[test]
fn print_real_protocol_converters_run_in_the_production_jail() {
    if !ready(&["magick", "montage", "pdftoppm", "pdfinfo"]) || crate::backend::sandboxprobe::skipped() { return; }
    let mut rig = Rig::new(false);
    let first = ppm(&rig.dir, "first.ppm", 150, 100, "255 0 0\n");
    let second = ppm(&rig.dir, "second.ppm", 100, 200, "0 0 255\n");
    let command = format!(r#"{{"c":"combine","paths":["{}","{}"],"format":"png","layout":"Row","spacing":"Small","background":"White"}}"#, first.display(), second.display());
    let replies = rig.ask(&command, "combinedone");
    assert!(field_bool(replies.last().unwrap(), "ok"), "{replies:?}");
    assert_eq!(crate::backend::imagesize::dimensions(&rig.dir.join("first (combined).png")), Some((209, 106)));
    let command = format!(r#"{{"c":"combine","paths":["{}","{}"],"format":"pdf","layout":"Row","spacing":"Small","background":"White"}}"#, first.display(), second.display());
    let replies = rig.ask(&command, "combinedone");
    assert!(field_bool(replies.last().unwrap(), "ok"), "{replies:?}");
    let combined = rig.dir.join("first (combined).pdf");
    assert!(std::fs::read(&combined).unwrap().starts_with(b"%PDF"));
    let info = tool(&rig.dir, vec!["pdfinfo".into(), "--".into(), combined.to_string_lossy().into_owned()]);
    assert!(info.lines().any(|l| l.strip_prefix("Pages:").is_some_and(|s| s.trim() == "1")), "{info}");
    assert!(info.lines().any(|l| l.strip_prefix("Page size:").is_some_and(|s| s.trim().starts_with("209 x 106 pts"))), "{info}");
    let input = pdf(&rig.dir);
    let command = format!(r#"{{"c":"convertbatch","path":"{}","format":"png","pages":"1, 3"}}"#, input.display());
    let replies = rig.ask(&command, "convertdone");
    assert!(field_bool(replies.last().unwrap(), "ok"), "{replies:?}");
    for page in [1, 3] { assert_eq!(crate::backend::imagesize::dimensions(&rig.dir.join(&format!("report (page {page}).png"))), Some((150, 150))); }
}

// Print's input step with a test-cache service in place of the shared one: the office leg converts on a production-built service and the office jail.
#[test]
#[ignore = "needs LibreOffice; tests/print-office-real.sh runs it on minipc"]
fn print_real_office_leg_converts_through_the_office_service() {
    let d = TestDir::new("print-real-office");
    let fixture = "{\\rtf1\\ansi Print and convert fixture.}";
    let source = d.file("letter.rtf", fixture);
    let cache = d.dir("cache");
    let fetch = |p: &std::path::Path, c| crate::backend::officepdf::cached_pdf_in(&cache, p, c);
    let pdf = print_input_with(fetch, &source, printconvert::Kind::Office, &Arc::new(AtomicBool::new(false)))
        .unwrap_or_else(|e| panic!("office conversion refused: {e}"));
    assert!(pdf.starts_with(&cache), "the PDF is served from the test's own cache: {}", pdf.display());
    assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF"), "the office service returned no PDF");
    assert_eq!(std::fs::read_to_string(&source).unwrap(), fixture, "conversion changed its source");
}

// magick expands %d anywhere in an output path, so a folder named scan%d sent a convert or a combine into scan0.
#[test]
fn print_real_a_percent_folder_keeps_every_magick_output_where_it_was_asked() {
    if !ready(&["magick", "montage"]) {
        return;
    }
    let d = TestDir::new("print-real-percent");
    std::fs::create_dir_all(d.join("scan%d")).unwrap();
    std::fs::create_dir_all(d.join("scan0")).unwrap();
    let a = ppm(&d, "a.ppm", 4, 2, "255 0 0\n");
    // Upstream ImageMagick 7.1.1 keeps %% literally and names the doubled path in its refusal; 7.1.2 unescapes it.
    let probe = d.join("scan%d/probe%d.png");
    let done = std::process::Command::new("magick")
        .arg(&a)
        .arg(format!("png:{}", crate::backend::printconvert::magick_output(&probe)))
        .output()
        .unwrap_or_else(|e| panic!("magick percent probe could not start: {e}"));
    let refusal = String::from_utf8_lossy(&done.stderr);
    if !done.status.success() && refusal.contains("scan%%d/probe%%d.png") {
        writeln!(std::io::stderr(), "SKIP print-convert percent paths: this magick does not unescape %%").ok();
        return;
    }
    assert!(done.status.success() && probe.is_file(), "magick percent probe missed {probe:?}: {refusal}");
    std::fs::remove_file(&probe).unwrap();
    let run = |argv: Vec<String>| {
        printtool::run(&argv, &AtomicBool::new(false), printtool::TOOL_WAIT)
            .unwrap_or_else(|e| panic!("{argv:?}: {}", e.message));
    };
    let converted = d.join("scan%d/c%d.png");
    run(convertbatch::image_argv(
        std::slice::from_ref(&a),
        &converted,
        false,
    ));
    let settings = Settings {
        layout: Layout::Grid,
        spacing: Spacing::Small,
        background: Background::White,
        columns: 2,
    };
    let scaled = d.join("scan%d/s%d.png");
    run(combine::scale_argv(&a, &scaled, (4, 2), &settings, true));
    let joined = d.join("scan%d/j%d.png");
    for command in combine::join_commands(
        &[scaled.clone(), scaled.clone()],
        &joined,
        (4, 2),
        &settings,
    ) {
        run(command);
    }
    for path in [&converted, &scaled, &joined] {
        assert!(path.is_file(), "{path:?} was not written");
    }
    assert_eq!(
        std::fs::read_dir(d.join("scan0")).unwrap().count(),
        0,
        "nothing lands in scan0"
    );
}
