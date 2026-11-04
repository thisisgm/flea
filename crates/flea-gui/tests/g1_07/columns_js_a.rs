//! tests/js/columns.js run() head: column floors, the set each width draws, the chooser's set.
use super::check::Checks;
use flea_gui::columns::sets::{dual_set, floors, names, set, Set, Tokens};

/// BOX: this box's resolved tokens at base-size 14, the name floor 20 advances of 7.8125 px.
const BOX: Tokens = Tokens {
    row_padding_x: 14.,
    gap: 9.,
    icon_size: 23.,
    name_min: 156.,
    mode: 70.,
    size: 70.,
    date: 125.,
    kind: 130.,
    location: 0.,
};

/// OTHER: a token set sharing no number with BOX, so no check passes on a constant.
const OTHER: Tokens = Tokens {
    row_padding_x: 6.,
    gap: 4.,
    icon_size: 16.,
    name_min: 100.,
    mode: 40.,
    size: 50.,
    date: 80.,
    kind: 60.,
    location: 0.,
};

/// PICKER: BOX with the chooser's 80 px date column in place of the window's 125.
const PICKER: Tokens = Tokens { date: 80., ..BOX };

/// The chooser's list area: the 875 px floating picker less its 175 px places rail.
const PICKER_SLOT: f32 = 700.;

/// Picker.js HIDDEN_COLS, the set the chooser hands each row in place of the window's own.
const PICKER_HIDDEN: [&str; 2] = ["mode", "kind"];

/// ui/Row.qml's anchor chain apart from the model: padding, mark, gap and each drawn column.
fn name_slot(width: f32, s: Set, t: &Tokens) -> f32 {
    let mut used = t.row_padding_x + t.icon_size + t.gap + t.row_padding_x;
    if s.mode {
        used += t.mode + t.gap;
    }
    if s.size {
        used += t.size + t.gap;
    }
    if s.date {
        used += t.date + t.gap;
    }
    if s.kind {
        used += t.kind + t.gap;
    }
    width - used
}

/// True when a narrower row's set draws a column the wider row before it had dropped.
fn grew(s: Set, previous: Set) -> bool {
    (s.mode && !previous.mode)
        || (s.size && !previous.size)
        || (s.date && !previous.date)
        || (s.kind && !previous.kind)
}

#[test]
fn a_dual_pane_keeps_its_name_floor() {
    let mut c = Checks::new("tests/js/columns.js");
    let dual = Tokens {
        row_padding_x: 14.,
        gap: 9.,
        icon_size: 13. * 1.45,
        name_min: 180.,
        mode: 0.,
        size: 70.,
        date: 125.,
        kind: 0.,
        location: 0.,
    };
    c.eq(
        45,
        "dual date fits the board's 431px floor",
        names(dual_set(431., &dual, &[])),
        "name,size,date".to_owned(),
    );
    c.eq(
        46,
        "dual date drops below its name floor",
        names(dual_set(430., &dual, &[])),
        "name,size".to_owned(),
    );
    c.eq(
        47,
        "hidden dual size releases its actual width",
        names(dual_set(361., &dual, &["size"])),
        "name,date".to_owned(),
    );
    c.eq(
        48,
        "dual minimum still keeps Name",
        names(dual_set(200., &dual, &[])),
        "name".to_owned(),
    );
    // runHidden and runPicker run as tests of their own.
    c.covers(&[49, 50]);
    c.finish();
}

#[test]
fn the_name_never_loses_and_the_metadata_drops_in_order() {
    let mut c = Checks::new("tests/js/columns.js");
    let f = floors(&BOX);
    c.eq(
        53,
        "mode needs the name's floor plus its own column and gap",
        f.mode,
        295.,
    );
    c.eq(54, "size needs mode's floor plus its own", f.size, 374.);
    c.eq(55, "date needs size's floor plus its own", f.date, 508.);
    c.eq(56, "kind needs date's floor plus its own", f.kind, 647.);
    c.ok(
        58,
        "the four floors nest, widest last",
        f.mode < f.size && f.size < f.date && f.date < f.kind,
    );
    c.eq(
        62,
        "the default window draws every column",
        names(set(732., &BOX, &[])),
        "name,mode,size,date,kind".to_owned(),
    );
    c.ok(
        64,
        "a column is kept at exactly its floor",
        set(647., &BOX, &[]).kind,
    );
    c.ok(
        66,
        "and dropped one pixel under it",
        !set(646., &BOX, &[]).kind,
    );
    c.eq(
        68,
        "kind goes first and the other three stay",
        names(set(646., &BOX, &[])),
        "name,mode,size,date".to_owned(),
    );
    c.eq(
        70,
        "date goes second",
        names(set(507., &BOX, &[])),
        "name,mode,size".to_owned(),
    );
    c.eq(
        72,
        "size goes third",
        names(set(373., &BOX, &[])),
        "name,mode".to_owned(),
    );
    c.eq(
        74,
        "mode goes last, and the last layout is the mark and the name",
        names(set(294., &BOX, &[])),
        "name".to_owned(),
    );
    c.eq(
        77,
        "the width that drew no name at all now draws the name, mode and size",
        names(set(453., &BOX, &[])),
        "name,mode,size".to_owned(),
    );
    let mut every_width_keeps_the_name = true;
    let mut never_grows_as_it_narrows = true;
    let mut previous: Option<Set> = None;
    for w in (216_i16..=2000).rev() {
        let w = f32::from(w);
        let s = set(w, &BOX, &[]);
        if name_slot(w, s, &BOX) < BOX.name_min {
            every_width_keeps_the_name = false;
        }
        if previous.is_some_and(|wider| grew(s, wider)) {
            never_grows_as_it_narrows = false;
        }
        previous = Some(s);
    }
    c.ok(
        95,
        "every width from the name's own floor up keeps the name at or above it",
        every_width_keeps_the_name,
    );
    c.ok(
        97,
        "no column ever comes back as the row narrows",
        never_grows_as_it_narrows,
    );
    c.eq(
        102,
        "under the last rung the set is empty rather than undefined",
        names(set(100., &BOX, &[])),
        "name".to_owned(),
    );
    c.eq(
        104,
        "a zero width answers rather than throwing",
        names(set(0., &BOX, &[])),
        "name".to_owned(),
    );
    c.eq(
        105,
        "a negative width answers the same",
        names(set(-500., &BOX, &[])),
        "name".to_owned(),
    );
    let g = floors(&OTHER);
    c.eq(
        109,
        "another token set moves every floor with it",
        format!("{}|{}|{}|{}", g.mode, g.size, g.date, g.kind),
        "176|230|314|378".to_owned(),
    );
    c.ok(
        111,
        "and keeps them nested",
        g.mode < g.size && g.size < g.date && g.date < g.kind,
    );
    c.ok(
        113,
        "and keeps the name above its own floor there too",
        name_slot(g.kind, set(g.kind, &OTHER, &[]), &OTHER) >= OTHER.name_min,
    );
    let every = Set {
        mode: true,
        size: true,
        date: true,
        kind: true,
        ..Set::default()
    };
    c.eq(
        117,
        "the set names the columns left to right, not in drop order",
        names(every),
        "name,mode,size,date,kind".to_owned(),
    );
    c.eq(
        120,
        "the name is in the set even when everything else is gone",
        names(Set::default()),
        "name".to_owned(),
    );
    // run() hands on to its sections, each a test of its own here.
    c.covers(&[123, 124, 125, 126, 127, 128, 129, 130, 131, 132, 133, 134]);
    c.finish();
}

#[test]
fn the_chooser_draws_the_boards_three_columns() {
    let mut c = Checks::new("tests/js/columns.js");
    c.eq(
        295,
        "the chooser's own slot is wide enough for every column",
        names(set(PICKER_SLOT, &PICKER, &[])),
        "name,mode,size,date,kind".to_owned(),
    );
    c.eq(
        297,
        "the chooser draws the board's three and nothing else",
        names(set(PICKER_SLOT, &PICKER, &PICKER_HIDDEN)),
        "name,size,date".to_owned(),
    );
    let mut ever_drawn = false;
    for w in (0_i16..=3000).rev() {
        let s = set(f32::from(w), &PICKER, &PICKER_HIDDEN);
        if s.mode || s.kind {
            ever_drawn = true;
        }
    }
    c.ok(
        307,
        "no width at all draws Mode or Kind in the chooser",
        !ever_drawn,
    );
    c.finish();
}
