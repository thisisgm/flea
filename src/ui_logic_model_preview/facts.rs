//! The words a 3D preview shows: the Quick Look bar's one fact, its plate words, the layer row and the preview column's facts.
use super::wire::{Data, Facts, Slicer};
use crate::i18n::I18n;

/// The file's own facts the listing already knows, formatted by the caller.
#[derive(Clone, Copy, Debug)]
pub struct FileFacts<'a> {
    pub kind: &'a str,
    pub size: &'a str,
    pub modified: &'a str,
}

/// One label and value row of the facts table.
pub type Row = (String, String);

fn one(i18n: &I18n, value: f64, decimals: usize) -> String {
    i18n.locale().format_number(value, decimals)
}

/// `X × Y × Z mm`, one decimal in the locale's digits and separator.
pub fn size_mm(i18n: &I18n, d: [f64; 3]) -> String {
    let [x, y, z] = d.map(|v| one(i18n, v, 1));
    i18n.tr_args("model3d.value.size_mm", &[&x, &y, &z])
}

/// A print time as `1 h 12 min`, minutes two digits under an hour count, or `45 min` under an hour.
pub fn print_time(i18n: &I18n, seconds: f64) -> Option<String> {
    if !seconds.is_finite() || seconds < 0. {
        return None;
    }
    let minutes = (seconds / 60.).round() as u64;
    let (h, m) = (minutes / 60, minutes % 60);
    let digits = |n: u64, width: usize| i18n.locale().localize_number(&format!("{n:0width$}"));
    Some(if h == 0 {
        i18n.tr_args("model3d.value.minutes", &[&digits(m, 1)])
    } else {
        i18n.tr_args("model3d.value.hours_minutes", &[&digits(h, 1), &digits(m, 2)])
    })
}

/// The layer row's label, `Layer 128 of 240 · 25.60 mm`; `layer` counts from 1.
pub fn layer_label(i18n: &I18n, layer: usize, total: usize, z_mm: f32) -> String {
    let count = |n: usize| i18n.locale().format_count(n as i64);
    i18n.tr_args("model3d.value.layer", &[&count(layer), &count(total), &one(i18n, f64::from(z_mm), 2)])
}

/// The bar's bare plate words, Plate 1, Plate 2 and on; nothing for a single plate.
pub fn plate_words(i18n: &I18n, count: usize) -> Vec<String> {
    if count < 2 {
        return Vec::new();
    }
    (1..=count).map(|n| i18n.tr_args("model3d.plate", &[&i18n.locale().format_count(n as i64)])).collect()
}

/// The shared failure sentence; the backend's reason stays diagnostic data.
pub fn cannot_preview(i18n: &I18n) -> String {
    i18n.tr("model3d.cannot_preview")
}

fn toolpath(format: &str) -> bool {
    matches!(format, "gcode" | "bgcode")
}

fn plate_facts(data: &Data, plate: usize) -> (&Facts, &Slicer) {
    match data.plates.get(plate) {
        Some(p) if data.plates.len() > 1 => (&p.facts, &p.slicer),
        _ => (&data.facts, &data.slicer),
    }
}

/// The bar's one fact: the chosen plate's size in mm, or a print's time where the slicer recorded it.
pub fn bar_fact(i18n: &I18n, data: &Data, plate: usize) -> Option<String> {
    let (facts, _) = plate_facts(data, plate);
    if toolpath(&data.format) {
        if let Some(time) = data.slicer.print_time_s.and_then(|s| print_time(i18n, s)) {
            return Some(time);
        }
    }
    facts.dimensions_mm.map(|d| size_mm(i18n, d))
}

fn label(i18n: &I18n, key: &str) -> String {
    i18n.tr(&format!("model3d.fact.{key}"))
}

/// The preview column's facts in two groups, the file's and the model's, then the slicer's; an unknown fact is left out; a 3MF project gives Objects and Plates, a mesh its Volume, Surface, Triangles and Watertight, a print its slicer facts.
pub fn rows(i18n: &I18n, file: FileFacts, data: &Data, plate: usize) -> Vec<Vec<Row>> {
    let (facts, _) = plate_facts(data, plate);
    let mut first: Vec<Row> = vec![
        (label(i18n, "kind"), file.kind.into()),
        (label(i18n, "size"), file.size.into()),
        (label(i18n, "modified"), file.modified.into()),
    ];
    if let Some(d) = facts.dimensions_mm {
        first.push((label(i18n, "dimensions"), size_mm(i18n, d)));
    }
    let count = |n: u64| i18n.locale().format_count(n.min(i64::MAX as u64) as i64);
    if data.format == "3mf" {
        if let Some(n) = data.facts.objects {
            first.push((label(i18n, "objects"), count(n)));
        }
        let plates = data.facts.plates.unwrap_or(data.plates.len() as u64).max(1);
        first.push((label(i18n, "plates"), count(plates)));
    } else if !toolpath(&data.format) {
        if let Some(v) = facts.volume_cm3 {
            first.push((label(i18n, "volume"), i18n.tr_args("model3d.value.cm3", &[&one(i18n, v, 1)])));
        }
        if let Some(v) = facts.surface_cm2 {
            first.push((label(i18n, "surface"), i18n.tr_args("model3d.value.cm2", &[&one(i18n, v, 1)])));
        }
        if let Some(n) = facts.triangles {
            first.push((label(i18n, "triangles"), count(n)));
        }
        if let Some(open) = facts.open_edges {
            let value = if open == 0 { i18n.tr("model3d.value.watertight") } else { i18n.trn("model3d.value.open_edges", open.min(i64::MAX as u64) as i64) };
            first.push((label(i18n, "watertight"), value));
        }
    }
    let s = &data.slicer;
    let mm = |v: f64| i18n.tr_args("model3d.value.mm", &[&one(i18n, v, 2)]);
    let second: Vec<Row> = [
        s.printer.clone().map(|v| (label(i18n, "printer"), v)),
        s.filament.clone().map(|v| (label(i18n, "filament"), v)),
        s.layer_height_mm.map(|v| (label(i18n, "layer_height"), mm(v))),
        s.print_time_s.and_then(|v| print_time(i18n, v)).map(|v| (label(i18n, "print_time"), v)),
        s.weight_g.map(|v| (label(i18n, "weight"), i18n.tr_args("model3d.value.g", &[&one(i18n, v, 1)]))),
    ]
    .into_iter()
    .flatten()
    .collect();
    let mut groups = vec![first];
    if !second.is_empty() {
        groups.push(second);
    }
    groups
}
