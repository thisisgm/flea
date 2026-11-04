//! Print focus, fields, destination choices and cancellation use the card grammar.
use super::card::{geometry, no_printer};
use super::*;
use crate::tui::input::{Key, WHEEL_DOWN, WHEEL_UP};

fn focuses(f: &Form) -> Vec<usize> {
    if no_printer(f) {
        return vec![5, 6];
    }
    let mut result = vec![5, 0, 1];
    if f.paged {
        result.push(2);
    }
    if f.caps.duplex {
        result.push(3);
    }
    if f.caps.color {
        result.push(4);
    }
    result.push(6);
    result
}
fn change_printer(m: &mut Model, w: &mut Wire, delta: isize) -> io::Result<()> {
    let f = m.printing.card.as_mut().unwrap();
    if f.destinations.is_empty() {
        return Ok(());
    }
    let current = f
        .destinations
        .iter()
        .position(|d| d.name == f.printer)
        .unwrap_or(0);
    let next = (current as isize + delta).rem_euclid(f.destinations.len() as isize) as usize;
    let printer = f.destinations[next].name.clone();
    query(m, w, &printer)
}
pub fn key(m: &mut Model, key: &Key, w: &mut Wire) -> io::Result<()> {
    if key.name == "Escape" {
        if m.printing.card.as_ref().is_some_and(|f| f.dropdown) {
            m.printing.card.as_mut().unwrap().dropdown = false;
        } else {
            cancel(m, w)?;
        }
        return Ok(());
    }
    let card = geometry(m);
    let column = super::card::field_column(m, crate::tui::render::cards::room(m.columns));
    let rtl = crate::tui::render::cards::rtl(m);
    let f = m.printing.card.as_mut().unwrap();
    if let Some(p) = &key.pointer {
        if p.released || !card.contains(p.x, p.y) {
            return Ok(());
        }
        if matches!(p.button, WHEEL_UP | WHEEL_DOWN) {
            if f.dropdown && !f.destinations.is_empty() {
                let n = f.destinations.len();
                f.choice = (f.choice + if p.button == WHEEL_UP { n - 1 } else { 1 }) % n;
            } else {
                f.scroll = if p.button == WHEEL_UP {
                    f.scroll.saturating_sub(1)
                } else {
                    f.scroll.saturating_add(1)
                };
            }
            return Ok(());
        }
        if p.motion {
            return Ok(());
        }
        if p.button != 0 {
            return Ok(());
        }
        if let Some(button) = card.button(p.x, p.y) {
            f.focus = 5 + button;
            if !p.motion {
                return activate(m, w);
            }
        } else {
            let row = p.y.saturating_sub(card.y + 4) + card.top;
            if f.dropdown && p.y >= card.y + 6 {
                let shown = f.destinations.len().min(card.shown.saturating_sub(2));
                let top = f.choice.saturating_sub(shown.saturating_sub(1));
                let index = p.y - card.y - 6 + top;
                if index < f.destinations.len() && index < top + shown {
                    f.choice = index;
                    if !p.motion {
                        return choose(m, w);
                    }
                }
                return Ok(());
            }
            let logical = if rtl {
                card.left() + card.content() - p.x.saturating_sub(card.left()) + 1
            } else {
                p.x
            };
            let right = logical > card.left() + column;
            let focus = match (row, right) {
                (1, _) => Some(0),
                (4, false) => Some(1),
                (4, true) if f.paged => Some(2),
                (6, false) if f.caps.duplex => Some(3),
                (6, true) if f.caps.color => Some(4),
                _ => None,
            };
            if let Some(focus) = focus {
                f.focus = focus;
                f.edit_fresh = true;
                if !p.motion && matches!(focus, 0 | 3 | 4) {
                    return activate(m, w);
                }
            }
        }
        return Ok(());
    }
    if f.dropdown {
        match key.name.as_str() {
            "Up" | "Down" => {
                let n = f.destinations.len();
                if n > 0 {
                    f.choice = (f.choice + if key.name == "Up" { n - 1 } else { 1 }) % n;
                }
            }
            "Return" | "Enter" | "Space" => return choose(m, w),
            _ => {}
        }
        return Ok(());
    }
    if key.name == "Tab" {
        let order = focuses(f);
        let current = order
            .iter()
            .position(|v| *v == f.focus)
            .unwrap_or(order.len() - 1);
        f.focus = order[(current
            + if key.mods == "shift" {
                order.len() - 1
            } else {
                1
            })
            % order.len()];
        f.edit_fresh = true;
    } else if matches!(key.name.as_str(), "Return" | "Enter" | "Space")
        && !(f.focus == 2 && key.name == "Space")
    {
        return activate(m, w);
    } else if f.focus == 0 && matches!(key.name.as_str(), "Left" | "Right" | "Up" | "Down") {
        return change_printer(
            m,
            w,
            if matches!(key.name.as_str(), "Left" | "Up") {
                -1
            } else {
                1
            },
        );
    } else if f.focus == 1 || f.focus == 2 {
        let copies = f.focus == 1;
        let value = if copies { &mut f.copies } else { &mut f.pages };
        if key.name == "Backspace" {
            value.pop();
            f.edit_fresh = false;
        } else if key.mods == "ctrl" && matches!(key.name.as_str(), "a" | "A" | "u" | "U") {
            value.clear();
            f.edit_fresh = false;
        } else if copies && matches!(key.name.as_str(), "Up" | "Down") {
            let n = value.parse::<usize>().unwrap_or(1);
            *value = if key.name == "Up" {
                (n + 1).min(printing::MAX_COPIES)
            } else {
                n.saturating_sub(1).max(printing::MIN_COPIES)
            }
            .to_string();
        } else if key.mods.is_empty()
            && !key.text.is_empty()
            && key
                .text
                .chars()
                .all(|c| c.is_ascii_digit() || (!copies && matches!(c, '-' | ',' | ' ')))
        {
            if f.edit_fresh {
                value.clear();
                f.edit_fresh = false;
            }
            if value.len() + key.text.len()
                <= if copies {
                    printing::MAX_COPIES.to_string().len()
                } else {
                    MAX_PAGES_BYTES
                }
            {
                value.push_str(&key.text);
            }
        }
        if f.queried {
            f.error.clear();
        }
    }
    Ok(())
}
fn choose(m: &mut Model, w: &mut Wire) -> io::Result<()> {
    let f = m.printing.card.as_mut().unwrap();
    f.dropdown = false;
    let Some(printer) = f.destinations.get(f.choice).map(|d| d.name.clone()) else {
        return Ok(());
    };
    query(m, w, &printer)
}
fn activate(m: &mut Model, w: &mut Wire) -> io::Result<()> {
    let f = m.printing.card.as_mut().unwrap();
    match f.focus {
        0 if f.queried && !f.destinations.is_empty() => {
            f.dropdown = !f.dropdown;
        }
        3 if f.caps.duplex && f.query.is_none() => f.two_sided = !f.two_sided,
        4 if f.caps.color && f.query.is_none() => f.grayscale = !f.grayscale,
        5 => {
            cancel(m, w)?;
        }
        6 => submit(m, w)?,
        _ => {}
    }
    Ok(())
}
