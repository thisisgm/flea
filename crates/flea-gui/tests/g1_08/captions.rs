//! tests/js/names.js's grid caption cases, and every output of the frozen GridNames.js over its seed corpus.
use flea_gui::grid::captions::{caption_marks, grid_caption, last_line_cells, run, LineMark};

// Format.js cellWidthOf over a line, a combining mark riding with the char before it.
fn cells(line: &str) -> usize {
    line.chars()
        .filter(|ch| !('\u{300}'..='\u{36f}').contains(ch))
        .map(|ch| if wide(ch) { 2 } else { 1 })
        .sum()
}
fn wide(ch: char) -> bool {
    matches!(ch as u32, 0x1100..=0x115F | 0x2E80..=0xA4CF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF | 0xFF00..=0xFF60 | 0x1F000..=0x1FAFF | 0x20000..=0x3FFFD)
}
fn fits(caption: &str, per: usize) -> bool {
    caption.split('\n').all(|line| cells(line) <= per)
}

#[test]
fn the_board_sample_breaks_deterministically_and_every_line_fits() {
    let full = "screenshot-2026-08-30-final-review-for-gm-after-the-bench-v3.png";
    let board = grid_caption(full, 16., 2.);
    assert_eq!(board, "screenshot-2026-\n\u{2026}he-bench-v3.png");
    assert!(fits(&board, 16));
    assert_eq!(
        grid_caption("my-vacation-1.jpeg", 16., 2.),
        "my-vacation-\n1.jpeg",
        "the extension's dot never breaks when an earlier separator fits"
    );
    let token = grid_caption("a verylongnamewithoutanyspacesatall.png", 16., 2.);
    assert_eq!(
        token, "a verylongnamewi\n\u{2026}spacesatall.png",
        "a short word then a long token keeps the extension"
    );
    assert!(fits(&token, 16));
    assert_eq!(
        grid_caption("IMG_4121.jpg", 16., 2.),
        "IMG_4121.jpg",
        "a name that fits in one line gains no break"
    );
    assert_eq!(grid_caption("", 16., 2.), "");
    let plain = grid_caption("a-very-long-filename-with-no-extension-at-all", 20., 2.);
    assert!(
        fits(&plain, 20) && plain.contains('\u{2026}'),
        "a name with no extension breaks, fits and elides only when it cannot fit"
    );
    let tiny = grid_caption("abcd", 1., 2.);
    assert!(
        fits(&tiny, 1) && tiny.split('\n').count() == 2,
        "a line length of 1 fits every line and still answers two lines"
    );
    let single = grid_caption(full, 16., 1.);
    assert!(
        !single.contains('\n') && fits(&single, 16) && single.ends_with(".png"),
        "one line takes no break and keeps the extension"
    );
}

#[test]
fn wide_names_keep_their_extension_and_never_overflow_a_line() {
    let party = grid_caption("\u{1f389}birthdaypartyphotos.jpg", 16., 2.);
    assert!(
        party.rsplit('\n').next().unwrap().ends_with(".jpg") && fits(&party, 16),
        "{party:?}"
    );
    let wide_name = format!("{}.pdf", "\u{4e2d}".repeat(20));
    let cjk = grid_caption(&wide_name, 16., 2.);
    assert!(
        cjk.rsplit('\n').next().unwrap().ends_with(".pdf") && fits(&cjk, 16),
        "{cjk:?}"
    );
    for (name, per, extension) in [
        (format!("{}.png", "\u{4e2d}".repeat(13)), 15., ".png"),
        (format!("a{}b.pdf", "\u{4e2d}".repeat(13)), 16., ".pdf"),
        (format!("abc{}x.jpg", "\u{4e2d}".repeat(12)), 16., ".jpg"),
    ] {
        let caption = grid_caption(&name, per, 2.);
        assert!(
            caption.rsplit('\n').next().unwrap().ends_with(extension),
            "a straddling wide name keeps {extension}: {caption:?}"
        );
        assert!(
            fits(&caption, per as usize),
            "every line fits its cells for {extension}: {caption:?}"
        );
    }
    assert_eq!(
        last_line_cells("ab\nc\u{1f389}"),
        3,
        "the last line counts its cells, not its chars"
    );
}

#[test]
fn a_dead_or_non_finite_budget_hands_the_whole_name_back() {
    let full = "screenshot-2026-08-30-final-review-for-gm-after-the-bench-v3.png";
    let forty = "a".repeat(40);
    assert_eq!(grid_caption(full, 0., 2.), full);
    assert_eq!(grid_caption(full, 16., 0.), full);
    assert_eq!(grid_caption("abcdef", f64::NAN, 2.), "abcdef");
    assert_eq!(grid_caption("abcdef", 16., f64::NAN), "abcdef");
    assert_eq!(grid_caption(&forty, f64::INFINITY, 2.), forty);
    assert_eq!(grid_caption(&forty, 16., f64::INFINITY), forty);
}

fn mark(line: usize, run: std::ops::Range<usize>, centred: usize) -> LineMark {
    LineMark { line, run, centred }
}

#[test]
fn the_match_run_splits_across_the_break_and_skips_trailing_blanks() {
    assert_eq!(
        caption_marks("appl\nication.log", "PLIC"),
        vec![mark(0, 2..4, 4), mark(1, 0..2, 11)],
        "a run across the wrap marks both lines"
    );
    assert_eq!(
        caption_marks("Map \nnotes.md", "p n"),
        vec![mark(0, 2..3, 3), mark(1, 0..1, 8)],
        "a blank past a line's last letter takes no mark"
    );
    assert_eq!(
        caption_marks("Map \nnotes.md", " n")
            .iter()
            .map(|mark| mark.line)
            .collect::<Vec<_>>(),
        [1],
        "a run that is only that blank marks the next line alone"
    );
    assert_eq!(
        caption_marks("sub/\napex.txt", "apex")
            .iter()
            .map(|mark| mark.line)
            .collect::<Vec<_>>(),
        [1]
    );
    assert_eq!(
        caption_marks("apap", "ap")[0].run.start,
        0,
        "the first run is the one marked"
    );
    assert!(
        caption_marks("notes.txt", "zz").is_empty() && caption_marks("notes.txt", "").is_empty(),
        "no run and no query mark nothing"
    );
    assert_eq!(
        run("Report.PDF", "pdf"),
        Some(7..10),
        "the run is case-insensitive like the backend's"
    );
    assert_eq!(
        run("caf\u{e9}.txt", "\u{c9}"),
        Some(3..5),
        "the run spans a multi-byte letter whole"
    );
}

// The seeds and grid of captions_golden.js, rebuilt here so the golden file holds outputs only.
const SEEDS: [&str; 13] = [
    "screenshot-2026-08-30-final-review-for-gm-after-the-bench-v3.png",
    "a-very-long-filename-with-no-extension-at-all",
    ".bashrc-hidden-config-name",
    ".profile.json",
    "name.",
    "data.yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy",
    "a b-c_d.e f",
    "\u{4e2d}\u{5199}\u{6f22}\u{5b57}\u{30c6}\u{30b9}\u{30c8}\u{d55c}\u{ae00}\u{3b1}\u{3b2}",
    "\u{1f389}\u{1f4f7}\u{1f680}\u{1f3b5}",
    "a\u{4e2d}\u{1f389}-_.Z",
    "e\u{301}clair-resume\u{301}",
    "caf\u{e9}-na\u{ef}ve-klinik",
    "x",
];
const LENS: [usize; 15] = [0, 1, 2, 3, 5, 8, 13, 15, 16, 17, 20, 31, 33, 48, 64];

fn stretched(seed: &str, len: usize) -> String {
    seed.chars()
        .cycle()
        .take(if seed.is_empty() { 0 } else { len })
        .collect()
}

// JSON.stringify's string form: the escapes it writes for these outputs.
fn unquote(line: &str) -> String {
    let inner = line
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .expect("a JSON string");
    let (mut out, mut chars) = (String::new(), inner.chars());
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('u') => {
                let code =
                    u32::from_str_radix(&chars.by_ref().take(4).collect::<String>(), 16).unwrap();
                out.push(char::from_u32(code).unwrap());
            }
            Some(other) => out.push(other),
            None => panic!("a dangling escape"),
        }
    }
    out
}

#[test]
fn every_caption_equals_the_frozen_gridnames_over_the_seed_corpus() {
    let golden = include_str!("captions.golden");
    let mut lines = golden.lines();
    let header = lines.next().unwrap();
    // tests/g1-08.sh checks this hash against ui/js/GridNames.js, so the golden file cannot drift from the oracle.
    assert!(
        header.starts_with("# GridNames.js sha256 ") && header.len() == 22 + 64,
        "golden header {header}"
    );
    let (mut cases, mut wrong) = (0, Vec::new());
    for seed in SEEDS {
        for len in LENS {
            let name = stretched(seed, len);
            for per in [1., 2., 7., 16., 40.] {
                for count in [1., 2., 3.] {
                    let want = unquote(lines.next().expect("a golden caption"));
                    let got = grid_caption(&name, per, count);
                    cases += 1;
                    if got != want {
                        wrong.push(format!("{name:?} {per} {count}: got {got:?} want {want:?}"));
                    }
                }
            }
            let want: usize = lines.next().expect("a golden cell count").parse().unwrap();
            cases += 1;
            if last_line_cells(&format!("ab\n{name}")) != want {
                wrong.push(format!("lastLineCells {name:?}"));
            }
        }
    }
    assert_eq!(
        lines.next(),
        None,
        "the golden file holds exactly the corpus"
    );
    assert_eq!(cases, 13 * 15 * 16);
    assert!(
        wrong.is_empty(),
        "{} of {cases} differ, first: {}",
        wrong.len(),
        wrong[0]
    );
}
