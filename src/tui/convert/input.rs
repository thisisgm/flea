//! The format list, option, range and bracket buttons keep one keyboard focus.
use super::*;
use crate::tui::input::{Key, WHEEL_DOWN, WHEEL_UP};

pub fn key(m: &mut Model, key: &Key, w: &mut Wire) -> io::Result<()> {
    if key.name == "Escape" {
        cancel(m, w)?;
        return Ok(());
    }
    let card = card::geometry(m);
    let f = m.converting.card.as_mut().unwrap();
    if let Some(p) = &key.pointer {
        if p.released || !card.contains(p.x, p.y) {
            return Ok(());
        }
        if matches!(p.button, WHEEL_UP | WHEEL_DOWN) {
            f.scroll = if p.button == WHEEL_UP {
                f.scroll.saturating_sub(1)
            } else {
                f.scroll.saturating_add(1)
            };
            return Ok(());
        }
        if p.motion {
            return Ok(());
        }
        if p.button != 0 && !p.motion {
            return Ok(());
        }
        if let Some(button) = card.button(p.x, p.y) {
            f.focus = button + 2;
            if !p.motion {
                return activate(m, w);
            }
        } else if p.y >= card.y + 4 && p.y < card.y + 4 + card.shown {
            let row = p.y - card.y - 4 + card.top;
            if f.kind != Kind::Office && row < f.kind.formats().len() {
                f.choice = row;
                f.focus = 0;
                f.error.clear();
            } else if f.kind != Kind::Office
                && row == card::option_row(f) + usize::from(f.kind == Kind::Pdf)
                && (f.kind == Kind::Pdf || f.option_enabled())
            {
                f.focus = 1;
                f.edit_fresh = true;
                if !p.motion && f.kind == Kind::Image {
                    return activate(m, w);
                }
            }
        }
        return Ok(());
    }
    if key.name == "Tab" {
        let order = if f.kind == Kind::Office {
            vec![2, 3]
        } else if f.kind == Kind::Pdf || f.option_enabled() {
            vec![0, 1, 2, 3]
        } else {
            vec![0, 2, 3]
        };
        let current = order.iter().position(|v| *v == f.focus).unwrap_or(0);
        f.focus = order[(current
            + if key.mods == "shift" {
                order.len() - 1
            } else {
                1
            })
            % order.len()];
        f.edit_fresh = true;
        let row = if f.focus == 0 {
            f.choice
        } else {
            card::option_row(f) + usize::from(f.kind == Kind::Pdf)
        };
        if f.focus < 2 {
            f.scroll = f.scroll.min(row).max((row + 1).saturating_sub(card.shown));
        }
    } else if matches!(key.name.as_str(), "Return" | "Enter")
        || key.name == "Space" && !(f.kind == Kind::Pdf && f.focus == 1)
    {
        return activate(m, w);
    } else if f.kind != Kind::Office
        && f.focus != 1
        && key.mods.is_empty()
        && (matches!(key.name.as_str(), "Up" | "Down") || matches!(key.text.as_str(), "j" | "k"))
    {
        let up = key.name == "Up" || key.text == "k";
        f.choice = if up {
            f.choice.saturating_sub(1)
        } else {
            (f.choice + 1).min(f.kind.formats().len() - 1)
        };
        f.focus = 0;
        f.scroll = f
            .scroll
            .min(f.choice)
            .max((f.choice + 1).saturating_sub(card.shown));
        f.error.clear();
    } else if f.kind == Kind::Pdf && f.focus == 1 {
        if key.name == "Backspace" {
            f.pages.pop();
            f.edit_fresh = false;
        } else if key.mods == "ctrl" && matches!(key.name.as_str(), "a" | "A" | "u" | "U") {
            f.pages.clear();
            f.edit_fresh = false;
        } else if key.mods.is_empty()
            && !key.text.is_empty()
            && key
                .text
                .chars()
                .all(|c| c.is_ascii_digit() || matches!(c, '-' | ',' | ' ' | 'a' | 'A' | 'l' | 'L'))
        {
            if f.edit_fresh {
                f.pages.clear();
                f.edit_fresh = false;
            }
            if f.pages.len() + key.text.len() <= flea::ui_logic_op4::pages::MAX_RANGE_BYTES {
                f.pages.push_str(&key.text);
            }
        }
        f.error.clear();
    }
    Ok(())
}
fn activate(m: &mut Model, w: &mut Wire) -> io::Result<()> {
    let f = m.converting.card.as_mut().unwrap();
    match f.focus {
        1 if f.option_enabled() => {
            if f.combine_option() {
                f.combine = !f.combine;
            } else {
                f.strip = !f.strip;
            }
        }
        2 => {
            cancel(m, w)?;
        }
        _ => submit(m, w)?,
    }
    Ok(())
}
