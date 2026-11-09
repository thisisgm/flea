//! tests/js/menuscroll.js wheel folding and the menu-fit and menu-scroll-width geometry the card draws from.
use super::copyas::{lines, Format};
use super::entry::{Entry, Text};
use super::fit::*;
use super::testkit::check;

fn rows(count: usize) -> Vec<Entry> {
    (0..count)
        .map(|i| Entry::row(format!("row{i}"), Text::Data(format!("Row {i}")), "file"))
        .collect()
}

#[test]
fn copy_as_lines() {
    let one = |path: &str, format| lines(&[path], format, "");
    check(
        "quoted wraps paths without special characters too",
        one("/home/gm/a.txt", Format::Quoted),
        "'/home/gm/a.txt'".into(),
    );
    check(
        "path keeps a space whole on its own line",
        one("/home/gm/directory two/a.txt", Format::Path),
        "/home/gm/directory two/a.txt".into(),
    );
    check(
        "name takes the leaf",
        one("/home/gm/directory two/a.txt", Format::Name),
        "a.txt".into(),
    );
    check(
        "stem drops the last extension",
        one("/home/gm/archive.tar.gz", Format::Stem),
        "archive.tar".into(),
    );
    check(
        "stem keeps a dotfile whole",
        one("/home/gm/.bashrc", Format::Stem),
        ".bashrc".into(),
    );
    check(
        "folder path takes the directory",
        one("/home/gm/directory two/a.txt", Format::Dirpath),
        "/home/gm/directory two".into(),
    );
    check(
        "uri encodes a space",
        one("/home/gm/directory two/a.txt", Format::Uri),
        "file:///home/gm/directory%20two/a.txt".into(),
    );
    check(
        "uri encodes hash and query bytes",
        one("/home/gm/a#b?c.txt", Format::Uri),
        "file:///home/gm/a%23b%3Fc.txt".into(),
    );
    check(
        "quoted wraps a space",
        one("/home/gm/directory two", Format::Quoted),
        "'/home/gm/directory two'".into(),
    );
    check(
        "quoted escapes an embedded quote",
        one("/home/gm/a'b", Format::Quoted),
        "'/home/gm/a'\\''b'".into(),
    );
    check(
        "quoted wraps a newline",
        one("/home/gm/a\nb", Format::Quoted),
        "'/home/gm/a\nb'".into(),
    );
    check(
        "quoted keeps a lossy non-UTF-8 name intact",
        one("/home/gm/a\u{fffd}b", Format::Quoted),
        "'/home/gm/a\u{fffd}b'".into(),
    );
    check(
        "every variant covers the whole selection one per line",
        lines(&["/a/one.txt", "/a/two.txt"], Format::Path, ""),
        "/a/one.txt\n/a/two.txt".into(),
    );
    check(
        "the leaf ids and the format words name one format",
        (
            Format::parse("copyStem"),
            Format::parse("stem"),
            Format::parse("bogus"),
        ),
        (Some(Format::Stem), Some(Format::Stem), None),
    );
}

fn fold(events: &[i32], mut accum: i32) -> (i32, i32) {
    let mut stepped = 0;
    for delta in events {
        let (steps, rest) = notch_steps(accum, *delta);
        stepped += steps;
        accum = rest;
    }
    (stepped, accum)
}

#[test]
fn wheel_and_touch_steps() {
    check(
        "a fractional wheel accrues but steps nowhere alone",
        notch_steps(0, -1).0,
        0,
    );
    check("eight 15-unit events step one row", fold(&[15; 8], 0).0, -1);
    check("and hold no leftover", fold(&[15; 8], 0).1, 0);
    check("four 15-unit events step nowhere", fold(&[15; 4], 0).0, 0);
    check("but keep their travel", fold(&[15; 4], 0).1, 60);
    check(
        "the next four spend what the first kept",
        fold(&[15; 4], 60).0,
        -1,
    );
    check(
        "a full notch still steps exactly one row",
        notch_steps(0, -120).0,
        1,
    );
    let held = notch_steps(0, 60).1;
    check(
        "a flip drops the held remainder",
        notch_steps(held, -15).1,
        -15,
    );
    check("and steps nothing on the flip", notch_steps(held, -15).0, 0);
    let mut accum = 0.;
    let mut stepped = 0;
    for _ in 0..20 {
        let (steps, rest) = touch_steps(accum, -2., 28.);
        stepped += steps;
        accum = rest;
    }
    check(
        "twenty -2 pixel nudges step one row, not twenty",
        stepped,
        1,
    );
    check(
        "one row height down steps one row",
        touch_steps(0., -37., 37.).0,
        1,
    );
    check("and holds no leftover", touch_steps(0., -37., 37.).1, 0.);
    check(
        "two row heights step two rows",
        touch_steps(0., -74., 37.).0,
        2,
    );
    check(
        "a partial row steps nowhere",
        touch_steps(0., -20., 37.).0,
        0,
    );
    check("but keeps its travel", touch_steps(0., -20., 37.).1, -20.);
    check(
        "the next event spends what the last one kept",
        touch_steps(-20., -20., 37.).0,
        1,
    );
    check(
        "leaving three pixels over",
        touch_steps(-20., -20., 37.).1,
        -3.,
    );
    check(
        "upward travel steps the highlight up",
        touch_steps(0., 40., 37.).0,
        -1,
    );
    check("no travel is no step", touch_steps(5., 0., 37.).0, 0);
    check(
        "a zero row height falls back to one pixel rows",
        touch_steps(0., -100., 0.).0,
        100,
    );
}

#[test]
fn width_fit() {
    let plain = RowParts {
        label: 80.,
        ..RowParts::default()
    };
    check(
        "the parts of rows that all fit stay under Theme.menuWidth",
        row_width(plain) <= MENU_W,
        true,
    );
    check("an empty card fits nothing", fit_width(0., 1000.), MENU_W);
    check(
        "the fit is the widest wanted width",
        fit_width(300.4, 1000.),
        301.,
    );
    check(
        "the frame is the parts' sum between the token and the work area",
        fit_width(
            row_width(RowParts {
                label: 260.,
                ..plain
            }),
            1000.,
        ),
        (2. + PAD_X + SLOT + GAP + 260. + PAD_X).ceil(),
    );
    check("stays inside the work area", fit_width(900., 480.), 480.);
    check(
        "a state word keeps the chevron's column so every word ends on one edge",
        row_width(RowParts { word: 40., ..plain }),
        row_width(RowParts {
            word: 40.,
            chevron: true,
            ..plain
        }),
    );
    check(
        "a key hint takes its gap and no chevron slot",
        row_width(RowParts { key: 20., ..plain }) - row_width(plain),
        GAP + 20.,
    );
    check(
        "a flyout is as wide as its widest row and its parent frame, capped by the work area",
        (
            flyout_width(300., 100., 1000.),
            flyout_width(260., 320., 1000.),
            flyout_width(300., 100., 280.),
        ),
        (300., 320., 280.),
    );
    // menu-scroll-width: a German menu 344 wide grows up to 360 when a longer label needs it.
    check(
        "a German label grows the card past 344 to the 360 it needs",
        fit_width(359.2, 1000.),
        360.,
    );
}

#[test]
fn placement() {
    let work = Rect {
        x: 0.,
        y: 0.,
        w: 480.,
        h: 240.,
    };
    let frame = place((470., 10.), MENU_W, 5 * ROW, work, 4.);
    check(
        "menu flips before shifting at the far edge",
        frame.x,
        470. - MENU_W,
    );
    check(
        "its height stops at the work area's inset",
        place((10., 10.), MENU_W, 20 * ROW, work, 4.).h,
        240. - 2. * 4.,
    );
    check(
        "and it starts inside the inset",
        place((10., 0.), MENU_W, 20 * ROW, work, 4.).y,
        4.,
    );
    let parent = Rect {
        x: 200.,
        y: 4.,
        w: MENU_W,
        h: 200.,
    };
    let flyout = place_flyout(parent, MENU_W, 6 * ROW, 40., work, 4.);
    check(
        "a flyout that fits neither side slides to the work area's right edge",
        flyout.x,
        480. - 4. - MENU_W,
    );
    let wide = Rect {
        x: 0.,
        y: 0.,
        w: 1200.,
        h: 800.,
    };
    check(
        "a flyout opens beside its parent where it fits",
        place_flyout(
            Rect {
                x: 10.,
                y: 10.,
                w: MENU_W,
                h: 300.,
            },
            MENU_W,
            3 * ROW,
            80.,
            wide,
            4.,
        )
        .x,
        10. + MENU_W,
    );
    check(
        "else it flips to its parent's left",
        place_flyout(
            Rect {
                x: 1000.,
                y: 10.,
                w: MENU_W,
                h: 300.,
            },
            MENU_W,
            3 * ROW,
            80.,
            wide,
            4.,
        )
        .x,
        1000. - MENU_W,
    );
    check(
        "its first row lines up with the row it hangs from",
        place_flyout(
            Rect {
                x: 10.,
                y: 10.,
                w: MENU_W,
                h: 300.,
            },
            MENU_W,
            3 * ROW,
            80.,
            wide,
            4.,
        )
        .y,
        80. - PAD as f32,
    );
}

#[test]
fn half_row_scroll_cue() {
    let items = rows(14);
    let view = 6 * ROW;
    check("the card scrolls", content_height(&items) > view, true);
    check(
        "at the top no top fade, a bottom fade",
        fades(&items, 0, view),
        (false, true),
    );
    let (y, h) = menu_cut(&items, 0, view, Some(0));
    check(
        "at rest the last drawn row is cut near half, saying the menu goes on",
        cue_ok(&items, y, h),
        true,
    );
    check("the fade does not wash a whole row", CLEAR < ROW, true);
    check(
        "the top fade reaches into the first cut row",
        CLEAR >= (ROW as f32 * 0.7).round() as i32,
        true,
    );
    let end = content_height(&items) - view;
    let (y, h) = scroll_for(&items, 13, 0, view);
    check(
        "at the bottom a top fade, no bottom fade",
        fades(&items, y, h),
        (true, false),
    );
    check("scrolling to the last row reaches the end", y, end);
    for cursor in 0..items.len() {
        let (y, h) = scroll_for(&items, cursor, 0, view);
        let top = row_top(&items, cursor) - y;
        check(
            "a revealed cursor row clears both fades",
            (y == 0 || top >= CLEAR - CUE_GIVE)
                && (y + h >= content_height(&items) || top + ROW <= h - (CLEAR - CUE_GIVE)),
            true,
        );
        // A cursor on the first or last row pins the menu at its start or end, a corner no board draws.
        if cursor > 0 && cursor + 1 < items.len() {
            check(
                "between the ends both edges cut a row at half",
                cue_ok(&items, y, h),
                true,
            );
        }
        if cursor == 6 {
            check(
                "between the ends both fades",
                fades(&items, y, h),
                (true, true),
            );
        }
    }
    let mut mixed = rows(12);
    mixed.insert(4, Entry::separator());
    mixed.insert(9, Entry::separator());
    // The first separator spans 148 to 158, so a scroll of 153 cuts it at the top edge.
    check(
        "a separator beside an edge never counts as the half row",
        cue_ok(&mixed, 153, view),
        false,
    );
    let short = rows(5);
    check(
        "rows that fit never scroll",
        menu_cut(&short, 0, 6 * ROW, Some(4)),
        (0, 6 * ROW),
    );
    let tight = rows(7);
    let (_, h) = menu_cut(&tight, 0, 7 * ROW - 8, Some(0));
    check(
        "rows overflowing by less than half a row stop the view at a half row instead",
        h < 7 * ROW - 8 && cue_ok(&tight, 0, h),
        true,
    );
}
