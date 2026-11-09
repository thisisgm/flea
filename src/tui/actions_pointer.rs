//! Pointer hit testing and gestures for the terminal listing.
use super::super::input::{WHEEL_DOWN, WHEEL_UP};
use super::{act, navigate, tab, Key, Map, Model, Wire};
use std::io;

pub(super) fn pointer_key(
    m: &mut Model,
    key: &Key,
    pointer: &super::super::input::Pointer,
    map: &Map,
    w: &mut Wire,
) -> io::Result<()> {
    if pointer.released {
        m.drag_anchor = None;
        return Ok(());
    }
    let scroll = match pointer.button {
        WHEEL_UP => -1,
        WHEEL_DOWN => 1,
        _ => 0,
    };
    if m.editor.is_some() {
        return Ok(());
    }
    if pointer.motion && pointer.button == 3 {
        return Ok(());
    }
    let (left, middle, _) = super::super::render::panes(m.columns, m.preview_visible);
    if !m.quicklook && pointer.y == 1 && pointer.button == 0 && !pointer.motion {
        let mut x = 2;
        for (i, label) in super::super::render::tab_layout(m, m.columns.saturating_sub(2)) {
            let width = super::super::render::text_width(&label);
            if pointer.x >= x && pointer.x < x + width {
                return tab(m, i, w);
            }
            x += width;
        }
        return Ok(());
    }
    if super::super::contents::holding(m) {
        return Ok(());
    }
    if pointer.y < super::super::render::BODY_ROW
        || pointer.y >= m.height + super::super::render::BODY_ROW
    {
        return Ok(());
    }
    let in_preview = m.preview_visible && pointer.x > left + middle + 2;
    // An empty preview column takes no press, so the listing keeps its keys.
    if in_preview && !m.quicklook && !super::super::layout::preview_focusable(m) {
        return Ok(());
    }
    if m.quicklook || in_preview {
        m.preview_focus = true;
        if scroll != 0 {
            return super::key(
                m,
                &Key::named(if scroll < 0 { "Up" } else { "Down" }, ""),
                map,
                w,
            );
        }
        if super::super::look::press(m, pointer) {
            return Ok(());
        }
        let Some((row, start, columns)) = super::super::picture::control_row(m) else {
            return Ok(());
        };
        if pointer.button == 0
            && pointer.y == row
            && pointer.x > start
            && pointer.x <= start + columns
        {
            let cell = pointer.x - start - 1;
            if let Some(pdf) = &mut m.pdf {
                if let Some((control, activate)) = pdf.control_at(cell, m.quicklook, columns) {
                    pdf.control = control;
                    if activate && !pointer.motion {
                        return super::key(m, &Key::named("Return", ""), map, w);
                    }
                }
            } else if let Some(player) = &mut m.player {
                if cell < player.play_width().min(columns) && !pointer.motion {
                    player.control = 0;
                    return player.toggle();
                }
                player.control = 1;
                return player.seek_at(cell, columns);
            }
        }
        return Ok(());
    }
    if m.pending.is_some() || m.held || m.name_filter.waiting {
        return Ok(());
    }
    if scroll != 0 {
        m.preview_focus = false;
        return m.move_by(scroll, false, w);
    }
    if pointer.x <= left {
        if pointer.button == 0 && !pointer.motion {
            if let Some(row) = m.parents.get(pointer.y - super::super::render::BODY_ROW) {
                let path = m
                    .path
                    .parent()
                    .unwrap_or(&m.path)
                    .join(flea::bytename::text_path(&row.name));
                if row.directory {
                    navigate(m, path, w)?;
                }
            }
        }
        return Ok(());
    }
    if pointer.x == left + 1 || pointer.x > left + middle + 1 {
        return Ok(());
    }
    let offset =
        (pointer.y - super::super::render::BODY_ROW) / super::super::contents::row_height(m);
    let index = if m.filter.is_empty() {
        m.top + offset
    } else {
        m.visible().get(offset).copied().unwrap_or(usize::MAX)
    };
    if !m.rows.contains_key(&index) || !super::super::contents::operable(m, index) {
        return Ok(());
    }
    m.preview_focus = false;
    // A click or drag on a row is the operator choosing the cursor, as a cursor key is.
    m.sort_anchor = None;
    if pointer.motion {
        if let Some(anchor) = m.drag_anchor {
            m.selected = if m.filter.is_empty() {
                (anchor.min(index)..=anchor.max(index)).collect()
            } else {
                m.shown()
                    .into_iter()
                    .filter(|i| *i >= anchor.min(index) && *i <= anchor.max(index))
                    .collect()
            };
            m.cursor = index;
            m.remember_selection();
        }
        return Ok(());
    }
    if pointer.button == 2 {
        m.cursor = index;
        if !m.selected.contains(&index) {
            m.selected.clear();
        }
        return act(m, "menu", w);
    }
    if pointer.button != 0 {
        return Ok(());
    }
    let old = m.cursor;
    m.cursor = index;
    if key.mods == "ctrl" || key.mods == "super" {
        if !m.selected.remove(&index) {
            m.selected.insert(index);
        }
    } else if key.mods == "shift" {
        m.selected = if m.filter.is_empty() {
            (old.min(index)..=old.max(index)).collect()
        } else {
            m.shown()
                .into_iter()
                .filter(|i| *i >= old.min(index) && *i <= old.max(index))
                .collect()
        };
    } else if key.mods.is_empty() {
        m.selected.clear();
        m.drag_anchor = Some(index);
        if let Some(path) = m.current_path() {
            const DOUBLE_CLICK: std::time::Duration = std::time::Duration::from_millis(400);
            if m.last_click
                .as_ref()
                .is_some_and(|(previous, at)| previous == &path && at.elapsed() <= DOUBLE_CLICK)
            {
                m.last_click = None;
                return act(
                    m,
                    if m.search.is_empty() {
                        "open"
                    } else {
                        "reveal"
                    },
                    w,
                );
            }
            m.last_click = Some((path, std::time::Instant::now()));
        }
    }
    m.remember_selection();
    Ok(())
}
