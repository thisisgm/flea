//! Assertions inspect actual codec bytes, dimensions, metadata and page content.
use super::{convert_process_tests::*, input::Key, wire::Wire};
use std::{io::Write, path::Path};

fn identify(path: &Path, format: &str) -> String {
    String::from_utf8(tool(&[
        "magick",
        "identify",
        "-format",
        format,
        path.to_str().unwrap(),
    ]))
    .unwrap()
}
fn has_pages(path: &Path, n: usize) {
    let info = String::from_utf8(tool(&["pdfinfo", path.to_str().unwrap()])).unwrap();
    assert!(info.lines().any(|l| l
        .strip_prefix("Pages:")
        .is_some_and(|s| s.trim() == n.to_string())));
    assert!(std::fs::read(path).unwrap().starts_with(b"%PDF-"));
}
pub fn formats(root: &Path, w: &mut Wire) {
    let files = root.join("files");
    let source = fixture(&files, "photo.png", "red");
    let original = std::fs::read(&source).unwrap();
    eprintln!(
        "codec runtime: {}",
        String::from_utf8_lossy(&tool(&["magick", "-version"]))
    );
    let mut codec_errors = Vec::new();
    for (choice, format) in flea::ui_logic_print_convert::output::IMAGE_FORMATS
        .iter()
        .enumerate()
    {
        let mut m = open(&files, w, &["photo.png"]);
        for _ in 0..choice {
            press(&mut m, w, Key::character('j', ""));
        }
        assert_eq!(m.converting.card.as_ref().unwrap().format(), *format);
        if *format != "pdf" {
            press(&mut m, w, Key::named("Tab", ""));
            press(&mut m, w, Key::named("Space", ""));
            press(&mut m, w, Key::named("Tab", "shift"));
        }
        submit(&mut m, w);
        assert!(m.error.is_empty(), "{format}: {}", m.error);
        let output = files.join(flea::ui_logic_print_convert::output::converted_name(
            &source, format,
        ));
        let bytes = std::fs::read(&output).unwrap();
        assert_ne!(
            bytes, original,
            "source bytes must not be copied as a new codec"
        );
        if *format == "pdf" {
            has_pages(&output, 1);
        } else {
            let facts = identify(&output, "%m %w %h %[comment]");
            let codec = match *format {
                "jpg" => "JPEG",
                "heic" => "HEIC",
                "tiff" => "TIFF",
                "webp" => "WEBP",
                "avif" => "AVIF",
                "bmp" => "BMP",
                _ => "PNG",
            };
            eprintln!(
                "output {format}: {facts}; first bytes {:?}",
                &bytes[..bytes.len().min(16)]
            );
            if !facts.starts_with(&format!("{codec} 12 8")) {
                codec_errors.push(format!("{format}: {facts}"));
            }
            assert!(
                !facts.contains("retained-metadata"),
                "{format}: metadata was not removed"
            );
        }
        assert_eq!(std::fs::read(&source).unwrap(), original);
    }
    assert!(
        codec_errors.is_empty(),
        "shared converter codec mismatch: {codec_errors:?}"
    );
}
pub fn pages(root: &Path, w: &mut Wire) {
    let files = root.join("files");
    let source = pdf(&files, "report.pdf");
    let original = std::fs::read(&source).unwrap();
    for (choice, format) in flea::ui_logic_print_convert::output::PDF_FORMATS
        .iter()
        .enumerate()
    {
        let mut m = open(&files, w, &["report.pdf"]);
        until(&mut m, w, |m| {
            m.converting
                .card
                .as_ref()
                .is_some_and(|f| f.count.is_some() || !f.error.is_empty())
        });
        assert_eq!(m.converting.card.as_ref().unwrap().count, Some(3));
        for _ in 0..choice {
            press(&mut m, w, Key::character('j', ""));
        }
        press(&mut m, w, Key::named("Tab", ""));
        for c in "3, 1".chars() {
            press(&mut m, w, Key::character(c, ""));
        }
        submit(&mut m, w);
        assert!(m.error.is_empty(), "{format}: {}", m.error);
        for page in [1, 3] {
            let output = files.join(format!("report (page {page}).{format}"));
            let facts = identify(&output, "%w %h %[fx:round(255*p{75,75}.r)] %[fx:round(255*p{75,75}.g)] %[fx:round(255*p{75,75}.b)]");
            assert!(facts.starts_with("150 150 "), "150 dpi: {facts}");
            let channels: Vec<i32> = facts
                .split_whitespace()
                .skip(2)
                .map(|c| c.parse().unwrap())
                .collect();
            let wanted = if page == 1 { [255, 0, 0] } else { [0, 0, 255] };
            assert_eq!(channels.len(), 3);
            assert!(
                channels
                    .iter()
                    .zip(wanted)
                    .all(|(actual, expected)| (*actual - expected).abs() <= 3),
                "page identity: {facts}"
            );
        }
        assert!(!files.join(format!("report (page 2).{format}")).exists());
    }
    assert_eq!(std::fs::read(source).unwrap(), original);
}
pub fn order(root: &Path, w: &mut Wire) {
    let files = root.join("files");
    fixture(&files, "a.png", "red");
    fixture(&files, "b.png", "blue");
    for combine in [true, false] {
        let mut m = open(&files, w, &["a.png", "b.png"]);
        for _ in 0..7 {
            press(&mut m, w, Key::named("Down", ""));
        }
        assert!(m.converting.card.as_ref().unwrap().combine);
        if !combine {
            press(&mut m, w, Key::named("Tab", ""));
            press(&mut m, w, Key::named("Space", ""));
            press(&mut m, w, Key::named("Tab", "shift"));
            let moved = files.join("combined.pdf");
            std::fs::rename(files.join("b (converted).pdf"), moved).unwrap();
        }
        submit(&mut m, w);
        assert!(m.error.is_empty(), "{}", m.error);
        let output = files.join("b (converted).pdf");
        has_pages(&output, if combine { 2 } else { 1 });
        if combine {
            for (page, color) in [(1, "0,0,255"), (2, "255,0,0")] {
                let prefix = files.join(format!("check-{page}"));
                tool(&[
                    "pdftoppm",
                    "-f",
                    &page.to_string(),
                    "-l",
                    &page.to_string(),
                    "-singlefile",
                    "-png",
                    output.to_str().unwrap(),
                    prefix.to_str().unwrap(),
                ]);
                assert!(identify(&prefix.with_extension("png"), "%[pixel:p{0,0}]").contains(color));
            }
        } else {
            has_pages(&files.join("a (converted).pdf"), 1);
        }
    }
}
pub fn office(root: &Path, w: &mut Wire) {
    let files = root.join("files");
    let document = files.join("report.docx");
    std::fs::write(&document, b"unchanged office fixture").unwrap();
    let ready = pdf(&files, "cached.pdf");
    let cached = root.join("cache/flea/office");
    std::fs::create_dir_all(&cached).unwrap();
    let cached = cached.join(format!(
        "{}.pdf",
        crate::backend::officepdf::Store::key(&document)
    ));
    let bytes = std::fs::read(ready).unwrap();
    let file = std::fs::File::create(&cached).unwrap();
    (&file).write_all(&bytes).unwrap();
    file.set_modified(document.metadata().unwrap().modified().unwrap())
        .unwrap();
    let mut m = open(&files, w, &["report.docx"]);
    submit(&mut m, w);
    assert!(m.error.is_empty(), "{}", m.error);
    let output = files.join("report (converted).pdf");
    assert_eq!(std::fs::read(&output).unwrap(), bytes);
    has_pages(&output, 3);
    assert_eq!(
        std::fs::read(document).unwrap(),
        b"unchanged office fixture"
    );
}
pub fn safe(root: &Path, w: &mut Wire) {
    let files = root.join("files");
    let source = fixture(&files, "photo.png", "red");
    let original = std::fs::read(&source).unwrap();
    let collision = files.join("photo (converted).jpg");
    std::fs::write(&collision, b"keep collision").unwrap();
    let mut m = open(&files, w, &["photo.png"]);
    submit(&mut m, w);
    assert!(m.error.is_empty(), "{}", m.error);
    assert_eq!(std::fs::read(collision).unwrap(), b"keep collision");
    let landed = std::fs::read(files.join("photo (converted) copy.jpg")).unwrap();
    assert!(
        landed.starts_with(&[0xff, 0xd8]),
        "a taken output lands as a JPEG under the copy name"
    );
    controlled_converter(root);
    let mut m = open(&files, w, &["photo.png"]);
    press(&mut m, w, Key::named("Down", ""));
    press(&mut m, w, Key::named("Return", ""));
    // The shell creates held before printf writes the pid, so wait for the whole line.
    until(&mut m, w, |_| {
        std::fs::read_to_string(root.join("held")).is_ok_and(|pid| pid.ends_with('\n'))
    });
    let pid: u32 = std::fs::read_to_string(root.join("held"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(pid > 1 && Path::new(&format!("/proc/{pid}")).exists());
    press(&mut m, w, Key::named("Escape", ""));
    until(&mut m, w, |m| m.converting.run.is_none());
    assert!(
        !Path::new(&format!("/proc/{pid}")).exists(),
        "cancel must reap the held tool"
    );
    assert!(m
        .message
        .contains("Conversion cancelled; 0 outputs created."));
    assert!(!files.join("photo (stripped).png").exists());
    std::fs::write(root.join("refuse"), b"owned refusal").unwrap();
    let mut m = open(&files, w, &["photo.png"]);
    press(&mut m, w, Key::named("Down", ""));
    submit(&mut m, w);
    assert!(m.error.contains("fixture converter refusal"));
    assert!(!files.join("photo (stripped).png").exists());
    assert_eq!(std::fs::read(source).unwrap(), original);
}
fn controlled_converter(root: &Path) {
    use std::os::unix::fs::PermissionsExt;
    assert!(root.is_absolute() && root.join(".flea-test-sandbox").is_file());
    let bwrap = root.join("bin/bwrap");
    let script = r#"#!/bin/sh
set -eu
while test "$#" -gt 0; do
 case "$1" in
  --ro-bind|--ro-bind-try|--ro-bind-fd|--bind|--symlink|--setenv) shift 3 ;;
  --tmpfs|--proc|--dev|--json-status-fd) shift 2 ;;
  --*) shift ;;
  *) exec "$@" ;;
 esac
done
exit 99
"#;
    std::fs::write(&bwrap, script).unwrap();
    std::fs::set_permissions(&bwrap, std::fs::Permissions::from_mode(0o755)).unwrap();
    let magick = root.join("bin/magick");
    let script = r#"#!/bin/sh
set -eu
root=${FLEA_TUI_CONVERT_ROOT:?}
case "$root" in /*) test -f "$root/.flea-test-sandbox" ;; *) exit 98 ;; esac
if test -f "$root/refuse"; then printf 'fixture converter refusal\n' >&2; exit 1; fi
printf '%s\n' "$$" > "$root/held"
exec /bin/sleep 300
"#;
    std::fs::write(&magick, script).unwrap();
    std::fs::set_permissions(&magick, std::fs::Permissions::from_mode(0o755)).unwrap();
}
