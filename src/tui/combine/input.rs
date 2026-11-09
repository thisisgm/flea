//! Ordered preview navigation, setting changes and SGR drag use the painted targets.
use super::super::input::{Key, WHEEL_DOWN, WHEEL_UP};
use super::*;

pub fn key(m: &mut Model, key: &Key, w: &mut Wire) -> io::Result<()> {
    if key.name == "Escape" {
        cancel(m, w)?;
        return Ok(());
    }
    let g = geometry(m);
    let rtl = crate::tui::render::cards::rtl(m);
    let f = m.combining.card.as_mut().unwrap();
    if let Some(p) = &key.pointer {
        if p.button != 0 || matches!(p.button, WHEEL_UP | WHEEL_DOWN) {
            return Ok(());
        }
        if p.released {
            if let Some((from, x, y)) = f.drag.take().filter(|(_, x, y)| (*x, *y) != (p.x, p.y)) {
                let _ = (x, y);
                if let Some(to) = g.tiles.iter().rposition(|b| b.hit(p.x, p.y)) {
                    f.reorder(from, to);
                }
            }
            return Ok(());
        }
        if let Some(index) = g.tiles.iter().rposition(|b| b.hit(p.x, p.y)) {
            if !p.motion {
                f.drag = Some((index, p.x, p.y));
                f.image = index;
                f.focus = 0;
                f.changed();
            }
        } else if !p.motion {
            f.drag = None;
            if let Some(index) = g.card.button(p.x, p.y) {
                f.focus = index + 7;
                return activate(m, w);
            }
            if let Some(field) = g
                .fields
                .iter()
                .find(|field| field.enabled && field.rect.hit(p.x, p.y))
            {
                f.focus = field.focus;
                f.fresh = true;
                setting(f, field.choice);
            }
        }
        return Ok(());
    }
    f.drag = None;
    if matches!(key.name.as_str(), "Tab" | "Backtab") && matches!(key.mods.as_str(), "" | "shift") {
        let mut order = vec![0, 1];
        if f.settings.layout == Layout::Grid {
            order.push(2);
        }
        order.extend([3, 4]);
        if f.background_enabled() {
            order.push(5);
        }
        order.extend([6, 7, 8]);
        let i = order.iter().position(|i| *i == f.focus).unwrap_or(0);
        f.focus = order[(i + if key.mods == "shift" || key.name == "Backtab" {
            order.len() - 1
        } else {
            1
        }) % order.len()];
        f.fresh = true;
        f.changed();
    } else if matches!(key.name.as_str(), "Return" | "Enter")
        || key.name == "Space" && key.mods.is_empty() && f.focus != 6
    {
        return activate(m, w);
    } else if f.focus == 6 || f.focus == 2 {
        if !g.fields.iter().any(|field| field.focus == f.focus) {
            return Ok(());
        }
        let value = if f.focus == 6 {
            &mut f.name
        } else {
            &mut f.columns
        };
        let mut edit = false;
        if key.name == "Backspace" {
            value.pop();
            edit = true;
        } else if key.mods == "ctrl" && matches!(key.name.as_str(), "a" | "A" | "u" | "U") {
            value.clear();
            edit = true;
        } else if key.mods.is_empty()
            && !key.text.is_empty()
            && !key.text.chars().any(char::is_control)
            && (f.focus == 6 || key.text.bytes().all(|b| b.is_ascii_digit()))
            && value.len() + key.text.len() <= 255
        {
            if f.fresh {
                value.clear();
            }
            value.push_str(&key.text);
            edit = true;
        }
        if edit {
            f.fresh = false;
            if f.focus == 6 {
                f.custom_name = true;
            } else {
                f.settings.columns = f.columns.parse().unwrap_or(0);
            }
            f.changed();
        }
    } else if key.mods.is_empty() {
        let horizontal = if key.name == "Left" || key.text == "h" {
            -1
        } else if key.name == "Right" || key.text == "l" {
            1
        } else {
            0
        };
        let horizontal = if rtl { -horizontal } else { horizontal };
        let vertical = if key.name == "Up" || key.text == "k" {
            -1
        } else if key.name == "Down" || key.text == "j" {
            1
        } else {
            0
        };
        if matches!(key.text.as_str(), "[" | "]") && !f.paths.is_empty() {
            let to = f
                .image
                .saturating_add_signed(if key.text == "[" { -1 } else { 1 })
                .min(f.paths.len() - 1);
            f.reorder(f.image, to);
        } else if f.focus == 0 || f.focus >= 7 {
            let columns = match f.settings.layout {
                Layout::Row => f.paths.len(),
                Layout::Column => 1,
                Layout::Grid => f.settings.columns.max(1),
            };
            let target = match (horizontal, vertical) {
                (-1, 0) => f.image.checked_sub(1),
                (1, 0) => f.image.checked_add(1),
                (0, -1) => f.image.checked_sub(columns),
                (0, 1) => f.image.checked_add(columns),
                _ => None,
            };
            if let Some(target) = target.filter(|target| {
                *target < f.paths.len()
                    && (horizontal == 0 || f.image / columns == *target / columns)
            }) {
                f.image = target;
                f.focus = 0;
                f.changed();
            }
        } else if horizontal != 0 || vertical != 0 {
            let choices = match f.focus {
                1 | 3 | 4 => 3,
                5 if f.format == "jpg" => 2,
                5 => 3,
                _ => 1,
            };
            let current: usize = match f.focus {
                1 => match f.settings.layout {
                    Layout::Row => 0,
                    Layout::Column => 1,
                    Layout::Grid => 2,
                },
                3 => output::COMBINE_FORMATS
                    .iter()
                    .position(|s| *s == f.format)
                    .unwrap_or(0),
                4 => match f.settings.spacing {
                    Spacing::None => 0,
                    Spacing::Small => 1,
                    Spacing::Large => 2,
                },
                5 => match f.settings.background {
                    Background::Transparent => 0,
                    Background::White => usize::from(f.format != "jpg"),
                    Background::Black => {
                        if f.format == "jpg" {
                            1
                        } else {
                            2
                        }
                    }
                },
                _ => 0,
            };
            setting(
                f,
                current
                    .saturating_add_signed(horizontal + vertical)
                    .min(choices - 1),
            );
        }
    }
    Ok(())
}
fn activate(m: &mut Model, w: &mut Wire) -> io::Result<()> {
    let focus = m.combining.card.as_ref().unwrap().focus;
    if focus == 7 {
        cancel(m, w)?;
    } else {
        submit(m, w)?;
    }
    Ok(())
}
fn setting(f: &mut Form, choice: usize) {
    match f.focus {
        1 => f.settings.layout = [Layout::Row, Layout::Column, Layout::Grid][choice],
        3 => {
            f.format = output::COMBINE_FORMATS[choice].into();
            if f.format == "jpg" && f.settings.background == Background::Transparent {
                f.settings.background = Background::White;
            }
            f.rename_default();
        }
        4 => f.settings.spacing = [Spacing::None, Spacing::Small, Spacing::Large][choice],
        5 if f.background_enabled() => {
            f.settings.background = if f.format == "jpg" {
                [Background::White, Background::Black][choice]
            } else {
                [
                    Background::Transparent,
                    Background::White,
                    Background::Black,
                ][choice]
            };
        }
        _ => return,
    }
    f.changed();
}
