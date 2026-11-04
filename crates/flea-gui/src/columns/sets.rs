//! ui/js/Columns.js and ColumnFit.js for the list views: the column sets a width draws, the floors that drop them and the autofit widths.
use crate::row;

pub const MIN_LIST_WIDTH: u32 = 48;
pub const MAX_LIST_WIDTH: u32 = 480;

/// storedNumber: only a finite non-negative JSON number is a stored width.
pub fn stored_number(raw: &flea::jsondoc::Json) -> Option<u32> {
    match raw {
        flea::jsondoc::Json::Num(text) => text
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite() && *value >= 0.)
            .map(|value| value.round() as u32),
        _ => None,
    }
}

pub fn clamp_list_width(px: f64) -> u32 {
    let n = px.round();
    if n.is_nan() || n < 0. {
        return MIN_LIST_WIDTH;
    }
    (n as u32).clamp(MIN_LIST_WIDTH, MAX_LIST_WIDTH)
}

/// autofitWidth: the widest held cell clamped; no held cell keeps the column it has.
pub fn autofit_width(widths: &[f64], fallback: f64) -> u32 {
    if widths.is_empty() {
        return clamp_list_width(fallback);
    }
    let best = widths
        .iter()
        .map(|width| width.round())
        .filter(|width| *width >= 0.)
        .fold(f64::from(MIN_LIST_WIDTH), f64::max);
    clamp_list_width(best)
}

/// Theme tokens the list floors read: row padding, gap, mark slot, the name's floor and one width per column.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub row_padding_x: f32,
    pub gap: f32,
    pub icon_size: f32,
    pub name_min: f32,
    pub mode: f32,
    pub size: f32,
    pub date: f32,
    pub kind: f32,
    pub location: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Set {
    pub location: bool,
    pub mode: bool,
    pub size: bool,
    pub date: bool,
    pub kind: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Floors {
    pub mode: f32,
    pub size: f32,
    pub date: f32,
    pub kind: f32,
}

/// floors: each optional column's row width, nesting because they drop kind, date, size, then mode.
pub fn floors(t: &Tokens) -> Floors {
    let mode =
        t.row_padding_x + t.icon_size + t.gap + t.name_min + t.row_padding_x + t.mode + t.gap;
    let size = mode + t.size + t.gap;
    let date = size + t.date + t.gap;
    Floors {
        mode,
        size,
        date,
        kind: date + t.kind + t.gap,
    }
}

pub fn set(width: f32, t: &Tokens, hidden: &[&str]) -> Set {
    let f = floors(t);
    let shown = |key: &str, floor: f32| width >= floor && !hidden.contains(&key);
    Set {
        location: false,
        mode: shown("mode", f.mode),
        size: shown("size", f.size),
        date: shown("date", f.date),
        kind: shown("kind", f.kind),
    }
}

/// dualSet: a Dual pane protects its name floor without reserving absent Mode and Kind.
pub fn dual_set(width: f32, t: &Tokens, hidden: &[&str]) -> Set {
    let base = 2. * t.row_padding_x + t.icon_size + t.gap + t.name_min;
    let size = !hidden.contains(&"size") && width >= base + t.size;
    let date = !hidden.contains(&"date") && width >= base + if size { t.size } else { 0. } + t.date;
    Set {
        size,
        date,
        ..Set::default()
    }
}

/// recentSet: Recent reserves every metadata width it would show for Location.
pub fn recent_set(width: f32, t: &Tokens, hidden: &[&str], dual: bool) -> Set {
    let base = 2. * t.row_padding_x + t.icon_size + t.gap + t.name_min;
    let gap = if dual { 0. } else { t.gap };
    let size = !hidden.contains(&"size") && width >= base + t.size + gap;
    let slot = if size { t.size + gap } else { 0. };
    let date = !hidden.contains(&"date") && width >= base + slot + t.date + gap;
    let reserve = |key, width: f32| {
        if hidden.contains(&key) {
            0.
        } else {
            width + gap
        }
    };
    let floor = base + reserve("size", t.size) + reserve("date", t.date) + t.location + t.gap;
    Set {
        size,
        date,
        location: width >= floor,
        ..Set::default()
    }
}

/// names: the drawn columns left to right, the string the header and every row must agree on.
pub fn names(s: Set) -> String {
    let mut out = String::from("name");
    for (on, key) in [
        (s.location, "location"),
        (s.mode, "mode"),
        (s.size, "size"),
        (s.date, "date"),
        (s.kind, "kind"),
    ] {
        if on {
            out.push(',');
            out.push_str(key);
        }
    }
    out
}

/// Format.permissions: nine rwx letters from the low mode bits.
pub fn permissions(mode: u32) -> String {
    (0..9)
        .map(|at| {
            if mode & (1 << (8 - at)) != 0 {
                ['r', 'w', 'x'][at % 3]
            } else {
                '-'
            }
        })
        .collect()
}

/// ColumnFit.cellText: the string a list cell draws, which a header autofit measures.
pub fn cell_text(
    key: &str,
    facts: &flea::protocol::RowFacts,
    mtime: Option<i64>,
    kind: Option<&str>,
    walk: Option<(u64, bool)>,
    words: &row::format::Words,
) -> String {
    match key {
        "mode" => permissions(facts.mode),
        "size" => row::format::size_text(
            facts,
            walk.map(|(bytes, partial)| crate::navigation::sort::Size { bytes, partial }),
            words,
        ),
        "date" => mtime
            .and_then(|stamp| words.locale.format_date(stamp))
            .unwrap_or_else(|| "--".into()),
        _ => kind.unwrap_or("").into(),
    }
}
