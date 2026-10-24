//! Replays of the HTML helpers: tag heads and attributes, the sanitizer, HTML headings and pictures, and the inline pass.
use super::super::markdown_chunks::chunk_list;
use super::super::markdown_html::{
    attr_kept, document_text, sanitize_tag, scan_attributes, tag_head, Attribute, Dead,
};
use super::super::markdown_html_block::{nest_depth, separate_blocks, split_html_images, Part};
use super::super::markdown_html_heading::{heading_unit, html_heading, HtmlHeading};
use super::super::markdown_html_image::{image_unit, picture_size};
use super::super::markdown_leaf::standalone_image;
use super::super::markdown_refs::skip_drop_content;
use super::super::markdown_run::{parse_inline, Env, Shape};
use super::super::oracle_tests::{arr, dump, js, obj};
use super::super::{Align, Block, Ctx, Words};
use super::json::{
    after, b, block, byte_of, f, field, map_json, num, number_map, s, string_map, strs, texts, u,
    unit_of,
};
use super::Answer;
use crate::jsondoc::Json;

fn attributes(found: &[Attribute]) -> String {
    arr(found, |attribute| {
        let mut fields = vec![("name", js(&attribute.name))];
        fields.push((
            "value",
            attribute.value.as_deref().map_or("null".into(), js),
        ));
        obj(fields)
    })
}

fn heading(head: &HtmlHeading) -> String {
    let align = match head.align {
        Some(Align::Center) => js("center"),
        Some(Align::Right) => js("right"),
        None => "null".into(),
    };
    obj(vec![
        ("align", align),
        ("aligned", head.aligned.to_string()),
        ("inner", js(&head.inner)),
        ("level", head.level.to_string()),
    ])
}

fn ctx_of<'w>(dir: &'w str, chrome: &'w str, ink: &'w str, words: &'w Words) -> Ctx<'w> {
    Ctx {
        dir,
        chrome,
        ink,
        words,
    }
}

// The scan cache in JavaScript's shape: -1 for no dead tag, offsets in UTF-16 units.
fn dead_of(body: &str, value: &Json) -> Dead {
    let tag_dead = f(field(value, "tagDead"));
    Dead {
        tag_dead: if tag_dead < 0.0 {
            0
        } else {
            byte_of(body, tag_dead as usize).unwrap_or(0)
        },
        comment_dead: b(field(value, "commentDead")),
        tag_close: value
            .get("tagClose")
            .and_then(Json::as_f64)
            .and_then(|at| byte_of(body, at as usize)),
    }
}

fn dead_json(body: &str, before: &Json, dead: &Dead) -> String {
    let unchanged = dead_of(body, before).tag_dead == dead.tag_dead;
    let tag_dead = match before.get("tagDead") {
        Some(kept) if unchanged => super::json::canon(kept),
        _ => unit_of(body, dead.tag_dead).to_string(),
    };
    let mut fields = vec![("tagDead", tag_dead)];
    if dead.comment_dead || before.get("commentDead").is_some() {
        fields.push(("commentDead", dead.comment_dead.to_string()));
    }
    if let Some(close) = dead.tag_close {
        fields.push(("tagClose", unit_of(body, close).to_string()));
    }
    obj(fields)
}

/// Replays one call, or None for a helper this file does not hold.
pub fn call(name: &str, a: &[Json]) -> Option<Answer> {
    let arg = |i: usize| a.get(i).unwrap_or(&Json::Null);
    let text = s(arg(0));
    let nullable = |found: Option<String>| Answer::of(found.unwrap_or_else(|| "null".into()));
    let words = Words::english();
    match name {
        "MdHtml.tagHead" => {
            let head = tag_head(text);
            Answer::of(obj(vec![
                ("attributes", attributes(&head.attributes)),
                ("closing", head.closing.to_string()),
                ("name", js(&head.name)),
                ("selfClose", head.self_close.to_string()),
                ("validAttrs", head.valid_attrs.to_string()),
            ]))
        }
        "MdHtml.scanAttributes" => {
            let (found, valid, self_close) = scan_attributes(text);
            Answer::of(obj(vec![
                ("attributes", attributes(&found)),
                ("selfClose", self_close.to_string()),
                ("valid", valid.to_string()),
            ]))
        }
        "MdHtml.attrKept" => Answer::of(attr_kept(text, s(arg(1)), s(arg(2))).to_string()),
        "MdHtml.documentText" => Answer::of(js(&document_text(text))),
        "MdHtml.sanitizeTag" => {
            let ctx = ctx_of(s(arg(1)), s(arg(3)), s(arg(4)), &words);
            let mut tokens = texts(arg(2));
            let mut inked = u(field(arg(5), "open"));
            let done = sanitize_tag(text, &ctx, &mut tokens, &mut inked);
            Some(Answer {
                result: obj(vec![
                    ("drop", done.drop.map_or("null".into(), js)),
                    ("emit", js(&done.emit)),
                ]),
                after: after(
                    a,
                    &[
                        (2, strs(&tokens)),
                        (5, obj(vec![("open", inked.to_string())])),
                    ],
                ),
            })
        }
        "MdRefs.skipDropContent" => {
            let mut dead = dead_of(text, arg(3));
            let from = byte_of(text, u(arg(1))).unwrap_or(text.len());
            let end = skip_drop_content(text, from, s(arg(2)), &mut dead);
            Some(Answer {
                result: unit_of(text, end).to_string(),
                after: after(a, &[(3, dead_json(text, arg(3), &dead))]),
            })
        }
        "MdHtmlBlock.htmlHeading" => nullable(html_heading(text).map(|head| heading(&head))),
        "MdHtmlBlock.separateBlocks" => Answer::of(strs(&separate_blocks(&texts(arg(0))))),
        "MdHtmlBlock.splitHtmlImages" => {
            let parts = split_html_images(&texts(arg(0)), s(arg(1)));
            Answer::of(arr(&parts, |part| match part {
                Part::Lines(lines) => obj(vec![("lines", strs(lines))]),
                Part::Block(found) => obj(vec![("block", dump(found))]),
                Part::RowBreak => obj(vec![("rowBreak", "true".into())]),
            }))
        }
        "MdHtmlHeading.headingUnit" => {
            nullable(heading_unit(&texts(arg(0)), u(arg(1))).map(|unit| {
                obj(vec![
                    ("end", unit.end.to_string()),
                    ("head", heading(&unit.head)),
                    ("wrapper", strs(&unit.wrapper)),
                ])
            }))
        }
        "MdHtmlHeading.nestDepth" => Answer::of(nest_depth(text, u(arg(1))).to_string()),
        "MdHtmlImage.imageUnit" => nullable(image_unit(&texts(arg(0)), u(arg(1)), s(arg(2))).map(
            |(found, end, wrapper)| {
                obj(vec![
                    ("block", dump(&found)),
                    ("end", end.to_string()),
                    ("wrapper", strs(&wrapper)),
                ])
            },
        )),
        "MdHtmlImage.pictureSize" => {
            let side = |key| f(field(arg(0), key));
            let (w, h, stretch) = picture_size(
                side("width"),
                side("height"),
                f(arg(1)),
                f(arg(2)),
                f(arg(3)),
            );
            Answer::of(obj(vec![
                ("h", num(h)),
                ("stretch", stretch.to_string()),
                ("w", num(w)),
            ]))
        }
        "MdLeaf.standaloneImage" => {
            let defs = string_map(arg(2));
            nullable(standalone_image(text, s(arg(1)), &defs).map(|found| dump(&found)))
        }
        "MdChunks.chunkList" => match block(arg(0)) {
            Block::List(list) => Answer::of(arr(&chunk_list(list), dump)),
            _ => None,
        },
        "MdRun.parseInline" => {
            let ctx = ctx_of(s(arg(1)), s(arg(4)), s(arg(5)), &words);
            let defs = string_map(arg(2));
            let mut numbers = number_map(arg(3));
            let mut tokens = texts(arg(6));
            let mut cited = texts(arg(7));
            let citing = arg(7).as_array().is_some();
            let shape = Shape {
                literal_plain: b(arg(8)),
                bare_text: b(arg(9)),
                fragment: b(arg(10)),
            };
            let mut env = Env {
                ctx: &ctx,
                defs: &defs,
                numbers: &mut numbers,
                tokens: &mut tokens,
                cited: citing.then_some(&mut cited),
            };
            let out = parse_inline(text, &mut env, shape);
            let mut changed = vec![(6, strs(&tokens))];
            if arg(3).as_object().is_some() {
                changed.push((3, map_json(&numbers, |n| n.to_string())));
            }
            if citing {
                changed.push((7, strs(&cited)));
            }
            Some(Answer {
                result: js(&out),
                after: after(a, &changed),
            })
        }
        _ => None,
    }
}
