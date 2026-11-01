use super::{model::{Model, Row}, wire::{count, flag, number, text, word, Wire}};
use crate::jsondoc::Json;
use std::{collections::BTreeMap, io, time::{Duration, Instant}};

// A lookup that could not run waits this long before its row asks again, so other drawn rows go first and a stuck mount never spins.
const OWNER_RETRY_DELAY: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Column { #[default] Size, Modified, Mode, Owner, Birth, Off }
impl Column {
    fn next(self) -> Self {
        match self { Self::Size => Self::Modified, Self::Modified => Self::Mode, Self::Mode => Self::Owner, Self::Owner | Self::Birth | Self::Off => Self::Size }
    }
    pub fn key(self) -> &'static str {
        match self { Self::Size => "window.column_size", Self::Modified => "window.column_modified", Self::Mode => "list.column_mode", Self::Owner => "properties.owner", Self::Birth => "properties.birth", Self::Off => "settings.choice.off" }
    }
    #[cfg(test)]
    pub fn label(self) -> &'static str {
        match self { Self::Size => "Size", Self::Modified => "Modified", Self::Mode => "Mode", Self::Owner => "Owner", Self::Birth => "Created", Self::Off => "Off" }
    }
}
#[derive(Default)]
pub struct Detail {
    pub mode: Column,
    generation: usize,
    token: usize,
    owners: BTreeMap<usize, String>,
    retry_after: BTreeMap<usize, Instant>,
    pending: Option<(usize, usize, usize, String)>,
}
impl Detail {
    pub fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.owners.clear();
        self.retry_after.clear();
    }
    pub fn receive(&mut self, value: &Json, rows: &BTreeMap<usize, Row>) {
        if !self.pending.as_ref().is_some_and(|pending| pending.0 == count(value, "token")) { return; }
        let (_, generation, index, name) = self.pending.take().unwrap();
        if generation == self.generation && count(value, "row") == index
            && rows.get(&index).is_some_and(|row| row.name == name) {
            if flag(value, "retry") {
                self.retry_after.insert(index, Instant::now() + OWNER_RETRY_DELAY);
            } else {
                self.retry_after.remove(&index);
                self.owners.insert(index, text(value, "owner").into());
            }
        }
    }
}
pub fn cycle(model: &mut Model) {
    model.detail.mode = model.detail.mode.next();
    model.say_text(super::words::args("tui.frame.detail", vec![super::words::Text::Key(model.detail.mode.key())]));
}
pub fn value(model: &Model, index: usize, row: &Row) -> String {
    if row.mode == 0 { return String::new(); }
    match model.detail.mode {
        Column::Off => String::new(),
        Column::Birth => row.birth.and_then(|time| model.i18n.locale().format_date(time)).unwrap_or_else(|| model.i18n.tr("pane.unavailable")),
        // A link names its target instead of a size, as v0.3.12 and the TUI boards draw it.
        Column::Size if !row.link.is_empty() => String::new(),
        Column::Size => model.row_size(index, row),
        Column::Modified => model.i18n.locale().format_date(row.modified).filter(|_| row.modified > 0).unwrap_or_default(),
        Column::Mode => model.i18n.locale().localize_number(&format!("{:04o}", row.mode & 0o7777)),
        Column::Owner => model.detail.owners.get(&index).map(|owner| super::cells::clean(owner)).unwrap_or_default(),
    }
}
// One asynchronous owner lookup at a time, for drawn rows only, never from the painter.
pub fn prepare(model: &mut Model, wire: &mut Wire) -> io::Result<()> {
    if model.detail.mode != Column::Owner || model.pending.is_some() || model.detail.pending.is_some() { return Ok(()); }
    let now = Instant::now();
    let Some(index) = model.visible().into_iter().find(|index| !model.detail.owners.contains_key(index)
        && !model.detail.retry_after.get(index).is_some_and(|after| now < *after)
        && model.rows.get(index).is_some_and(|row| row.mode != 0)) else { return Ok(()); };
    model.detail.token = model.detail.token.wrapping_add(1).max(1);
    let token = model.detail.token;
    wire.send(vec![("c", word("owner")), ("row", number(index)), ("token", number(token))])?;
    model.detail.pending = Some((token, model.detail.generation, index, model.rows[&index].name.clone()));
    Ok(())
}

#[cfg(test)]
#[path = "detail_tests.rs"]
mod tests;
