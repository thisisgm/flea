//! Replays of the line helpers: containers, leaves, definition lines, display math and inline spans.
use super::super::markdown_blocks::{figure_kind, rule_suffix};
use super::super::markdown_container::{
    expand_lead, indentation_at, list_at, quote_at, starts_block, take_indent, take_list,
    take_quote, text_at, unindent, Marker, View, CODE_INDENT,
};
use super::super::markdown_entity::decode_references;
use super::super::markdown_inline::{escape_html_text, span_intervals, SpanKind};
use super::super::markdown_leaf::{
    atx_heading, cell_extent, chunk_table, delim_aligns, fence_open, heading_safe, is_setext,
    is_thematic, setext_level, split_row,
};
use super::super::markdown_math::{
    display_at, front_close_at, inline_sources, split_display, Piece as MathPiece,
};
use super::super::markdown_paragraphs::cut_after;
use super::super::markdown_refs::{
    definition_at, kill_definition, read_definition, read_definition_parts,
    read_footnote_definition,
};
use super::super::oracle_tests::{arr, dump, js, obj};
use super::super::{Block, Table};
use super::json::{after, b, block, byte_of, cell_align_name, field, s, strs, texts, u, unit_of};
use super::Answer;
use crate::jsondoc::Json;

fn view(line: &str, value: &Json) -> View {
    View {
        at: byte_of(line, u(field(value, "at"))).unwrap_or(line.len()),
        padding: u(field(value, "padding")),
        column: u(field(value, "column")),
    }
}

fn view_json(line: &str, view: &View) -> String {
    obj(vec![
        ("at", unit_of(line, view.at).to_string()),
        ("column", view.column.to_string()),
        ("padding", view.padding.to_string()),
    ])
}

fn marker(line: &str, value: &Json) -> Marker {
    Marker {
        indent: u(field(value, "indent")),
        ordered: b(field(value, "ordered")),
        mark: s(field(value, "mark")).bytes().next().unwrap_or(b'-'),
        start: u(field(value, "start")) as u64,
        content_col: u(field(value, "contentCol")),
        marker_end: byte_of(line, u(field(value, "markerEnd"))).unwrap_or(line.len()),
        marker_column: u(field(value, "markerColumn")),
        gap: u(field(value, "gap")),
    }
}

fn marker_json(line: &str, marker: &Marker) -> String {
    obj(vec![
        ("contentCol", marker.content_col.to_string()),
        ("gap", marker.gap.to_string()),
        ("indent", marker.indent.to_string()),
        ("mark", js(&char::from(marker.mark).to_string())),
        ("markerColumn", marker.marker_column.to_string()),
        ("markerEnd", unit_of(line, marker.marker_end).to_string()),
        ("ordered", marker.ordered.to_string()),
        ("start", marker.start.to_string()),
    ])
}

/// Replays one call, or None for a helper this file does not hold.
pub fn call(name: &str, a: &[Json]) -> Option<Answer> {
    let arg = |i: usize| a.get(i).unwrap_or(&Json::Null);
    let line = s(arg(0));
    let nullable = |found: Option<String>| Answer::of(found.unwrap_or_else(|| "null".into()));
    match name {
        "MdLeaf.isThematic" => Answer::of(is_thematic(line).to_string()),
        "MdLeaf.isSetext" => Answer::of(is_setext(line).to_string()),
        "MdLeaf.setextLevel" => Answer::of(setext_level(line).to_string()),
        "MdLeaf.headingSafe" => Answer::of(js(&heading_safe(line))),
        "MdLeaf.splitRow" => Answer::of(strs(&split_row(line))),
        "MdLeaf.atxHeading" => nullable(
            atx_heading(line)
                .map(|(level, text)| obj(vec![("level", level.to_string()), ("text", js(text))])),
        ),
        "MdLeaf.fenceOpen" => nullable(fence_open(line).map(|fence| {
            obj(vec![
                ("indent", fence.indent.to_string()),
                ("info", js(&fence.info)),
                ("len", fence.len.to_string()),
                ("tick", js(&char::from(fence.tick).to_string())),
            ])
        })),
        "MdLeaf.delimAligns" => {
            nullable(delim_aligns(line).map(|aligns| arr(&aligns, cell_align_name)))
        }
        "MdLeaf.cellExtent" => {
            let extent = cell_extent(line);
            Answer::of(obj(vec![
                ("image", extent.image.to_string()),
                ("text", extent.text.to_string()),
                ("word", extent.word.to_string()),
            ]))
        }
        "MdLeaf.chunkTable" => match block(arg(0)) {
            Block::Table(table) => Answer::of(arr(&chunk_table(table), |chunk| {
                let text = dump(chunk);
                match chunk {
                    // The oracle's dump leaves the key out; the call's answer carries it.
                    Block::Table(Table { key: Some(key), .. }) => {
                        format!("{},\"tableKey\":{key}}}", &text[..text.len() - 1])
                    }
                    _ => text,
                }
            })),
            _ => None,
        },
        "MdContainer.textAt" => Answer::of(js(&text_at(line, &view(line, arg(1))))),
        "MdContainer.quoteAt" => Answer::of(
            quote_at(line, &view(line, arg(1))).map_or("-1".to_string(), |i| i.to_string()),
        ),
        "MdContainer.listAt" => {
            nullable(list_at(line, &view(line, arg(1))).map(|found| marker_json(line, &found)))
        }
        "MdContainer.indentationAt" => {
            let limit = arg(2).as_f64().map_or(CODE_INDENT, |x| x as usize);
            let (end, width) = indentation_at(line, &view(line, arg(1)), limit);
            Answer::of(obj(vec![
                ("end", unit_of(line, end).to_string()),
                ("width", width.to_string()),
            ]))
        }
        "MdContainer.takeQuote" | "MdContainer.takeIndent" | "MdContainer.takeList" => {
            let mut at = view(line, arg(1));
            match name {
                "MdContainer.takeQuote" => take_quote(line, &mut at, u(arg(2))),
                "MdContainer.takeIndent" => take_indent(line, &mut at, u(arg(2))),
                _ => take_list(line, &mut at, &marker(line, arg(2))),
            }
            Some(Answer {
                result: "null".into(),
                after: after(a, &[(1, view_json(line, &at))]),
            })
        }
        "MdContainer.unindent" => Answer::of(js(unindent(line, u(arg(1))))),
        "MdContainer.expandLead" => Answer::of(js(&expand_lead(line, u(arg(1))))),
        "MdContainer.startsBlock" => Answer::of(starts_block(line).to_string()),
        "MdRefs.killDefinition" => Answer::of(js(&kill_definition(line))),
        "MdRefs.readDefinition" => nullable(read_definition(line).map(|found| {
            obj(vec![
                ("key", js(&found.key)),
                ("target", js(&found.target)),
                ("titled", found.titled.to_string()),
            ])
        })),
        "MdRefs.readDefinitionTarget" => Answer::of(js(&read_definition_parts(line).0)),
        "MdRefs.readFootnoteDefinition" => nullable(
            read_footnote_definition(line)
                .map(|(id, text)| obj(vec![("id", js(&id)), ("text", js(&text))])),
        ),
        "MdRefs.definitionAt" => {
            let lines = texts(arg(0));
            let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
            nullable(
                definition_at(&refs, u(arg(1))).map(|(key, target, titled, end)| {
                    obj(vec![
                        ("end", end.to_string()),
                        ("key", js(&key)),
                        ("target", js(&target)),
                        ("titled", titled.to_string()),
                    ])
                }),
            )
        }
        "MdBlocks.ruleSuffix" => {
            let (start, count) = rule_suffix(line);
            Answer::of(obj(vec![
                ("count", count.to_string()),
                ("start", unit_of(line, start).to_string()),
            ]))
        }
        "MdBlocks.figureKind" => Answer::of(js(figure_kind(line).map_or("", |kind| kind.name()))),
        "MdFront.closeAt" => {
            let lines = texts(arg(0));
            let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
            Answer::of(front_close_at(&refs).map_or("-1".to_string(), |i| i.to_string()))
        }
        "MdMath.displayAt" => {
            let lines = texts(arg(0));
            let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
            nullable(display_at(&refs, u(arg(1)), s(arg(2))).map(|found| {
                obj(vec![
                    ("source", js(&found.source)),
                    ("tail", js(&found.tail)),
                    ("to", found.to.to_string()),
                ])
            }))
        }
        "MdMath.splitDisplay" => nullable(split_display(line).map(|pieces| {
            arr(&pieces, |piece| match piece {
                MathPiece::Text(text) => obj(vec![("text", js(text))]),
                MathPiece::Math(math) => obj(vec![("math", js(math))]),
            })
        })),
        "MdMath.inlineSources" => Answer::of(strs(&inline_sources(line))),
        "MdParagraphs.cutAfter" => {
            Answer::of(arr(&cut_after(&texts(arg(0))), |cut| cut.to_string()))
        }
        "MdEntity.decodeReferences" => Answer::of(js(&decode_references(line))),
        "MdInline.escapeHtmlText" => Answer::of(js(&escape_html_text(line))),
        "MdInline.spanIntervals" => {
            let mut flat = Vec::new();
            for span in span_intervals(line) {
                let kind = usize::from(span.kind == SpanKind::Math);
                flat.extend([
                    unit_of(line, span.from),
                    unit_of(line, span.to),
                    span.len,
                    kind,
                ]);
            }
            Answer::of(arr(&flat, |x| x.to_string()))
        }
        _ => None,
    }
}
