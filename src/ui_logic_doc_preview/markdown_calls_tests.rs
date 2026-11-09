//! Every call the JavaScript suites of v0.3.12 made to a Markdown helper, replayed against its Rust port: same arguments, same answer, same arguments after.
use crate::jsondoc::{parse, Json};
use std::collections::BTreeMap;

#[path = "markdown_calls_html.rs"]
mod html;
#[path = "markdown_calls_json.rs"]
mod json;
#[path = "markdown_calls_lines.rs"]
mod lines;
#[path = "markdown_calls_state.rs"]
mod state;
#[path = "markdown_calls_text.rs"]
mod text;

/// A helper's answer as JSON, and its arguments after the call when it changes them.
pub struct Answer {
    pub result: String,
    pub after: Option<String>,
}

impl Answer {
    pub fn of(result: String) -> Option<Self> {
        Some(Self {
            result,
            after: None,
        })
    }
}

/// The recorded helpers with no Rust replay, each with why; every other recorded helper must replay.
const NOT_REPLAYED: &[(&str, &str)] = &[
    ("MarkdownMaths.compose", "the Rust view draws a formula as its own picture item; no markup is spliced"),
    ("MarkdownMaths.picture", "the Rust line places the formula's box on the baseline (markdown_fit::formula_box); no <img> markup is padded around an anchor"),
    ("MarkdownMaths.spans", "the Rust run lists its formulas on the block; no markup is scanned for them"),
    ("MarkdownPictures.fit", "the Rust line caps each picture item (markdown_fit::picture_fit, markdown_fit_tests); no <img> markup is written"),
    ("MarkdownPictures.urls", "the Rust view loads the pictures its laid items hold; the blocks oracle pins that only a local picture parses to one"),
    ("Markdown.countLine", "the bar's count is the document.lines string of the catalog, formatted by the GUI"),
    ("Markdown.sourceChunk", "chunks are cut by byte, not by UTF-16 unit; markdown_source_tests carries the suite's chunk checks"),
    ("Markdown.sourceChunkStarts", "chunks are cut by byte, not by UTF-16 unit; markdown_source_tests carries the suite's chunk checks"),
    ("MdBlocks.blockJob", "the Rust parse runs on its own thread; there is no time-sliced job"),
    ("MdBlocks.sliceDue", "the Rust parse runs on its own thread; there is no time slice"),
    ("MdBlocks.useClock", "the Rust parse runs on its own thread; there is no time slice"),
    ("MdDocument.writer", "takes a callback; the blocks oracle replays every writer the suites drove"),
    ("MdFront.sendFront", "takes a callback; the blocks oracle replays every front matter the suites parsed"),
    ("MdItems.builder", "returns an object of closures; the blocks oracle replays every list the suites built"),
    ("MdItems.listBlock", "takes a builder of closures; the blocks oracle replays every list the suites built"),
    ("MdItems.quoteBlocks", "takes a writer of closures; the blocks oracle replays every quote the suites built"),
    ("MdResolve.resolvePair", "takes a host of closures in Rust; parseInline replays every pair the suites resolved"),
    ("MarkdownPrepared.store", "the Rust cache holds one entry behind a lock; markdown_prepared tests carry store and take"),
    ("MarkdownPrepared.take", "the Rust cache holds one entry behind a lock; markdown_prepared tests carry store and take"),
];

/// Answer fields no JavaScript caller read, which the Rust port therefore does not keep; each is left out of the comparison.
const UNREAD_FIELDS: &[(&str, &str, &str)] = &[(
    "MdHtml.tagHead",
    "rest",
    "the text after the name; scanAttributes is handed it directly",
)];

/// Answer fields that count something process-wide, so their values depend on run order; each is compared by which values repeat, numbered in first-seen order.
const RENUMBERED: &[(&str, &str, &str)] = &[(
    "MdLeaf.chunkTable",
    "tableKey",
    "a count of the tables chunked so far; every chunk of one call carries the same key",
)];

fn renumber(name: &str, value: Json) -> Json {
    match RENUMBERED.iter().find(|(helper, _, _)| *helper == name) {
        Some((_, key, _)) => renumber_in(value, key, &mut Vec::new()),
        None => value,
    }
}

fn renumber_in(value: Json, key: &str, seen: &mut Vec<String>) -> Json {
    match value {
        Json::Arr(items) => Json::Arr(
            items
                .into_iter()
                .map(|item| renumber_in(item, key, seen))
                .collect(),
        ),
        Json::Obj(pairs) => Json::Obj(
            pairs
                .into_iter()
                .map(|(field, item)| {
                    if field != key {
                        return (field, renumber_in(item, key, seen));
                    }
                    let text = json::canon(&item);
                    let at = seen.iter().position(|known| *known == text);
                    let at = at.unwrap_or_else(|| {
                        seen.push(text);
                        seen.len() - 1
                    });
                    (field, Json::Num(at.to_string()))
                })
                .collect(),
        ),
        other => other,
    }
}

fn without_unread(name: &str, recorded: &Json) -> Json {
    match recorded {
        Json::Obj(pairs) => Json::Obj(
            pairs
                .iter()
                .filter(|(key, _)| {
                    !UNREAD_FIELDS
                        .iter()
                        .any(|(helper, field, _)| *helper == name && field == key)
                })
                .cloned()
                .collect(),
        ),
        other => other.clone(),
    }
}

// A record's interned values put back: {"$l": [ids]} a list of shared lines, {"$s": id} a shared value.
fn resolve(value: Json, lines: &[Json], table: &[Json]) -> Json {
    match value {
        Json::Obj(pairs) if pairs.len() == 1 && pairs[0].0 == "$s" => {
            table[json::u(&pairs[0].1)].clone()
        }
        Json::Obj(pairs) if pairs.len() == 1 && pairs[0].0 == "$l" => Json::Arr(
            json::list(&pairs[0].1)
                .iter()
                .map(|id| lines[json::u(id)].clone())
                .collect(),
        ),
        Json::Arr(items) => Json::Arr(
            items
                .into_iter()
                .map(|item| resolve(item, lines, table))
                .collect(),
        ),
        Json::Obj(pairs) => Json::Obj(
            pairs
                .into_iter()
                .map(|(key, item)| (key, resolve(item, lines, table)))
                .collect(),
        ),
        other => other,
    }
}

// The written answer matches the recorded one, or its digest when the record kept only that.
fn matches(name: &str, written: &str, recorded: &Json) -> bool {
    if let Some(want) = recorded.get("$fnv").and_then(Json::as_str) {
        return parse(written)
            .is_ok_and(|ours| super::oracle_tests::digest(&json::canon(&ours)) == want);
    }
    parse(written)
        .is_ok_and(|ours| json::same(&renumber(name, ours), &renumber(name, recorded.clone())))
}

// Where written and recorded arguments part, both shown from there in canonical form.
fn first_difference(written: &str, recorded: &Json) -> String {
    let ours = parse(written).map_or_else(|_| written.to_string(), |value| json::canon(&value));
    let theirs = json::canon(recorded);
    let at = ours
        .char_indices()
        .zip(theirs.chars())
        .find(|((_, a), b)| a != b)
        .map_or(ours.len().min(theirs.len()), |((at, _), _)| at);
    let from = |text: &str| {
        text.get(at..)
            .unwrap_or("")
            .chars()
            .take(160)
            .collect::<String>()
    };
    format!(
        "\n  after wrote ..{}\n  after wanted ..{}",
        from(&ours),
        from(&theirs)
    )
}

fn call(name: &str, args: &[Json]) -> Option<Answer> {
    text::call(name, args)
        .or_else(|| lines::call(name, args))
        .or_else(|| state::call(name, args))
        .or_else(|| html::call(name, args))
}

#[test]
fn every_recorded_helper_call_answers_as_the_javascript_helper_did() {
    let fixture = include_str!("../../tests/fixtures/markdown-oracle/calls.jsonl");
    let (mut lines, mut table) = (Vec::new(), Vec::new());
    let mut replayed: BTreeMap<String, usize> = BTreeMap::new();
    let mut skipped: BTreeMap<String, usize> = BTreeMap::new();
    let mut bad = Vec::new();
    for (n, line) in fixture.lines().enumerate() {
        let Ok(Json::Arr(fields)) = parse(line) else {
            panic!("record {n} is a JSON array");
        };
        match fields.first().and_then(Json::as_str) {
            Some("$t") => {
                lines.push(fields[2].clone());
                continue;
            }
            Some("$s") => {
                let shared = resolve(fields[2].clone(), &lines, &table);
                table.push(shared);
                continue;
            }
            _ => {}
        }
        assert_eq!(
            fields.len(),
            4,
            "record {n} holds name, arguments, answer and after"
        );
        let name = fields[0].as_str().unwrap_or("").to_string();
        let args = resolve(fields[1].clone(), &lines, &table);
        let Some(answer) = call(&name, json::list(&args)) else {
            *skipped.entry(name).or_default() += 1;
            continue;
        };
        *replayed.entry(name.clone()).or_default() += 1;
        let result_ok = matches(&name, &answer.result, &without_unread(&name, &fields[2]));
        let after_ok = match (&fields[3], &answer.after) {
            (Json::Null, _) => true,
            (recorded, Some(after)) => matches(&name, after, recorded),
            (_, None) => false,
        };
        if !result_ok || !after_ok {
            let shown: String = json::canon(&args).chars().take(300).collect();
            let wrote: String = answer.result.chars().take(300).collect();
            let wanted: String = json::canon(&fields[2]).chars().take(300).collect();
            let moved = match (&fields[3], &answer.after) {
                (recorded, Some(after)) if !after_ok => first_difference(after, recorded),
                _ => String::new(),
            };
            bad.push(format!(
                "{name} {shown}\n  wrote {wrote}\n  wanted {wanted}\n  after ok {after_ok}{moved}"
            ));
        }
    }
    let unexplained: Vec<_> = skipped
        .keys()
        .filter(|name| NOT_REPLAYED.iter().all(|(listed, _)| listed != name))
        .collect();
    assert!(
        unexplained.is_empty(),
        "helpers with no replay and no reason: {unexplained:?}"
    );
    let stale: Vec<_> = NOT_REPLAYED
        .iter()
        .filter(|(listed, _)| replayed.contains_key(*listed))
        .collect();
    assert!(
        stale.is_empty(),
        "listed as not replayed, yet replayed: {stale:?}"
    );
    let total: usize = replayed.values().sum();
    assert!(total > 3000, "only {total} calls replayed");
    assert!(
        bad.is_empty(),
        "{} of {total} differ:\n{}",
        bad.len(),
        bad.join("\n")
    );
}
