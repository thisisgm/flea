//! Replays of the helpers that read or write a reference state: definitions stored and hidden, a whole pass, and the prepared text.
use super::super::markdown_blocks::block_pass;
use super::super::markdown_event::Line;
use super::super::markdown_html::document_text;
use super::super::markdown_items::visible_lines;
use super::super::markdown_refs::{hide_definition, store_definition, Hidden, Lines, Refs};
use super::super::markdown_run::{parse_inline, Env, Shape};
use super::super::oracle_tests::{arr, js, obj};
use super::super::{Ctx, Words};
use super::json::{after, b, field, list, map_json, number_map, s, string_map, strs, texts, u};
use super::Answer;
use crate::jsondoc::Json;
use std::collections::BTreeMap;

fn flags(keys: impl Iterator<Item = (usize, String)>) -> String {
    let sorted: BTreeMap<String, String> = keys.map(|(i, v)| (i.to_string(), v)).collect();
    obj(sorted
        .iter()
        .map(|(k, v)| (k.as_str(), v.clone()))
        .collect())
}

/// A reference state from its recorded object.
pub fn state(value: &Json) -> (Refs, Lines) {
    let keys = |key| {
        field(value, key)
            .as_object()
            .unwrap_or(&[])
            .iter()
            .filter_map(|(k, v)| k.parse::<usize>().ok().map(|i| (i, v)))
            .collect::<Vec<_>>()
    };
    let refs = Refs {
        defs: string_map(field(value, "defs")),
        // JavaScript holds each note as {text}.
        notes: field(value, "notes")
            .as_object()
            .unwrap_or(&[])
            .iter()
            .map(|(k, v)| (k.clone(), s(field(v, "text")).to_string()))
            .collect(),
        numbers: number_map(field(value, "numbers")),
    };
    let lines = Lines {
        hidden: keys("hidden")
            .into_iter()
            .map(|(i, v)| {
                (
                    i,
                    if v.as_str() == Some("paragraph") {
                        Hidden::Paragraph
                    } else {
                        Hidden::Line
                    },
                )
            })
            .collect(),
        escaped: keys("escaped").into_iter().map(|(i, _)| i).collect(),
        code: keys("code").into_iter().map(|(i, _)| i).collect(),
        dropped: list(field(value, "dropped"))
            .iter()
            .map(|pair| (u(&list(pair)[0]), u(&list(pair)[1])))
            .collect(),
        deep: b(field(value, "deep")),
    };
    (refs, lines)
}

/// A reference state written as the JavaScript state object.
pub fn state_json(refs: &Refs, lines: &Lines) -> String {
    let mut fields = vec![
        (
            "code",
            flags(lines.code.iter().map(|i| (*i, "true".to_string()))),
        ),
        ("defs", map_json(&refs.defs, |v| js(v))),
        (
            "dropped",
            arr(&lines.dropped, |(a, b)| format!("[{a},{b}]")),
        ),
        (
            "escaped",
            flags(lines.escaped.iter().map(|i| (*i, "true".to_string()))),
        ),
        (
            "hidden",
            flags(lines.hidden.iter().map(|(i, v)| {
                (
                    *i,
                    if *v == Hidden::Paragraph {
                        js("paragraph")
                    } else {
                        "true".to_string()
                    },
                )
            })),
        ),
        (
            "notes",
            map_json(&refs.notes, |v| obj(vec![("text", js(v))])),
        ),
        ("numbers", map_json(&refs.numbers, |v| v.to_string())),
    ];
    if lines.deep {
        fields.push(("deep", "true".to_string()));
    }
    obj(fields)
}

// The document as one inline text: code lines verbatim, prose through the inline pass, what the suites read as MdBlocks.prepare.
fn prepared(a: &[Json]) -> String {
    let arg = |i: usize| a.get(i).unwrap_or(&Json::Null);
    let text = document_text(s(arg(0)));
    let lines: Vec<&str> = text.split('\n').collect();
    let (mut refs, mut held) = (Refs::default(), Lines::default());
    block_pass(&lines, Some(&mut refs), &mut held, false, None, None);
    let defs = match arg(2).as_object() {
        Some(_) => string_map(arg(2)),
        None => refs.defs.clone(),
    };
    let words = Words::english();
    let ctx = Ctx {
        dir: s(arg(1)),
        chrome: s(arg(3)),
        ink: s(arg(4)),
        words: &words,
    };
    let (mut tokens, mut cited, mut out, mut prose) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut flush = |prose: &mut Vec<String>, out: &mut Vec<String>| {
        if !prose.is_empty() {
            let mut env = Env {
                ctx: &ctx,
                defs: &defs,
                numbers: &mut refs.numbers,
                tokens: &mut tokens,
                cited: Some(&mut cited),
            };
            out.push(parse_inline(&prose.join("\n"), &mut env, Shape::default()));
        }
        prose.clear();
    };
    for (i, line) in lines.iter().enumerate() {
        if held.hidden.contains_key(&i) {
            continue;
        }
        if held.code.contains(&i) {
            flush(&mut prose, &mut out);
            out.push(line.to_string());
        } else {
            let one = Line {
                text: line.to_string(),
                index: i,
                raw: false,
                hidden: false,
                escaped: held.escaped.contains(&i),
            };
            prose.extend(visible_lines(&[one]).texts);
        }
    }
    flush(&mut prose, &mut out);
    out.join("\n")
}

/// Replays one call, or None for a helper this file does not hold.
pub fn call(name: &str, a: &[Json]) -> Option<Answer> {
    let arg = |i: usize| a.get(i).unwrap_or(&Json::Null);
    match name {
        "MdRefs.storeDefinition" => {
            let mut defs = string_map(arg(0));
            store_definition(&mut defs, s(arg(1)).to_string(), s(arg(2)).to_string());
            Some(Answer {
                result: "null".into(),
                after: after(a, &[(0, map_json(&defs, |v| js(v)))]),
            })
        }
        "MdRefs.hideDefinition" => {
            let (refs, mut lines) = state(arg(0));
            hide_definition(&mut lines, u(arg(1)), u(arg(2)));
            Some(Answer {
                result: "null".into(),
                after: after(a, &[(0, state_json(&refs, &lines))]),
            })
        }
        "MdBlocks.referenceState" => Answer::of(state_json(&Refs::default(), &Lines::default())),
        "MdBlocks.collectReferences" => {
            let text = document_text(s(arg(0)));
            let lines: Vec<&str> = text.split('\n').collect();
            let (mut refs, mut held) = (Refs::default(), Lines::default());
            block_pass(&lines, Some(&mut refs), &mut held, false, None, None);
            Answer::of(state_json(&refs, &held))
        }
        "MdBlocks.blockPass" => {
            let lines = texts(arg(0));
            let refs_of: Vec<&str> = lines.iter().map(String::as_str).collect();
            let (mut refs, mut held) = state(arg(1));
            let refs_arg = if b(arg(3)) { Some(&mut refs) } else { None };
            block_pass(&refs_of, refs_arg, &mut held, b(arg(4)), None, None);
            Some(Answer {
                result: "null".into(),
                after: after(a, &[(1, state_json(&refs, &held))]),
            })
        }
        "MdBlocks.prepare" => Answer::of(js(&prepared(a))),
        "MdItems.visibleLines" => {
            let (_, held) = state(arg(1));
            let lines: Vec<Line> = list(arg(0))
                .iter()
                .map(|item| {
                    let index = u(field(item, "index"));
                    Line {
                        text: s(field(item, "text")).to_string(),
                        index,
                        raw: b(field(item, "raw")),
                        hidden: held.hidden.contains_key(&index),
                        escaped: held.escaped.contains(&index),
                    }
                })
                .collect();
            Answer::of(strs(&visible_lines(&lines).texts))
        }
        _ => None,
    }
}
