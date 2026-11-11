//! One Enter on a typing Find or Filter line commits it and acts on the highlighted row once its answer lands, as the window does.
use crate::tui::{input::Key, model::Model, wire::Wire};
use std::io;

// The committed line's wait: a filter waits for its answer, a find for its own generation's rows.
pub fn arm(m: &mut Model, kind: &str, w: &mut Wire) -> io::Result<()> {
    match kind {
        "filter" if !m.filter.is_empty() => m.name_filter.enter_after = true,
        "search" => m.find.enter_after = Some(m.find.next),
        _ => return Ok(()),
    }
    landed(m, w)
}

// Any key or press other than a pointer's motion or release drops a waiting Enter.
pub fn cancel(m: &mut Model, key: &Key) {
    if key.pointer.as_ref().is_some_and(|p| p.motion || p.released) {
        return;
    }
    m.name_filter.enter_after = false;
    m.find.enter_after = None;
}

// The listing holds the keyboard: no line, menu, sheet, card, question or focused preview took it since the Enter.
fn listing_has_keys(m: &Model) -> bool {
    m.editor.is_none()
        && !m.menu
        && !m.sheet
        && m.properties.is_none()
        && m.prompts.question.is_none()
        && m.deletion.is_none()
        && m.combining.card.is_none()
        && m.converting.card.is_none()
        && m.printing.card.is_none()
        && !m.preview_focus
        && !m.quicklook
        && m.pending.is_none()
}

// After every reply: a waiting Enter whose answer is drawn acts on the highlighted row through the listing's own open.
pub fn landed(m: &mut Model, w: &mut Wire) -> io::Result<()> {
    if m.name_filter.enter_after && !m.name_filter.waiting {
        m.name_filter.enter_after = false;
        let shown = crate::tui::filter::answered(m)
            .is_some_and(|rows| rows.binary_search(&m.cursor).is_ok());
        if shown && listing_has_keys(m) && m.rows.contains_key(&m.cursor) {
            return crate::tui::actions::act(m, "open", w);
        }
    }
    let Some(gen) = m.find.enter_after else {
        return Ok(());
    };
    if m.find.active.as_ref().map(|q| q.gen) != Some(gen)
        || crate::tui::contents::holding(m)
        || !m.rows.contains_key(&m.cursor)
    {
        return Ok(());
    }
    m.find.enter_after = None;
    if listing_has_keys(m) && crate::tui::contents::operable(m, m.cursor) {
        return crate::tui::actions::act(m, "open", w);
    }
    Ok(())
}

#[cfg(test)]
#[path = "enter_once_tests.rs"]
mod tests;
