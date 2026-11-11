//! Replays of the URL, link, prepared-document, table-fit and document-level helpers.
use super::super::markdown_escape::CHIP_PAD;
use super::super::markdown_event::LineKind;
use super::super::markdown_fit::table_fit;
use super::super::markdown_link::{read_barelink, read_inline_target};
use super::super::markdown_prepared::{
    formulas_in, hex_of, is_markdown_row, path_of_url, picture_urls, reads_inline, small_pictures,
    Row,
};
use super::super::markdown_url::{
    canonical_url, classify_image, dir_of, host_of, is_external_link, is_remote_url,
    target_allowed, Picture,
};
use super::super::oracle_tests::{arr, dump, js, obj};
use super::super::{
    deep_blocks, deep_head, figures_in, line_count, Block, Ctx, Words, NESTING_LIMIT,
};
use super::json::{b, blocks, byte_of, canon, f, field, list, num, s, strs, texts, u, unit_of};
use super::Answer;
use crate::jsondoc::Json;

fn row(value: &Json) -> Option<Row<'_>> {
    value.as_object().map(|_| Row {
        name: s(field(value, "n")),
        dir: b(field(value, "d")),
        mode: f(field(value, "p")) as u32,
        size: f(field(value, "s")) as u64,
    })
}

fn picture(found: Picture) -> String {
    match found {
        Picture::Local(url) => obj(vec![("kind", js("local")), ("url", js(&url))]),
        Picture::Remote(host) => obj(vec![("kind", js("remote")), ("host", js(&host))]),
        Picture::Dropped => obj(vec![("kind", js("dropped"))]),
    }
}

// The line kinds by the names the JavaScript parser gave them.
fn line_kind(name: &str) -> Option<LineKind> {
    Some(match name {
        "run" => LineKind::Run,
        "setext" => LineKind::Setext,
        "hidden" => LineKind::Hidden,
        "fenceOpen" => LineKind::FenceOpen,
        "fenceBody" => LineKind::FenceBody,
        "fenceClose" => LineKind::FenceClose,
        "codeLine" => LineKind::CodeLine,
        "codeEnd" => LineKind::CodeEnd,
        _ => return None,
    })
}

// The notice as the pane fills it: the English catalog string with the nesting limit.
fn deep_notice() -> Option<String> {
    let english = crate::jsondoc::parse(include_str!("../../i18n/en/flea.json")).ok()?;
    let notice = english.get("document.markdown_too_deep")?.as_str()?;
    Some(crate::i18n::substitute(
        notice,
        &[&NESTING_LIMIT.to_string()],
    ))
}

fn flag(value: bool) -> Option<Answer> {
    Answer::of(value.to_string())
}

fn text(value: &str) -> Option<Answer> {
    Answer::of(js(value))
}

/// Replays one call, or None for a helper this file does not hold.
pub fn call(name: &str, a: &[Json]) -> Option<Answer> {
    let arg = |i: usize| a.get(i).unwrap_or(&Json::Null);
    match name {
        "MdUrl.isRemoteUrl" => flag(is_remote_url(s(arg(0)))),
        "MdUrl.hostOf" => text(&host_of(s(arg(0)))),
        "MdUrl.canonicalUrl" => text(&canonical_url(s(arg(0)))),
        "MdUrl.targetAllowed" => flag(target_allowed(s(arg(0)))),
        "MdUrl.classifyImage" => Answer::of(picture(classify_image(s(arg(0)), s(arg(1))))),
        "Markdown.isExternalLink" => flag(is_external_link(s(arg(0)))),
        "Markdown.dirOf" => text(dir_of(s(arg(0)))),
        "Markdown.placeholder" => {
            let words = Words::english();
            let ctx = Ctx {
                dir: "",
                chrome: "",
                ink: "",
                words: &words,
            };
            text(&ctx.placeholder(s(arg(0))))
        }
        "Markdown.deepNotice" => text(&deep_notice()?),
        "MdItems.isRaw" => flag(line_kind(s(arg(0))).is_some_and(LineKind::raw)),
        "Markdown.lineCount" => Answer::of(line_count(s(arg(0))).to_string()),
        "Markdown.deepBlocks" => Answer::of(arr(&deep_blocks(), dump)),
        "Markdown.deepHead" => {
            let source = s(arg(0));
            let (deep, scanned) = deep_head(source);
            Answer::of(obj(vec![
                ("deep", deep.to_string()),
                ("scanned", unit_of(source, scanned).to_string()),
            ]))
        }
        "Markdown.figuresIn" => {
            let held = blocks(arg(0));
            let found: Vec<String> = figures_in(&held)
                .into_iter()
                .map(|figure| dump(&Block::Figure(figure.clone())))
                .collect();
            let mut out: Vec<String> = list(arg(1)).iter().map(canon).collect();
            out.extend(found.iter().cloned());
            Some(Answer {
                result: format!("[{}]", found.join(",")),
                after: Some(format!("[{},[{}]]", canon(arg(0)), out.join(","))),
            })
        }
        "MdEscape.chipPad" => text(CHIP_PAD),
        "MdLink.readBarelink" => {
            let line = s(arg(0));
            let found = byte_of(line, u(arg(1))).and_then(|at| read_barelink(line, at));
            Answer::of(found.map_or("null".to_string(), |(url, href, end)| {
                obj(vec![
                    ("end", unit_of(line, end).to_string()),
                    ("href", js(&href)),
                    ("url", js(&url)),
                ])
            }))
        }
        "MdLink.readInlineTarget" => {
            let line = s(arg(0));
            let found = byte_of(line, u(arg(1))).and_then(|at| read_inline_target(line, at));
            Answer::of(found.map_or("null".to_string(), |(url, end)| {
                obj(vec![
                    ("end", unit_of(line, end).to_string()),
                    ("url", js(&url)),
                ])
            }))
        }
        "MarkdownPrepared.pathOfUrl" => text(&path_of_url(s(arg(0)))),
        "MarkdownPrepared.hexOf" => {
            let channel = |key| f(field(arg(0), key)) as f32;
            text(&hex_of(channel("r"), channel("g"), channel("b")))
        }
        "MarkdownPrepared.isMarkdownRow" => flag(is_markdown_row(row(arg(0)).as_ref())),
        "MarkdownPrepared.readsInline" => {
            flag(reads_inline(row(arg(0)).as_ref(), s(arg(1)), b(arg(2))))
        }
        "MarkdownPrepared.smallPictures" => {
            Answer::of(strs(&small_pictures(&texts(arg(0)), s(arg(1)))))
        }
        "MarkdownPrepared.pictureUrls" => {
            Answer::of(strs(&picture_urls(&blocks(arg(0)), u(arg(1)), s(arg(2)))))
        }
        "MarkdownPrepared.formulasIn" => {
            let found = formulas_in(&blocks(arg(0)), u(arg(1)));
            Answer::of(arr(&found, |formula| {
                obj(vec![
                    ("display", formula.display.to_string()),
                    ("source", js(&formula.source)),
                ])
            }))
        }
        "MarkdownTableFit.fit" => {
            let numbers = |value: &Json| list(value).iter().map(f).collect::<Vec<f64>>();
            let widths = table_fit(&numbers(arg(0)), &numbers(arg(1)), f(arg(2)), f(arg(3)));
            Answer::of(arr(&widths, |width| num(*width)))
        }
        _ => None,
    }
}
