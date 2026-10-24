//! A raw tag sanitized: an allowlisted tag keeps its safe attributes, a picture becomes an image token, every other tag is escaped.

use super::super::markdown_escape::{escape_text, CHIP_PAD};
use super::super::markdown_inline::link_ink;
use super::super::markdown_text::{is_colour, is_space};
use super::super::markdown_url::{
    canonical_url, classify_image, srcset_pick, stripped_target, target_allowed, Picture,
};
use super::super::Ctx;
use super::{allowed, drops_content, global_attr, hold_token, tag_head, void, DISCLOSURE_OPEN};

// One attribute value, its references decoded for the safety checks below.
pub(in crate::ui_logic_pv2::markdown) fn attr_kept(name: &str, value: &str, tag: &str) -> bool {
    let seen = stripped_target(&canonical_url(value)).to_lowercase();
    // CSS url() in any attribute loads, so the attribute goes.
    if seen
        .match_indices("url")
        .any(|(at, _)| seen[at + 3..].trim_start_matches(is_space).starts_with('('))
    {
        return false;
    }
    match name {
        "href" => tag == "a" && target_allowed(value),
        "src" => tag == "img" || tag == "source",
        "color" => tag == "font",
        _ => global_attr(name),
    }
}

fn chip_open(chrome: &str) -> String {
    if is_colour(chrome) {
        format!("<code style=\"background-color:{chrome}\">{CHIP_PAD}")
    } else {
        "<code>".to_string()
    }
}

fn chip_close(chrome: &str) -> String {
    if is_colour(chrome) {
        format!("{CHIP_PAD}</code>")
    } else {
        "</code>".to_string()
    }
}

fn escape_attr(value: &str) -> String {
    value
        .replace('&', "&#38;")
        .replace('"', "&#34;")
        .replace('<', "&#60;")
}

/// A sanitized tag: the markup it emits and, for a dropped-content tag, the name whose body is skipped.
pub struct Sanitized {
    pub emit: String,
    pub drop: Option<&'static str>,
}

fn emit(text: impl Into<String>) -> Sanitized {
    Sanitized {
        emit: text.into(),
        drop: None,
    }
}

/// Sanitizes one tag; inked counts the open links wearing the ink, so a closer takes off only ink its opener put on.
pub fn sanitize_tag(
    tag: &str,
    ctx: &Ctx,
    tokens: &mut Vec<String>,
    inked: &mut usize,
) -> Sanitized {
    let head = tag_head(tag);
    let name = match head.name.as_str() {
        "del" | "strike" => "s",
        other => other,
    };
    if name.is_empty() {
        return emit("&#60;");
    }
    if let Some(dropped) = drops_content(name) {
        return Sanitized {
            emit: String::new(),
            drop: (!head.closing && !head.self_close).then_some(dropped),
        };
    }
    if !allowed(name) || name == "details" {
        return emit("");
    }
    // A summary is a bold line led by the open disclosure mark, and its body follows.
    if name == "summary" {
        return emit(if head.closing {
            "</b>".to_string()
        } else {
            format!("<b>{DISCLOSURE_OPEN} ")
        });
    }
    // kbd and code are the inline code chip, never a bare tag.
    if name == "kbd" || name == "code" {
        return emit(if head.closing {
            chip_close(ctx.chrome)
        } else {
            chip_open(ctx.chrome)
        });
    }
    let wear = if name == "a" { link_ink(ctx.ink) } else { None };
    if head.closing {
        let worn = wear.is_some() && *inked > 0;
        if worn {
            *inked -= 1;
        }
        let close = if worn { wear.map_or("", |w| w.1) } else { "" };
        return emit(format!("{close}</{name}>"));
    }
    if !head.valid_attrs {
        return emit("");
    }
    let mut kept = String::new();
    let mut src_seen: Option<String> = None;
    let mut srcset_seen: Option<String> = None;
    let mut alt_seen: Option<String> = None;
    for attr in &head.attributes {
        let aname = attr.name.as_str();
        // Every style, class, id, background, srcset, poster, data-* and on* attribute is dropped, whatever its value.
        if matches!(aname, "style" | "class" | "id" | "background" | "poster")
            || aname.starts_with("data-")
            || aname.starts_with("on")
        {
            continue;
        }
        if aname == "srcset" {
            srcset_seen.get_or_insert_with(|| attr.value.clone().unwrap_or_default());
            continue;
        }
        if aname == "src" && (name == "img" || name == "source") {
            src_seen.get_or_insert_with(|| attr.value.clone().unwrap_or_default());
            continue;
        }
        if aname == "alt" {
            if alt_seen.is_some() {
                continue;
            }
            alt_seen = Some(attr.value.clone().unwrap_or_default());
        }
        let value = match (&attr.value, aname) {
            (Some(v), "href") => Some(stripped_target(v)),
            (v, _) => v.clone(),
        };
        match &value {
            None => {
                kept.push(' ');
                kept.push_str(aname);
            }
            Some(v) if attr_kept(aname, v, name) => {
                kept.push_str(&format!(" {aname}=\"{}\"", escape_attr(v)));
            }
            Some(_) => {}
        }
    }
    if name == "img" || name == "source" {
        let picked = match (&src_seen, &srcset_seen) {
            (Some(src), _) => Some(classify_image(src, ctx.dir)),
            (None, Some(set)) => srcset_pick(set, ctx.dir),
            (None, None) => None,
        };
        let alt = alt_seen.unwrap_or_default();
        return match picked {
            // A local picture reaches the renderer as a Markdown image.
            Some(Picture::Local(url)) => {
                let html = format!(
                    "![{}]({})",
                    escape_text(&alt),
                    url.replace('(', "%28").replace(')', "%29")
                );
                emit(hold_token(tokens, html))
            }
            Some(Picture::Remote(host)) => {
                emit(format!("\n\n{}\n\n", ctx.placeholder(&escape_text(&host))))
            }
            _ => emit(if canonical_url(&alt).is_empty() {
                String::new()
            } else {
                escape_text(&alt)
            }),
        };
    }
    let close = if head.self_close || void(name) {
        " /"
    } else {
        ""
    };
    let wears = wear.is_some() && close.is_empty();
    if wears {
        *inked += 1;
    }
    let open = if wears {
        wear.map_or(String::new(), |w| w.0)
    } else {
        String::new()
    };
    emit(format!("<{name}{kept}{close}>{open}"))
}
