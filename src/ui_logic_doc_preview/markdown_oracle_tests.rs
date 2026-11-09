//! Every input the JavaScript suites of v0.3.12 handed Markdown.blocks, replayed: the Rust blocks, written as the JavaScript renderer wrote them, must hash to the digest it gave.
use super::*;
use crate::jsondoc::{parse, Json};

pub(super) fn js(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub(super) fn arr<T>(items: &[T], f: impl Fn(&T) -> String) -> String {
    format!("[{}]", items.iter().map(f).collect::<Vec<_>>().join(","))
}

pub(super) fn obj(mut fields: Vec<(&str, String)>) -> String {
    fields.sort_by(|a, b| a.0.cmp(b.0));
    format!(
        "{{{}}}",
        fields
            .into_iter()
            .map(|(k, v)| format!("{}:{}", js(k), v))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn align(a: Option<Align>) -> Option<String> {
    a.map(|a| {
        js(match a {
            Align::Center => "center",
            Align::Right => "right",
        })
    })
}

pub(super) fn image(i: &Image, with_align: bool) -> String {
    let mut f = vec![
        ("type", js("image")),
        ("url", js(&i.url)),
        ("alt", js(&i.alt)),
    ];
    if let Some(w) = i.width {
        f.push(("width", w.to_string()));
    }
    if let Some(h) = i.height {
        f.push(("height", h.to_string()));
    }
    if with_align {
        if let Some(a) = align(i.align) {
            f.push(("align", a));
        }
    }
    if let Some(l) = &i.link {
        f.push(("link", js(l)));
    }
    obj(f)
}

fn markers(m: &[RunMarker]) -> String {
    arr(m, |r| {
        obj(vec![
            ("ordered", r.ordered.to_string()),
            ("marker", js(&r.marker)),
        ])
    })
}

pub fn dump(b: &Block) -> String {
    match b {
        Block::Run { text, maths } => {
            let mut f = vec![("type", js("run")), ("text", js(text))];
            if !maths.is_empty() {
                f.push(("maths", arr(maths, |m| js(m))));
            }
            obj(f)
        }
        Block::Heading {
            level,
            text,
            align: a,
        } => {
            let mut f = vec![
                ("type", js("heading")),
                ("level", level.to_string()),
                ("text", js(text)),
            ];
            if let Some(a) = align(*a) {
                f.push(("align", a));
            }
            obj(f)
        }
        Block::Fence { text, info } => obj(vec![
            ("type", js("fence")),
            ("text", js(text)),
            ("info", js(info)),
        ]),
        Block::Figure(fig) => obj(vec![
            ("type", js("figure")),
            ("kind", js(fig.kind.name())),
            ("source", js(&fig.source)),
            ("display", fig.display.to_string()),
        ]),
        Block::Quote(q) => {
            let mut f = vec![("type", js("quote")), ("text", js(&q.text))];
            if let Some(p) = &q.parts {
                f.push(("parts", arr(p, dump)));
            }
            if let Some(d) = q.depth {
                f.push(("depth", d.to_string()));
            }
            if q.joined {
                f.push(("joined", "true".into()));
            }
            obj(f)
        }
        Block::List(l) => {
            let mut f = vec![
                ("type", js("list")),
                ("ordered", l.ordered.to_string()),
                ("start", l.start.to_string()),
                ("items", arr(&l.items, |s| js(s))),
            ];
            if let Some(d) = &l.depths {
                f.push(("depths", arr(d, |x| x.to_string())));
            }
            if let Some(m) = &l.markers {
                f.push(("markers", arr(m, |x| js(x))));
            }
            if let Some(g) = &l.gaps {
                f.push(("gaps", arr(g, |x| x.to_string())));
            }
            if let Some(p) = &l.parts {
                f.push((
                    "parts",
                    arr(p, |x| match x {
                        None => "null".into(),
                        Some(bs) => arr(bs, dump),
                    }),
                ));
            }
            if let Some(last) = l.last {
                f.push(("last", last.to_string()));
            }
            if l.last.is_some() || l.joined {
                f.push(("joined", l.joined.to_string()));
            }
            if let Some(c) = &l.carry {
                f.push(("carry", markers(c)));
            }
            if let Some(t) = &l.tail {
                f.push(("tail", markers(t)));
            }
            obj(f)
        }
        Block::Table(t) => {
            let al = |a: &CellAlign| {
                js(match a {
                    CellAlign::Left => "left",
                    CellAlign::Center => "center",
                    CellAlign::Right => "right",
                })
            };
            let mut f = vec![
                ("type", js("table")),
                ("head", arr(&t.head, |s| js(s))),
                ("aligns", arr(&t.aligns, al)),
                ("rows", arr(&t.rows, |r| arr(r, |s| js(s)))),
                ("cols", t.cols.to_string()),
                ("measure", arr(&t.measure, |s| js(s))),
                ("pictures", arr(&t.pictures, |s| js(s))),
                ("weights", arr(&t.weights, |x| x.to_string())),
                ("words", arr(&t.words, |x| x.to_string())),
            ];
            if t.key.is_some() || t.joined {
                f.push(("joined", t.joined.to_string()));
            }
            if let Some(last) = t.last {
                f.push(("last", last.to_string()));
            }
            obj(f)
        }
        Block::Image(i) => image(i, true),
        Block::Remote { host, align: a } => {
            let mut f = vec![("type", js("remote")), ("host", js(host))];
            if let Some(a) = align(*a) {
                f.push(("align", a));
            }
            obj(f)
        }
        Block::Images { items, align: a } => {
            let mut f = vec![
                ("type", js("images")),
                ("items", arr(items, |i| image(i, false))),
            ];
            if let Some(a) = align(*a) {
                f.push(("align", a));
            }
            obj(f)
        }
        Block::Deep { limit } => obj(vec![("type", js("deep")), ("limit", limit.to_string())]),
    }
}

// FNV-1a over the written blocks, the digest each fixture record holds.
pub(super) fn digest(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[test]
fn every_recorded_input_writes_the_blocks_the_javascript_renderer_wrote() {
    let fixture = include_str!("../../tests/fixtures/markdown-oracle/blocks.jsonl");
    let words = Words::english();
    let mut bad = Vec::new();
    let mut total = 0;
    for (n, line) in fixture.lines().enumerate() {
        let Ok(Json::Arr(fields)) = parse(line) else {
            panic!("record {n} is a JSON array");
        };
        let fields: Vec<&str> = fields.iter().filter_map(Json::as_str).collect();
        assert_eq!(
            fields.len(),
            5,
            "record {n} holds input, folder, chrome, ink and digest"
        );
        total += 1;
        let ctx = Ctx {
            dir: fields[1],
            chrome: fields[2],
            ink: fields[3],
            words: &words,
        };
        let written = arr(&blocks(fields[0], &ctx), dump);
        if digest(&written) != fields[4] {
            let input: String = fields[0].chars().take(200).collect();
            let start: String = written.chars().take(400).collect();
            bad.push(format!("record {n} {input:?} wrote {start}"));
        }
    }
    assert_eq!(total, 2715, "every recorded input is replayed");
    assert!(
        bad.is_empty(),
        "{} of {total} differ:\n{}",
        bad.len(),
        bad.join("\n")
    );
}
