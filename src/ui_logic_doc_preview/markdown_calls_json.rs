//! The call replay's JSON: reading a recorded argument, writing a Rust answer the way JSON.stringify wrote the JavaScript one, and comparing the two.
use super::super::oracle_tests::{arr, js, obj};
use super::super::{
    Align, Block, CellAlign, Figure, FigureKind, Image, List, Quote, RunMarker, Table,
};
use crate::jsondoc::Json;
use std::collections::HashMap;

/// A number as JSON.stringify writes it: a whole value has no fraction.
pub fn num(x: f64) -> String {
    if x.fract() == 0.0 && x.abs() < 1e21 {
        format!("{}", x as i64)
    } else {
        format!("{x}")
    }
}

/// The arguments after a call, the changed ones replaced by what the Rust helper left.
pub fn after(a: &[Json], changed: &[(usize, String)]) -> Option<String> {
    let mut items: Vec<String> = a.iter().map(canon).collect();
    for (i, text) in changed {
        if let Some(item) = items.get_mut(*i) {
            *item = text.clone();
        }
    }
    Some(format!("[{}]", items.join(",")))
}

pub fn strs(items: &[String]) -> String {
    arr(items, |text| js(text))
}

pub fn s(value: &Json) -> &str {
    value.as_str().unwrap_or("")
}

pub fn f(value: &Json) -> f64 {
    value.as_f64().unwrap_or(0.0)
}

pub fn u(value: &Json) -> usize {
    f(value).max(0.0) as usize
}

pub fn b(value: &Json) -> bool {
    value.as_bool().unwrap_or(false)
}

pub fn list(value: &Json) -> &[Json] {
    value.as_array().unwrap_or(&[])
}

pub fn texts(value: &Json) -> Vec<String> {
    list(value).iter().map(|item| s(item).to_string()).collect()
}

pub fn field<'j>(value: &'j Json, key: &str) -> &'j Json {
    value.get(key).unwrap_or(&Json::Null)
}

/// An object of strings, as the references and notes are held.
pub fn string_map(value: &Json) -> HashMap<String, String> {
    let pairs = value.as_object().unwrap_or(&[]);
    pairs
        .iter()
        .map(|(k, v)| (k.clone(), s(v).to_string()))
        .collect()
}

/// An object of numbers, as the note numbers are held.
pub fn number_map(value: &Json) -> HashMap<String, usize> {
    let pairs = value.as_object().unwrap_or(&[]);
    pairs.iter().map(|(k, v)| (k.clone(), u(v))).collect()
}

/// A map written as a JSON object, keys sorted.
pub fn map_json<V>(map: &HashMap<String, V>, write: impl Fn(&V) -> String) -> String {
    obj(map.iter().map(|(k, v)| (k.as_str(), write(v))).collect())
}

/// The UTF-16 offset JavaScript gives the byte offset `at` of `text`.
pub fn unit_of(text: &str, at: usize) -> usize {
    text[..at.min(text.len())].encode_utf16().count()
}

/// The byte offset of the UTF-16 offset `unit`; None inside a character.
pub fn byte_of(text: &str, unit: usize) -> Option<usize> {
    let mut units = 0;
    for (at, c) in text.char_indices() {
        if units == unit {
            return Some(at);
        }
        units += c.len_utf16();
        if units > unit {
            return None;
        }
    }
    (units == unit).then_some(text.len())
}

/// Whether two JSON values are the same: numbers by value and objects by key, whatever their order.
pub fn same(a: &Json, b: &Json) -> bool {
    match (a, b) {
        (Json::Num(x), Json::Num(y)) => x.parse::<f64>().ok() == y.parse::<f64>().ok(),
        (Json::Arr(x), Json::Arr(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q))
        }
        (Json::Obj(x), Json::Obj(y)) => {
            x.len() == y.len()
                && x.iter().all(|(key, value)| {
                    y.iter()
                        .find(|(other, _)| other == key)
                        .is_some_and(|(_, theirs)| same(value, theirs))
                })
        }
        _ => a == b,
    }
}

/// A value written back as JSON.stringify writes it, keys sorted, so a recorded argument can be passed on whole.
pub fn canon(value: &Json) -> String {
    match value {
        Json::Null => "null".to_string(),
        Json::Bool(flag) => flag.to_string(),
        Json::Num(text) => num(text.parse::<f64>().unwrap_or(0.0)),
        Json::Str(text) => js(text),
        Json::Arr(items) => arr(items, canon),
        Json::Obj(pairs) => {
            // JSON.stringify writes integer keys first in numeric order, then the rest as they were added, sorted here.
            let mut sorted: Vec<&(String, Json)> = pairs.iter().collect();
            sorted.sort_by_key(|(key, _)| (index_key(key).is_none(), index_key(key), key.clone()));
            let fields: Vec<String> = sorted
                .iter()
                .map(|(key, item)| format!("{}:{}", js(key), canon(item)))
                .collect();
            format!("{{{}}}", fields.join(","))
        }
    }
}

// A key JavaScript treats as an array index: a canonical whole number below 2^32 - 1.
fn index_key(key: &str) -> Option<u32> {
    key.parse::<u32>()
        .ok()
        .filter(|n| *n < u32::MAX && n.to_string() == key)
}

fn align(value: &Json) -> Option<Align> {
    match value.as_str() {
        Some("center") => Some(Align::Center),
        Some("right") => Some(Align::Right),
        _ => None,
    }
}

pub fn cell_align(value: &Json) -> CellAlign {
    match value.as_str() {
        Some("center") => CellAlign::Center,
        Some("right") => CellAlign::Right,
        _ => CellAlign::Left,
    }
}

pub fn cell_align_name(value: &CellAlign) -> String {
    js(match value {
        CellAlign::Left => "left",
        CellAlign::Center => "center",
        CellAlign::Right => "right",
    })
}

fn image(value: &Json) -> Image {
    let size = |key| value.get(key).and_then(Json::as_f64).map(|x| x as u32);
    Image {
        url: s(field(value, "url")).to_string(),
        alt: s(field(value, "alt")).to_string(),
        width: size("width"),
        height: size("height"),
        align: align(field(value, "align")),
        link: value.get("link").and_then(Json::as_str).map(str::to_string),
    }
}

fn markers(value: &Json) -> Option<Vec<RunMarker>> {
    value.as_array().map(|items| {
        items
            .iter()
            .map(|item| RunMarker {
                ordered: b(field(item, "ordered")),
                marker: s(field(item, "marker")).to_string(),
            })
            .collect()
    })
}

pub fn blocks(value: &Json) -> Vec<Block> {
    list(value).iter().map(block).collect()
}

/// The block a recorded block object describes, every field it leaves out at its default.
pub fn block(value: &Json) -> Block {
    let text = s(field(value, "text")).to_string();
    let parts = |item: &Json| item.as_array().map(|_| blocks(item));
    match s(field(value, "type")) {
        "heading" => Block::Heading {
            level: u(field(value, "level")) as u8,
            text,
            align: align(field(value, "align")),
        },
        "fence" => Block::Fence {
            text,
            info: s(field(value, "info")).to_string(),
        },
        "figure" => Block::Figure(Figure {
            kind: if s(field(value, "kind")) == "mermaid" {
                FigureKind::Mermaid
            } else {
                FigureKind::Math
            },
            source: s(field(value, "source")).to_string(),
            display: b(field(value, "display")),
        }),
        "quote" => Block::Quote(Quote {
            text,
            parts: parts(field(value, "parts")),
            depth: value
                .get("depth")
                .and_then(Json::as_f64)
                .map(|x| x as usize),
            joined: b(field(value, "joined")),
        }),
        "list" => Block::List(List {
            ordered: b(field(value, "ordered")),
            start: f(field(value, "start")) as u64,
            items: texts(field(value, "items")),
            depths: value
                .get("depths")
                .map(|items| list(items).iter().map(u).collect()),
            markers: value.get("markers").map(texts),
            gaps: value
                .get("gaps")
                .map(|items| list(items).iter().map(b).collect()),
            parts: value
                .get("parts")
                .map(|items| list(items).iter().map(parts).collect()),
            last: value.get("last").and_then(Json::as_f64).map(|x| x as u64),
            joined: b(field(value, "joined")),
            carry: markers(field(value, "carry")),
            tail: markers(field(value, "tail")),
        }),
        "table" => Block::Table(Table {
            head: texts(field(value, "head")),
            aligns: list(field(value, "aligns"))
                .iter()
                .map(cell_align)
                .collect(),
            rows: list(field(value, "rows")).iter().map(texts).collect(),
            cols: u(field(value, "cols")),
            measure: texts(field(value, "measure")),
            pictures: texts(field(value, "pictures")),
            weights: list(field(value, "weights")).iter().map(u).collect(),
            words: list(field(value, "words")).iter().map(u).collect(),
            joined: b(field(value, "joined")),
            key: None,
            last: value.get("last").and_then(Json::as_bool),
        }),
        "image" => Block::Image(image(value)),
        "remote" => Block::Remote {
            host: s(field(value, "host")).to_string(),
            align: align(field(value, "align")),
        },
        "images" => Block::Images {
            items: list(field(value, "items")).iter().map(image).collect(),
            align: align(field(value, "align")),
        },
        "deep" => Block::Deep {
            limit: u(field(value, "limit")),
        },
        _ => Block::Run {
            text,
            maths: texts(field(value, "maths")),
        },
    }
}
