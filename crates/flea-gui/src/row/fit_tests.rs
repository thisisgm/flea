//! I50-26: file names fit by their shaped glyphs and keep a tall script's marks on their letters; sizes and dates take the roster's digits.
use super::cache::{Extra, Shaping};
use super::format::{date_text, size_text, Words};
use crate::list::element::drawn_name;
use flea::i18n::{I18n, Paths};
use flea::protocol::RowFacts;
use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use unicode_segmentation::UnicodeSegmentation;

/// The shipped catalog for `language` under the fixture roster, loaded from a marked scratch root that is gone once the catalog is read.
pub(crate) fn catalog(language: &str) -> Arc<I18n> {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "flea-i50-fit-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&root).expect("a fresh scratch root");
    fs::write(root.join(".flea-test-sandbox"), "I50-26").expect("the marker");
    fs::create_dir_all(root.join("default/i18n")).unwrap();
    fs::write(
        root.join("default/i18n/locales.json"),
        include_str!("../../../../src/i18n/fixtures/locales.json"),
    )
    .unwrap();
    fs::write(root.join("language"), language).unwrap();
    let catalog = I18n::load(Paths {
        omarchy: root.clone(),
        language_file: root.join("language"),
        catalogs: vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../i18n")],
    });
    assert!(root.join(".flea-test-sandbox").is_file() && root.starts_with(std::env::temp_dir()));
    fs::remove_dir_all(&root).expect("the scratch root goes");
    Arc::new(catalog)
}

/// A monospace face whose digit is 8.4px, with CJK glyphs one em wide and any other glyph as wide as `other`.
fn width(other: f32) -> impl Fn(&str) -> f32 {
    move |text: &str| {
        text.chars()
            .map(|letter| match super::cells_of(letter) {
                2 => 14.,
                _ if letter.is_ascii() || letter == '\u{2026}' => 8.4,
                _ => other,
            })
            .sum()
    }
}

fn shaping(cells: usize) -> Shaping {
    Shaping {
        name_cells: cells,
        clip_cells: cells,
        advance: 8.4,
        ..Shaping::default()
    }
}

fn named(name: &str) -> RowFacts {
    RowFacts {
        name: name.into(),
        ..RowFacts::default()
    }
}

#[test]
fn i50_fit_cjk_glyph_width() {
    // 16 ideographs and 9 Latin cells: 41 cells by Format.js, 299.6px by their glyphs, inside a 40 cell (336px) name column.
    let name = "見積書と議事録と最終版の資料一式_2025.pdf";
    let cells = super::cell_count(name);
    assert!(
        cells > 40,
        "the cell recipe must elide it, so this proves something: {cells}"
    );
    assert_ne!(super::middle_elide(name, 40), name);
    let glyphs = width(8.4);
    let (drawn, _) = drawn_name(
        &named(name),
        Extra::default(),
        &shaping(40),
        "",
        Some(&glyphs),
    );
    assert_eq!(drawn, name, "its glyphs fit the column, so it draws whole");
    // A longer one is cut where its glyphs reach the column, not where its cells do.
    let long = format!("{name}{name}");
    let (drawn, _) = drawn_name(
        &named(&long),
        Extra::default(),
        &shaping(40),
        "",
        Some(&glyphs),
    );
    assert!(
        glyphs(&drawn) <= 336. + 0.01,
        "{drawn:?} runs past the column"
    );
    assert!(
        drawn.chars().count() > super::middle_elide(&long, 40).chars().count(),
        "{drawn:?} keeps more than the cells' guess"
    );
    assert_eq!(
        drawn,
        widest(&long, 336., &glyphs),
        "it stops short of the column"
    );
}

/// The middle elision at the largest budget whose glyphs still fit `room`, found by walking every budget.
fn widest(name: &str, room: f32, glyphs: &dyn Fn(&str) -> f32) -> String {
    (0..=super::cell_count(name))
        .rev()
        .find(|cells| glyphs(&super::middle_elide(name, *cells)) <= room + 0.01)
        .map(|cells| super::middle_elide(name, cells).into_owned())
        .unwrap()
}

#[test]
fn i50_fit_wide_glyphs_never_pass_the_column() {
    // A fallback script one and a half cells wide: the cells would draw 60px past a 20 cell column.
    let name = "\u{0985}\u{09AE}\u{09BE}\u{09B0}-\u{09AA}\u{09CD}\u{09B0}\u{09A4}\u{09BF}\u{09AC}\u{09C7}\u{09A6}\u{09A8}-\u{09B8}\u{09BE}\u{09B0}\u{09BE}\u{0982}\u{09B6}.txt";
    let glyphs = width(12.6);
    let (drawn, _) = drawn_name(
        &named(name),
        Extra::default(),
        &shaping(20),
        "",
        Some(&glyphs),
    );
    assert!(
        glyphs(&drawn) <= 20. * 8.4 + 0.01,
        "{drawn:?} runs past the column"
    );
    assert!(drawn.contains('\u{2026}'));
    // Fewer cells than the column's own count, and no fewer than the glyphs allow.
    assert_eq!(drawn, widest(name, 20. * 8.4, &glyphs));
}

#[test]
fn i50_fit_whole_name_wins_over_its_ellipsis() {
    // A name whose last two narrow letters are slimmer than the ellipsis: 39 cells overflow where all 40 fit.
    let fits = |cells: usize| cells <= 35 || cells == 40;
    assert_eq!(super::fit_budget(35, 40, fits), 40);
    assert_eq!(super::fit_budget(20, 40, |cells| cells <= 30), 30);
    assert_eq!(super::fit_budget(20, 40, |cells| cells <= 12), 12);
}

#[test]
fn i50_fit_marks_keep_format_cells() {
    // A cluster costs what Format.js charsOf() counts: a Latin accent rides free, a spacing sign keeps its cell.
    assert_eq!(super::unit_cells("e\u{301}"), 1);
    assert_eq!(super::cell_count("cafe\u{301}.txt"), 8);
    assert_eq!(super::cell_count("\u{0915}\u{093F}"), 2);
    assert_eq!(super::cell_count("\u{0E01}\u{0E34}"), 2);
    assert_eq!(super::cell_count("\u{8CC7}\u{6599}"), 4);
    // A search run that ends inside a Thai cluster lights the whole cluster.
    let name = "\u{0E01}\u{0E34}\u{0E19}\u{0E02}\u{0E49}\u{0E32}\u{0E27}-photo-archive-2025.png";
    let (drawn, (start, end)) = super::elide_around(name, 12, (0, 3));
    assert_eq!(&drawn[start..end], "\u{0E01}\u{0E34}");
    // One that starts on the vowel lights its letter too.
    let (drawn, (start, end)) = super::elide_around(name, 12, (3, 6));
    assert_eq!(&drawn[start..end], "\u{0E01}\u{0E34}");
    // An empty run lights nothing at either end.
    assert_eq!(super::elide_around("abc", 12, (0, 0)).1, (0, 0));
    assert_eq!(super::elide_around("abc", 12, (3, 3)).1, (3, 3));
}

#[test]
fn i50_fit_filename_elision_recipe() {
    // In the monospace face every name is cut exactly where Format.js cuts it: the old Latin geometry is unchanged.
    let glyphs = width(8.4);
    let long = "screenshot-2026-08-30-final-review-for-gm-after-the-bench-v3.png";
    for cells in [8, 24, 49, 70] {
        let (drawn, run) = drawn_name(
            &named(long),
            Extra::default(),
            &shaping(cells),
            "",
            Some(&glyphs),
        );
        assert_eq!(drawn, super::middle_elide(long, cells), "{cells} cells");
        assert_eq!(run, None);
    }
    let (drawn, _) = drawn_name(
        &named(long),
        Extra::default(),
        &shaping(49),
        "",
        Some(&glyphs),
    );
    assert_eq!(
        drawn,
        "screenshot-2026-08-30-fi\u{2026}m-after-the-bench-v3.png"
    );
    // A match run is still drawn whole inside the name's cells.
    let (drawn, run) = drawn_name(
        &named(long),
        Extra::default(),
        &shaping(24),
        "bench",
        Some(&glyphs),
    );
    let (start, end) = run.expect("the run is marked");
    assert_eq!(&drawn[start..end], "bench");
    assert_eq!(
        (drawn.clone(), run),
        drawn_name(&named(long), Extra::default(), &shaping(24), "bench", None)
    );
    // Columns and the TUI-shaped budget agree with no shaped width at all.
    assert_eq!(
        super::fitted(long, 30, 0., Some(&glyphs), |cells| (
            super::middle_elide(long, cells),
            ()
        ))
        .0,
        super::middle_elide(long, 30)
    );
}

#[test]
fn i50_fit_tall_row_ink() {
    // Thai, Devanagari and Tamil marks sit above or below their letter; a cut between them would orphan the mark.
    for name in [
        "\u{0E17}\u{0E35}\u{0E48}\u{0E2A}\u{0E38}\u{0E14}\u{0E02}\u{0E2D}\u{0E07}\u{0E1B}\u{0E35}\u{0E19}\u{0E35}\u{0E49}-\u{0E23}\u{0E39}\u{0E1B}\u{0E20}\u{0E32}\u{0E1E}\u{0E17}\u{0E35}\u{0E48}\u{0E2A}\u{0E38}\u{0E14}.png",
        "\u{0928}\u{093F}\u{0930}\u{094D}\u{0926}\u{0947}\u{0936}\u{093F}\u{0915}\u{093E}-\u{0915}\u{0940}-\u{092A}\u{094D}\u{0930}\u{0924}\u{093F}\u{0932}\u{093F}\u{092A}\u{093F}-\u{0905}\u{0902}\u{0924}\u{093F}\u{092E}.pdf",
        "\u{0B85}\u{0BB1}\u{0BBF}\u{0B95}\u{0BCD}\u{0B95}\u{0BC8}-\u{0B87}\u{0BB1}\u{0BC1}\u{0BA4}\u{0BBF}\u{0BAA}\u{0BCD}\u{0BAA}\u{0BA4}\u{0BBF}\u{0BAA}\u{0BCD}\u{0BAA}\u{0BC1}.txt",
    ] {
        let whole: Vec<&str> = name.graphemes(true).collect();
        for cells in 2..super::cell_count(name) {
            for drawn in [super::middle_elide(name, cells), super::right_elide(name, cells)] {
                for part in drawn.split('\u{2026}') {
                    let units: Vec<&str> = part.graphemes(true).collect();
                    assert!(
                        units.iter().all(|unit| whole.contains(unit)),
                        "{cells} cells cut a mark from its letter in {drawn:?}"
                    );
                }
            }
        }
        for per in 3..12 {
            let caption = crate::grid::captions::grid_caption(name, per as f64, 2.);
            for part in caption.split(['\u{2026}', '\n']) {
                assert!(
                    part.graphemes(true).all(|unit| whole.contains(&unit)),
                    "the grid caption cut a mark from its letter in {caption:?}"
                );
            }
        }
    }
}

#[test]
fn i50_fit_digits_decimal() {
    let file = RowFacts {
        name: "2025-06-15 report 22.1.txt".into(),
        size: 22_100,
        ..RowFacts::default()
    };
    // A June 2025 time, whose date keeps Flea's one year-month-day form in every language.
    let mtime = 1_749_981_600;
    for (language, size) in [
        ("en", "22.1 kB"),
        ("de", "22,1 kB"),
        ("ru", "22,1 kB"),
        ("fa", "\u{06F2}\u{06F2}\u{066B}\u{06F1} kB"),
        ("bn", "\u{09E8}\u{09E8}.\u{09E7} kB"),
    ] {
        let catalog = catalog(language);
        let words = Words::from_catalog(&catalog);
        assert_eq!(size_text(&file, None, &words), size, "{language}");
        let date = date_text(mtime, &words);
        let shape: String = date
            .chars()
            .map(|letter| if letter.is_numeric() { '0' } else { letter })
            .collect();
        assert_eq!(shape, "0000-00-00 00:00", "{language}: {date}");
        // The date takes the size's digits: the size's first digit is a 2.
        let zero = size.chars().next().map_or(0, u32::from) - 2;
        assert!(
            date.chars()
                .filter(|letter| letter.is_numeric())
                .all(|digit| (zero..zero + 10).contains(&u32::from(digit))),
            "{language}: {date}"
        );
    }
}
