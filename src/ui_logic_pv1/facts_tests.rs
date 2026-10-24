// Port of tests/js/facts.js: each value check, labelled with the JS label text in the board's order.
use super::facts::*;
use super::kinds::{self, State};
use super::meta::{Entry, Meta};
use super::photo::{self, Photo, PhotoFacts};
use crate::i18n::I18n;
const MODE: u32 = 0o100644;
// 2026-08-31 12:00 UTC less a day, the JS fixed instant, so no check reads the clock.
const MTIME: i64 = 1_788_091_200;
fn row(icon: &str, size: u64, mode: u32) -> Row { Row { name: "x".into(), is_dir: false, size, mtime: MTIME, mode, icon: icon.into() } }
fn named(name: &str, icon: &str) -> Row { Row { name: name.into(), ..row(icon, 10, MODE) } }
fn kind_of(name: &str, icon: &str) -> State { state(Some(&named(name, icon)), 1, false, "", "") }
fn labels(list: &[Fact]) -> String { list.iter().map(|f| f.label.as_str()).collect::<Vec<_>>().join("|") }
fn value(list: &[Fact], label: &str) -> String {
    list.iter().find(|f| f.label == label).map_or("(no such row)".to_string(), |f| f.value.clone())
}
fn fx(i18n: &I18n, st: State, r: &Row, m: Option<&Meta>, kind: &str, extra: Extra) -> Vec<Fact> { facts(st, r, m, kind, &extra, i18n) }
fn with_lines(n: u64) -> Meta { Meta { lines: Some(n), ..Default::default() } }
fn with_entries(n: u64) -> Meta { Meta { entries: Some(n), ..Default::default() } }
fn ent(name: &str, is_dir: bool) -> Entry { Entry { name: name.into(), is_dir } }
fn pages(p: &str) -> Extra { Extra { pages: p.into(), ..Default::default() } }
fn canon() -> Photo {
    Photo { make: "Canon".into(), model: "Canon EOS R6".into(), lens: "RF35mm F1.8 MACRO IS STM".into(), exp: Some((1.0, 250.0)), fnum: Some((28.0, 10.0)),
        iso: 400, focal: Some((35.0, 1.0)), taken: "2026:09:12 18:31:00".into(), ..Default::default() }
}
fn big() -> Meta {
    Meta { entries: Some(214), unpacked: 3_400_000_000, names: vec![ent("daemon", true), ent("ui", true), ent("Cargo.toml", false)], ..Default::default() }
}
fn many() -> Vec<Option<Row>> {
    vec![Some(row("image-x-generic", 1000, MODE)), Some(row("image-x-generic", 2000, MODE)), Some(row("video-x-generic", 4000, MODE)), Some(row("text-x-generic", 8000, MODE))]
}
// The tall line as the JS test joins it: Kind, then "|", then the tail.
fn compact(i18n: &I18n, st: State, size: u64, m: Option<&Meta>, kind: &str, extra: Extra) -> String {
    let (k, t) = compact_line(st, Some(&row("text-x-generic", size, MODE)), m, kind, &extra, i18n); format!("{k}|{t}")
}
fn cam(i18n: &I18n, make: &str, model: &str) -> String {
    photo::photo_facts(Some(&Photo { make: make.into(), model: model.into(), ..Default::default() }), i18n).camera
}
fn exposure_of(i18n: &I18n, p: Photo) -> String { photo::photo_facts(Some(&p), i18n).exposure }
fn focal_of(i18n: &I18n, p: Photo) -> String { photo::photo_facts(Some(&p), i18n).focal }
#[test] fn icon_and_extension_routing() {
    // Kinds.kindState, one icon per family
    assert_eq!(kinds::kind_state("image-x-generic"), State::Image);
    assert_eq!(kinds::kind_state("video-x-generic"), State::Video);
    assert_eq!(kinds::kind_state("audio-x-generic"), State::Audio);
    assert_eq!(kinds::kind_state("text-x-script"), State::Code);
    assert_eq!(kinds::kind_state("text-x-generic"), State::Text);
    assert_eq!(kinds::kind_state("package-x-generic"), State::Archive);
    assert_eq!(kinds::kind_state("application-x-executable"), State::Unsupported);
    // the office family is unsupported, because none of it is text
    for icon in ["x-office-document", "x-office-spreadsheet", "x-office-presentation"] { assert_eq!(kinds::kind_state(icon), State::Unsupported); }
    // a word processor document is not text; a pdf sharing that icon still is a pdf
    assert_eq!(kind_of("report.odt", "x-office-document"), State::Unsupported);
    assert_eq!(kind_of("manual.pdf", "x-office-document"), State::Pdf);
    // CSV and TSV reach text despite their spreadsheet icon
    assert_eq!(kind_of("table.csv", "x-office-spreadsheet"), State::Text);
    assert_eq!(kinds::quick_look_kind("x-office-spreadsheet", "table.csv", false), State::Text);
    assert_eq!(kind_of("table.TSV", "x-office-spreadsheet"), State::Text);
    assert_eq!(kinds::quick_look_kind("x-office-spreadsheet", "table.TSV", false), State::Text);
    // binary spreadsheets and names containing csv remain unsupported
    for name in ["table.xlsx", "table.ods", "table.csv.bak"] {
        assert_eq!(kind_of(name, "x-office-spreadsheet"), State::Unsupported);
        assert_eq!(kinds::quick_look_kind("x-office-spreadsheet", name, false), State::Unsupported);
    }
    // markdown reaches the text frame, every suffix; a name merely containing md is not markdown
    assert_eq!(kind_of("field-bench-notes.md", "x-office-document"), State::Text);
    assert_eq!(kind_of("a.markdown", "x-office-document"), State::Text);
    assert_eq!(kind_of("b.mkd", "x-office-document"), State::Text);
    assert_eq!(kind_of("amd.doc", "x-office-document"), State::Unsupported);
    // a package, a disk image and an appimage draw the archive tile by extension, Quick Look too
    assert_eq!(kind_of("driver.deb", "application-x-generic"), State::Archive);
    assert_eq!(kind_of("debian-13.iso", "application-x-generic"), State::Archive);
    assert_eq!(kind_of("run.AppImage", "application-x-executable"), State::Archive);
    assert_eq!(kinds::quick_look_kind("application-x-generic", "driver.deb", false), State::Archive);
    assert_eq!(kinds::quick_look_kind("application-x-generic", "debian-13.iso", false), State::Archive);
}
#[test] fn state_precedence_and_code() {
    // a symlink is a symlink whatever its name; a multi-selection outranks the row
    assert_eq!(state(Some(&row("image-x-generic", 10, 0o120777)), 0, false, "", ""), State::Symlink);
    assert_eq!(state(Some(&row("image-x-generic", 10, MODE)), 4, false, "", ""), State::Multi);
    // an error outranks a kind; a preview still arriving is loading
    assert_eq!(state(Some(&row("image-x-generic", 10, MODE)), 1, false, "Permission denied", ""), State::Error);
    assert_eq!(state(Some(&row("video-x-generic", 10, MODE)), 1, true, "", ""), State::Loading);
    // a source file is code whatever icon the backend gave it; prose is not
    for name in ["icons.rs", "app.py", "main.go", "run.sh", "conf.toml"] {
        assert_eq!(state(Some(&named(name, "text-x-generic")), 1, false, "", "Rust source code"), State::Code);
    }
    assert_eq!(state(Some(&named("notes.md", "text-x-generic")), 1, false, "", "Markdown"), State::Text);
    // an extension alone never promotes a row the icon did not call text
    assert_eq!(state(Some(&named("sprite.css", "image-x-generic")), 1, false, "", "PNG image"), State::Image);
    // the extension table matches only a real suffix
    assert!(kinds::is_code("a.rs") && !kinds::is_code("rs") && !kinds::is_code("a.rsx") && !kinds::is_code("README"));
    // a type nothing recognised is unsupported, not text; a real text type stays text
    assert_eq!(state(Some(&row("text-x-generic", 212_000_000, MODE)), 1, false, "", "Data"), State::Unsupported);
    assert_eq!(state(Some(&row("text-x-generic", 18_000, MODE)), 1, false, "", "Plain text document"), State::Text);
    // a caller that names no kind still classifies by icon; no row at all is unsupported
    assert_eq!(state(Some(&row("text-x-generic", 18_000, MODE)), 1, false, "", ""), State::Text);
    assert_eq!(state(None, 0, false, "", ""), State::Unsupported);
    // a directory is not a preview subject
    let dir = Row { is_dir: true, ..row("folder", 0, 0o040755) };
    assert!(!is_previewable(Some(&dir)) && is_previewable(Some(&row("image-x-generic", 1, MODE))) && !is_previewable(None));
}
#[test] fn quick_look_kinds_and_models() {
    // the Quick Look draws an image, an archive and a symlink to an image by the icon sent
    assert_eq!(kinds::quick_look_kind("image-x-generic", "photo.png", false), State::Image);
    assert_eq!(kinds::quick_look_kind("package-x-generic", "backup.tar.zst", false), State::Archive);
    assert_eq!(kinds::quick_look_kind("image-x-generic", "link-to-photo.png", false), State::Image);
    // it keeps its media, pdf, markdown and text answers
    assert_eq!(kinds::quick_look_kind("audio-x-generic", "tone.wav", false), State::Audio);
    assert_eq!(kinds::quick_look_kind("video-x-generic", "clip.mp4", false), State::Video);
    assert_eq!(kinds::quick_look_kind("x-office-document", "manual.pdf", false), State::Pdf);
    assert_eq!(kinds::quick_look_kind("x-office-document", "notes.md", false), State::Text);
    assert_eq!(kinds::quick_look_kind("text-x-generic", "notes.txt", false), State::Text);
    // code renders as text; an office file or an executable is declined
    assert_eq!(kinds::quick_look_kind("text-x-script", "script.sh", false), State::Text);
    assert_eq!(kinds::quick_look_kind("x-office-document", "report.doc", false), State::Unsupported);
    assert_eq!(kinds::quick_look_kind("application-x-executable", "a.out", false), State::Unsupported);
    // an icon classifies by its family, not by a list of the names seen so far
    assert_eq!(kinds::kind_state("text-x-generic-template"), State::Text);
    assert_eq!(kinds::kind_state("image-x-generic"), State::Image);
    // a 3D model is an image when a thumbnailer serves it; unserved (or JS undefined, read false) it is unsupported
    assert_eq!(kinds::quick_look_kind("image-x-generic", "part.STL", true), State::Image);
    assert_eq!(kinds::quick_look_kind("image-x-generic", "part.STL", false), State::Unsupported);
    assert_eq!(kinds::quick_look_kind("image-x-generic", "photo.png", false), State::Image);
    // only a real model suffix names a model
    assert!(kinds::is_model("part.STL") && kinds::is_model("a.3mf") && kinds::is_model("scene.glb"));
    assert!(!kinds::is_model("photo.png") && !kinds::is_model("stl") && !kinds::is_model("a.stlx"));
}
#[test] fn labels_per_state() {
    let i18n = super::english();
    let icon = row("image-x-generic", 2_100_000, MODE);
    let png = Meta { w: 2560, h: 1440, ..Default::default() };
    // image states Kind, Size, Modified, Pixels
    let png_list = fx(&i18n, State::Image, &icon, Some(&png), "PNG image", Extra::default());
    assert_eq!(labels(&png_list), "Kind|Size|Modified|Pixels");
    // an animated gif adds Frames after Pixels; a one-frame gif and a png draw no Frames row
    let gif = Meta { w: 480, h: 270, frames: 24, ..Default::default() };
    let gif_row = row("image-x-generic", 1_200_000, MODE);
    assert_eq!(labels(&fx(&i18n, State::Image, &gif_row, Some(&gif), "GIF image", Extra::default())), "Kind|Size|Modified|Pixels|Frames");
    let still = Meta { w: 480, h: 270, frames: 1, ..Default::default() };
    let still_list = fx(&i18n, State::Image, &gif_row, Some(&still), "GIF image", Extra::default());
    assert_eq!(format!("{}|{}", labels(&still_list), labels(&png_list)), "Kind|Size|Modified|Pixels|Kind|Size|Modified|Pixels");
    // video states Kind, Size, Modified, Duration, Pixels
    let hd = Meta { w: 1920, h: 1080, ..Default::default() };
    let vid_row = row("video-x-generic", 48_000_000, MODE);
    let vid = Extra { duration: "1:12".into(), ..Default::default() };
    assert_eq!(labels(&fx(&i18n, State::Video, &vid_row, Some(&hd), "MP4 video", vid)), "Kind|Size|Modified|Duration|Pixels");
    // audio states Kind, Size, Modified, Duration, Rate
    let aud = Extra { duration: "4:05".into(), rate: "44.1 kHz".into(), ..Default::default() };
    assert_eq!(labels(&fx(&i18n, State::Audio, &row("audio-x-generic", 31_000_000, MODE), None, "FLAC audio", aud)), "Kind|Size|Modified|Duration|Rate");
    // pdf states Kind, Size, Modified, Pages
    assert_eq!(labels(&fx(&i18n, State::Pdf, &row("application-pdf", 2_300_000, MODE), None, "PDF document", pages("51"))), "Kind|Size|Modified|Pages");
    // text states Kind, Size, Modified, Lines; code adds Mode
    let text = fx(&i18n, State::Text, &row("text-x-generic", 18_000, MODE), Some(&with_lines(214)), "Markdown", Extra::default());
    assert_eq!(labels(&text), "Kind|Size|Modified|Lines");
    let code = fx(&i18n, State::Code, &row("text-x-script", 4200, MODE), Some(&with_lines(132)), "Rust source", Extra::default());
    assert_eq!(labels(&code), "Kind|Size|Modified|Lines|Mode");
    // archive states Kind, Size, Modified, Entries
    let arc = row("package-x-generic", 1_200_000_000, MODE);
    let arc_extra = Extra { entries: "214".into(), unpacked: "3.4 GB".into(), ..Default::default() };
    assert_eq!(labels(&fx(&i18n, State::Archive, &arc, None, "Zstandard tar", arc_extra)), "Kind|Size|Modified|Entries");
    // symlink states Kind, Target, Points at, Mode
    let folder = Meta { target: "/usr/share/omarchy".into(), target_dir: true, ..Default::default() };
    assert_eq!(labels(&fx(&i18n, State::Symlink, &row("folder", 18, 0o120777), Some(&folder), "Folder", Extra::default())), "Kind|Target|Points at|Mode");
    // error states Kind, Size, Mode, Owner; loading states Kind, Size, State
    let owner = Extra { owner: "root".into(), ..Default::default() };
    assert_eq!(labels(&fx(&i18n, State::Error, &row("application-x-generic", 419, 0o100600), None, "Unknown", owner)), "Kind|Size|Mode|Owner");
    assert_eq!(labels(&fx(&i18n, State::Loading, &vid_row, None, "MP4 video", Extra::default())), "Kind|Size|State");
    // unsupported states Kind, Size, Modified, Mode
    assert_eq!(labels(&fx(&i18n, State::Unsupported, &row("application-x-generic", 212_000_000, MODE), None, "Data", Extra::default())), "Kind|Size|Modified|Mode");
    // HANDOFF rule 19: a label with no value is not drawn, so the row goes
    let failed = Meta { lines: Some(0), lines_failed: true, ..Default::default() };
    assert_eq!(labels(&fx(&i18n, State::Text, &row("text-x-generic", 18_000, MODE), Some(&failed), "Markdown", Extra::default())), "Kind|Size|Modified");
    // a raw image with unreadable dimensions and an unlisted archive draw no Pixels or Entries row
    assert_eq!(labels(&fx(&i18n, State::Image, &icon, None, "Raw image", Extra::default())), "Kind|Size|Modified");
    assert_eq!(labels(&fx(&i18n, State::Archive, &arc, None, "Zip archive", Extra::default())), "Kind|Size|Modified");
    // a video nothing has probed yet drops Duration and keeps the rest
    let probe = Meta { w: 1920, h: 1080, ..Default::default() };
    assert_eq!(labels(&fx(&i18n, State::Video, &vid_row, Some(&probe), "MP4 video", Extra::default())), "Kind|Size|Modified|Pixels");
}
#[test] fn values_per_state() {
    let i18n = super::english();
    let icon = row("image-x-generic", 2_100_000, MODE);
    // its pixels read as the canvas writes them
    let png = Meta { w: 2560, h: 1440, ..Default::default() };
    assert_eq!(value(&fx(&i18n, State::Image, &icon, Some(&png), "PNG image", Extra::default()), "Pixels"), "2560 \u{d7} 1440");
    // an animated gif's frames read as a bare count
    let gif = Meta { w: 480, h: 270, frames: 24, ..Default::default() };
    assert_eq!(value(&fx(&i18n, State::Image, &icon, Some(&gif), "GIF image", Extra::default()), "Frames"), "24");
    // a video reads its duration off the meta answer; an audio row reads its rate the way the canvas writes it
    let hd = Meta { w: 1920, h: 1080, duration_ms: 72000, ..Default::default() };
    assert_eq!(value(&fx(&i18n, State::Video, &icon, Some(&hd), "MP4 video", Extra::default()), "Duration"), "1:12");
    let flac = Meta { duration_ms: 245000, sample_rate: 44100.0, ..Default::default() };
    assert_eq!(value(&fx(&i18n, State::Audio, &icon, Some(&flac), "FLAC audio", Extra::default()), "Rate"), "44.1 kHz");
    // an archive states its entries and what they weigh unpacked; a count alone stays a count
    let arc = row("package-x-generic", 1_200_000_000, MODE);
    let both = Extra { entries: "214".into(), unpacked: "3.4 GB".into(), ..Default::default() };
    assert_eq!(value(&fx(&i18n, State::Archive, &arc, None, "Zstandard tar", both), "Entries"), "214, 3.4 GB out");
    let count_only = Extra { entries: "214".into(), ..Default::default() };
    assert_eq!(value(&fx(&i18n, State::Archive, &arc, None, "Zstandard tar", count_only), "Entries"), "214");
    // a long archive states an exact count and total; a four-figure one groups
    assert_eq!(value(&fx(&i18n, State::Archive, &arc, Some(&big()), "Zstandard tar", Extra::default()), "Entries"), "214, 3.4 GB out");
    let four = Meta { entries: Some(1204), unpacked: 3_400_000_000, ..Default::default() };
    assert_eq!(value(&fx(&i18n, State::Archive, &arc, Some(&four), "Zstandard tar", Extra::default()), "Entries"), "1,204, 3.4 GB out");
    // symlink says what it points at, a folder or a file
    let folder = Meta { target: "/usr/share/omarchy".into(), target_dir: true, ..Default::default() };
    assert_eq!(value(&fx(&i18n, State::Symlink, &icon, Some(&folder), "Folder", Extra::default()), "Points at"), "Folder");
    let file = Meta { target: "a.txt".into(), target_dir: false, ..Default::default() };
    assert_eq!(value(&fx(&i18n, State::Symlink, &icon, Some(&file), "x", Extra::default()), "Points at"), "File");
}
#[test] fn media_and_archive_values() {
    let i18n = super::english();
    // a whole number of kilohertz drops the decimal; nothing probed is empty; no meta is not a crash
    assert_eq!(media_extra(Some(&Meta { sample_rate: 48000.0, ..Default::default() }), &i18n).1, "48 kHz");
    let unprobed = Meta { duration_ms: 0, sample_rate: 0.0, ..Default::default() };
    assert_eq!(media_extra(Some(&unprobed), &i18n), (String::new(), String::new()));
    assert_eq!(media_extra(None, &i18n), (String::new(), String::new()));
    // an archive nothing has measured yet shows empty cells rather than zeroes
    let zero = Meta { entries: Some(0), unpacked: 0, ..Default::default() };
    assert_eq!(archive_extra(Some(&zero), &i18n), (String::new(), String::new()));
    // Quick Look's archive line: count and unpacked size, nothing before the index lands
    let index = Meta { entries: Some(214), unpacked: 3_400_000_000, ..Default::default() };
    let one = Meta { entries: Some(1), unpacked: 0, ..Default::default() };
    let failed = Meta { entries: Some(0), archive_failed: true, ..Default::default() };
    let clean_empty = Meta { entries: Some(0), archive_failed: false, ..Default::default() };
    let line = [archive_line(Some(&index), &i18n), archive_line(Some(&one), &i18n), archive_line(None, &i18n), archive_line(Some(&clean_empty), &i18n), archive_line(Some(&failed), &i18n)];
    assert_eq!(line.join("|"), "214 entries \u{b7} 3.4 GB unpacked|1 entry||0 entries|");
    assert_eq!(archive_line(Some(&with_entries(1204)), &i18n), "1,204 entries");
    // the tile lists the names the wire carried, and states how many it could not list
    let list = big();
    let names: Vec<&str> = archive_entries(Some(&list), None).iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names.join("|"), "daemon|ui|Cargo.toml");
    assert_eq!(archive_more(Some(&list), None), 211);
    // an archive whose names all fit says no more; one nothing has listed lists nothing
    let fits = Meta { entries: Some(3), unpacked: 10, names: vec![ent("a", false), ent("b", false), ent("c", false)], ..Default::default() };
    assert_eq!(archive_more(Some(&fits), None), 0);
    assert_eq!(archive_entries(None, None).len(), 0);
    assert_eq!(archive_more(None, None), 0);
    // the frame's room bounds the list a second time: two rows list two, none lists none
    let two: Vec<&str> = archive_entries(Some(&list), Some(2)).iter().map(|e| e.name.as_str()).collect();
    assert_eq!(two.join("|"), "daemon|ui");
    assert_eq!(archive_more(Some(&list), Some(2)), 212);
    assert_eq!(archive_entries(Some(&list), Some(0)).len(), 0);
    assert_eq!(archive_more(Some(&list), Some(0)), 214);
}
#[test] fn line_count_and_pixels() {
    let i18n = super::english();
    // a complete line count is a plain number and groups; a truncated one is a floor
    assert_eq!(line_count(Some(&with_lines(214)), &i18n), "214");
    assert_eq!(line_count(Some(&with_lines(1204)), &i18n), "1,204");
    assert_eq!(line_count(Some(&Meta { lines: Some(9000), partial: true, ..Default::default() }), &i18n), "> 9,000");
    // no meta is an empty cell; a file that could not be opened states no count; an empty file states zero
    assert_eq!(line_count(None, &i18n), "");
    assert_eq!(line_count(Some(&Meta { lines: Some(0), lines_failed: true, ..Default::default() }), &i18n), "");
    assert_eq!(line_count(Some(&with_lines(0)), &i18n), "0");
    // pixels with nothing behind them are empty rather than 0 x 0
    assert_eq!(pixels(None, &i18n), "");
    assert_eq!(pixels(Some(&Meta::default()), &i18n), "");
}
#[test] fn multi_summary() {
    let i18n = super::english();
    let rows = many();
    let all = multi_facts(&rows, None, &i18n);
    // multi-select states Kinds, Combined, Newest, Oldest; the kinds read the way the canvas phrases them
    assert_eq!(labels(&all), "Kinds|Combined|Newest|Oldest");
    assert_eq!(value(&all, "Kinds"), "2 images, 1 video, 1 text");
    // a four-figure kind groups
    let grouped = [Group { word: Word::Image, icon: "image-x-generic".into(), count: 1204 }];
    assert_eq!(kind_summary(&grouped, false, &i18n), "1,204 images");
    // the combined size is every selected row added up
    assert_eq!(value(&all, "Combined"), "15.0 kB");
    // an empty selection states only the row it can fill; a hole is skipped rather than counted
    assert_eq!(labels(&multi_facts(&[], None, &i18n)), "Combined");
    let holed = vec![None, Some(row("image-x-generic", 1000, MODE))];
    assert_eq!(value(&multi_facts(&holed, None, &i18n), "Kinds"), "1 image");
    // a selection wider than the held window states floors, not an undercount
    assert_eq!(value(&multi_facts(&rows, Some(9), &i18n), "Combined"), "> 15.0 kB");
    assert_eq!(value(&multi_facts(&rows, Some(9), &i18n), "Kinds"), "> 2 images, > 1 video, > 1 text");
    // a selection inside the window states plain totals; a hole inside it is a floor too
    assert_eq!(value(&multi_facts(&rows, Some(4), &i18n), "Combined"), "15.0 kB");
    assert_eq!(value(&multi_facts(&rows, Some(4), &i18n), "Kinds"), "2 images, 1 video, 1 text");
    assert_eq!(value(&multi_facts(&holed, Some(2), &i18n), "Combined"), "> 1.0 kB");
}
#[test] fn compact_lines() {
    let i18n = super::english();
    let none = Extra::default();
    // code states its Kind, size and lines; a Markdown document reads the same shape
    assert_eq!(compact(&i18n, State::Code, 4100, Some(&with_lines(132)), "Rust source code", none.clone()), "Rust source code| \u{b7} 4.1 kB \u{b7} 132 lines");
    assert_eq!(compact(&i18n, State::Text, 6200, Some(&with_lines(48)), "Markdown document", none.clone()), "Markdown document| \u{b7} 6.2 kB \u{b7} 48 lines");
    // one line is a singular and none is a plural
    assert_eq!(compact(&i18n, State::Text, 3, Some(&with_lines(1)), "Plain text", none.clone()), "Plain text| \u{b7} 3 B \u{b7} 1 line");
    assert_eq!(compact(&i18n, State::Text, 0, Some(&with_lines(0)), "Plain text", none.clone()), "Plain text| \u{b7} 0 B \u{b7} 0 lines");
    // a count stopped early keeps its floor, and a four-figure count its group
    let floor = Meta { lines: Some(5000), partial: true, ..Default::default() };
    assert_eq!(compact(&i18n, State::Code, 4100, Some(&floor), "Rust source code", none.clone()), "Rust source code| \u{b7} 4.1 kB \u{b7} > 5,000 lines");
    assert_eq!(compact(&i18n, State::Text, 4100, Some(&with_lines(1204)), "Plain text", none.clone()), "Plain text| \u{b7} 4.1 kB \u{b7} 1,204 lines");
    // a count not there yet, or unreadable, leaves the line at Kind and size
    let unreadable = Meta { lines: Some(0), lines_failed: true, ..Default::default() };
    assert_eq!(compact(&i18n, State::Code, 4100, None, "Rust source code", none.clone()), "Rust source code| \u{b7} 4.1 kB");
    assert_eq!(compact(&i18n, State::Code, 4100, Some(&unreadable), "Rust source code", none.clone()), "Rust source code| \u{b7} 4.1 kB");
    // an archive states its entries: singular, exact zero, and nothing for a failed index
    let failed_index = Meta { entries: Some(0), archive_failed: true, ..Default::default() };
    assert_eq!(compact(&i18n, State::Archive, 9000, Some(&with_entries(214)), "Zip archive", none.clone()), "Zip archive| \u{b7} 9.0 kB \u{b7} 214 entries");
    assert_eq!(compact(&i18n, State::Archive, 9000, Some(&with_entries(1)), "Zip archive", none.clone()), "Zip archive| \u{b7} 9.0 kB \u{b7} 1 entry");
    assert_eq!(compact(&i18n, State::Archive, 9000, Some(&with_entries(0)), "Zip archive", none.clone()), "Zip archive| \u{b7} 9.0 kB \u{b7} 0 entries");
    assert_eq!(compact(&i18n, State::Archive, 9000, Some(&failed_index), "Zip archive", none.clone()), "Zip archive| \u{b7} 9.0 kB");
    // a PDF states its pages with a singular and a grouped count, and nothing until read
    assert_eq!(compact(&i18n, State::Pdf, 2_400_000, None, "PDF document", pages("12")), "PDF document| \u{b7} 2.4 MB \u{b7} 12 pages");
    assert_eq!(compact(&i18n, State::Pdf, 31_000, None, "PDF document", pages("1")), "PDF document| \u{b7} 31.0 kB \u{b7} 1 page");
    assert_eq!(compact(&i18n, State::Pdf, 9_000_000, None, "PDF document", pages("1204")), "PDF document| \u{b7} 9.0 MB \u{b7} 1,204 pages");
    assert_eq!(compact(&i18n, State::Pdf, 31_000, None, "PDF document", pages("")), "PDF document| \u{b7} 31.0 kB");
    // a Kind that looks like a placeholder is kept whole; a missing Kind drops the separator
    assert_eq!(compact(&i18n, State::Code, 4100, Some(&with_lines(2)), "100% %2 source", none.clone()), "100% %2 source| \u{b7} 4.1 kB \u{b7} 2 lines");
    assert_eq!(compact(&i18n, State::Code, 4100, Some(&with_lines(2)), "", none.clone()), "|4.1 kB \u{b7} 2 lines");
    // a missing row states nothing
    assert_eq!(compact_line(State::Code, None, None, "Rust source code", &none, &i18n), (String::new(), String::new()));
    // TallPreviews: text, code, an archive list and a PDF are tall; every other kind is not
    let kinds_tall = [(State::Text, true), (State::Code, true), (State::Archive, true), (State::Image, false), (State::Video, false), (State::Audio, false),
        (State::Pdf, true), (State::Symlink, false), (State::Multi, false), (State::Loading, false), (State::Error, false), (State::Unsupported, false)];
    for (st, want) in kinds_tall { assert_eq!(tall(st), want); }
    // the tall frame is the column less its margins, the name, the facts and the two gaps
    assert_eq!(tall_height(700.0, 12.0, 20.0, 18.0, 9.0), 620.0);
    assert_eq!(tall_height(40.0, 12.0, 20.0, 18.0, 9.0), 0.0);
}
#[test] fn photo_facts_values() {
    let i18n = super::english();
    let shot = photo::photo_facts(Some(&canon()), &i18n);
    // a photo's facts read as the board writes them
    assert_eq!([shot.camera, shot.lens, shot.exposure, shot.focal, shot.taken].join("|"), "Canon EOS R6|RF35mm F1.8 MACRO IS STM|1/250 s \u{b7} f/2.8 \u{b7} ISO 400|35 mm|2026-09-12 18:31");
    // a model starting with the make's first word stands alone; otherwise it is joined to the make
    assert_eq!(format!("{}|{}", cam(&i18n, "NIKON CORPORATION", "Nikon Z 6"), cam(&i18n, "OLYMPUS IMAGING CORP.", "OLYMPUS E-M1")), "Nikon Z 6|OLYMPUS E-M1");
    assert_eq!(format!("{}|{}", cam(&i18n, "SONY", "ILCE-7M3"), cam(&i18n, "FUJIFILM", "X-T4")), "SONY ILCE-7M3|FUJIFILM X-T4");
    // a make or a model alone is the camera, and neither is no camera
    assert_eq!(format!("{}|{}|{}", cam(&i18n, "Canon", ""), cam(&i18n, "", "X100V"), cam(&i18n, "", "")), "Canon|X100V|");
    // under 0.3 s a fraction, from 0.3 s a number of seconds
    let shutters = [(1.0, 8000.0), (10.0, 2500.0), (1.0, 4.0), (3.0, 10.0), (1.0, 3.0), (1.0, 2.0), (6.0, 10.0), (8.0, 10.0), (1.0, 1.0), (2.0, 1.0), (3.0, 2.0), (30.0, 1.0)];
    let got: Vec<String> = shutters.iter().map(|&(n, d)| exposure_of(&i18n, Photo { exp: Some((n, d)), ..Default::default() })).collect();
    assert_eq!(got.join("|"), "1/8000 s|1/250 s|1/4 s|0.3 s|0.3 s|0.5 s|0.6 s|0.8 s|1 s|2 s|1.5 s|30 s");
    // the aperture keeps one decimal and drops a trailing zero
    let stops = [(28.0, 10.0), (4.0, 1.0), (18.0, 10.0), (56.0, 10.0), (14.0, 10.0)];
    let fs: Vec<String> = stops.iter().map(|&(n, d)| exposure_of(&i18n, Photo { fnum: Some((n, d)), ..Default::default() })).collect();
    assert_eq!(fs.join("|"), "f/2.8|f/4|f/1.8|f/5.6|f/1.4");
    // an absent part of the exposure is left out, and none leaves no exposure
    assert_eq!(exposure_of(&i18n, Photo { iso: 800, ..Default::default() }), "ISO 800");
    assert_eq!(exposure_of(&i18n, Photo { fnum: Some((28.0, 10.0)), iso: 100, ..Default::default() }), "f/2.8 \u{b7} ISO 100");
    assert_eq!(exposure_of(&i18n, Photo { exp: Some((1.0, 60.0)), fnum: Some((4.0, 1.0)), ..Default::default() }), "1/60 s \u{b7} f/4");
    assert_eq!(exposure_of(&i18n, Photo { exp: Some((0.0, 1.0)), fnum: Some((1.0, 0.0)), iso: 0, ..Default::default() }), "");
    // the focal length rounds to a whole millimetre and zero is absent
    let focals = [(35.0, 1.0), (244.0, 10.0), (245.0, 10.0), (0.0, 1.0), (5.0, 0.0)];
    let fl: Vec<String> = focals.iter().map(|&f| focal_of(&i18n, Photo { focal: Some(f), ..Default::default() })).collect();
    assert_eq!(fl.join("|"), "35 mm|24 mm|25 mm||");
    // a taken stamp is written like Modified and a malformed one is absent
    let taken = [photo::taken("2026:01:02 03:04:05"), photo::taken("0000:00:00 00:00:00"), photo::taken("yesterday")];
    assert_eq!(taken.join("|"), "2026-01-02 03:04||");
    // nothing reads Unknown: a missing photo is a set of empty strings
    assert_eq!(photo::photo_facts(None, &i18n), PhotoFacts::default());
}
#[test] fn raw_image_rows() {
    let i18n = super::english();
    let raw = Meta { photo: Some(Photo { w: 6000, h: 4000, ..canon() }), ..Default::default() };
    let raw_row = row("image-x-generic", 31_200_000, MODE);
    let list = fx(&i18n, State::Image, &raw_row, Some(&raw), "Canon CR3 raw image", Extra::default());
    // a raw image's column rows follow the board's order, and carry the worded values
    assert_eq!(labels(&list), "Kind|Size|Modified|Camera|Lens|Exposure|Focal|Pixels|Taken");
    assert_eq!(value(&list, "Exposure"), "1/250 s \u{b7} f/2.8 \u{b7} ISO 400");
    assert_eq!(value(&list, "Pixels"), "6000 \u{d7} 4000");
    // no EXIF keeps today's rows; some facts draw only those, in order
    let bare_row = row("image-x-generic", 2_100_000, MODE);
    let bare = Meta { photo: None, ..Default::default() };
    assert_eq!(labels(&fx(&i18n, State::Image, &bare_row, Some(&bare), "Canon CR3 raw image", Extra::default())), "Kind|Size|Modified");
    let some = Meta { photo: Some(Photo { model: "X-T4".into(), make: "FUJIFILM".into(), taken: "2026:01:02 03:04:05".into(), ..Default::default() }), ..Default::default() };
    assert_eq!(labels(&fx(&i18n, State::Image, &bare_row, Some(&some), "Fujifilm RAF raw image", Extra::default())), "Kind|Size|Modified|Camera|Taken");
    // the Pixels row is the sensor size, and a frame header's size wins
    assert_eq!(pixels(Some(&raw), &i18n), "6000 \u{d7} 4000");
    assert_eq!(pixels(Some(&Meta { w: 64, h: 48, photo: raw.photo.clone(), ..Default::default() }), &i18n), "64 \u{d7} 48");
    assert_eq!(pixels(Some(&Meta { photo: Some(Photo { w: 6000, ..Default::default() }), ..Default::default() }), &i18n), "");
    assert_eq!(pixels(Some(&Meta { photo: None, ..Default::default() }), &i18n), "");
}
