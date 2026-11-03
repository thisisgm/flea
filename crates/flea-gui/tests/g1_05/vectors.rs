use super::{
    json::{map, V},
    oracle, root, scratch, Lcg,
};
use flea_gui::input::scroll::Phase;
use std::{io::Write, path::PathBuf};

/// The vector file and how many rows it holds; every row is one oracle decision or one replayed sequence.
pub(super) struct Out {
    file: std::fs::File,
    rows: usize,
}

impl Out {
    pub(super) fn row(&mut self, kind: &str, mut fields: Vec<(&str, V)>) {
        fields.insert(0, ("kind", kind.into()));
        writeln!(self.file, "{}", map(fields)).unwrap();
        self.rows += 1;
    }

    pub(super) fn call(&mut self, module: &str, name: &str, args: Vec<V>, out: impl Into<V>) {
        self.row(
            "fn",
            vec![
                ("m", module.into()),
                ("f", name.into()),
                ("args", V::List(args)),
                ("out", out.into()),
            ],
        );
    }
}

/// A number on an eighth-pixel grid, so the file spells it exactly and both sides read the same value.
pub(super) fn real(lcg: &mut Lcg, low: f64, high: f64) -> f64 {
    low + lcg.below(((high - low) * 8.) as u64 + 1) as f64 / 8.
}

pub(super) fn int(lcg: &mut Lcg, low: i64, high: i64) -> i64 {
    low + lcg.below((high - low + 1) as u64) as i64
}

/// The oracle's spelling of a missing position.
pub(super) fn index(found: Option<usize>) -> V {
    found.map_or(V::Num(-1.), V::from)
}

/// Qt's scroll phase numbers, NoScrollPhase first.
pub(super) fn qt_phase(phase: Phase) -> V {
    V::from(match phase {
        Phase::None => 0usize,
        Phase::Begin => 1,
        Phase::Update => 2,
        Phase::End => 3,
    })
}

pub(super) fn nums(values: &[f64]) -> Vec<V> {
    values.iter().map(|value| V::from(*value)).collect()
}

/// The fewest rows a whole matrix writes; tests/g1-05.qml refuses a file under the same rowFloor.
const ROW_FLOOR: usize = 7_000;

#[test]
fn export_oracle_vectors_for_scroll_physics_and_slow_clicks() {
    let root = root();
    let output =
        std::env::var_os("G1_05_VECTORS").map_or_else(|| root.join("vectors.jsonl"), PathBuf::from);
    let parent = output.parent().unwrap();
    assert!(output.is_absolute() && parent.join(".flea-test-sandbox").is_file());
    assert!(parent
        .canonicalize()
        .unwrap()
        .starts_with(scratch().canonicalize().unwrap()));
    let file = std::fs::File::options()
        .write(true)
        .create_new(true)
        .open(output)
        .unwrap();
    let mut out = Out { file, rows: 0 };
    let mut lcg = Lcg(0x0105_2026_1018);
    oracle::scroll_functions(&mut out, &mut lcg);
    oracle::slow_vectors(&mut out, &mut lcg);
    assert!(
        out.rows >= ROW_FLOOR,
        "empty or incomplete vector matrix: {}",
        out.rows
    );
    println!("G1_05 vectors: {} oracle rows", out.rows);
}
