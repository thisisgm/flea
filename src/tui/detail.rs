use super::{model::{Model, Row}, wire::{count, flag, number, text, word, Wire}};
use crate::jsondoc::Json;
use std::{collections::BTreeMap, io, time::{Duration, Instant}};

// A lookup that could not run waits this long before its row asks again, so other drawn rows go first and a stuck mount never spins.
const OWNER_RETRY_DELAY: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Column { #[default] Size, Modified, Mode, Owner }
impl Column {
    fn next(self) -> Self {
        match self { Self::Size => Self::Modified, Self::Modified => Self::Mode, Self::Mode => Self::Owner, Self::Owner => Self::Size }
    }
    pub fn label(self) -> &'static str {
        match self { Self::Size => "Size", Self::Modified => "Modified", Self::Mode => "Mode", Self::Owner => "Owner" }
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
    model.say(format!("Detail column · {}", model.detail.mode.label()));
}
pub fn value(model: &Model, index: usize, row: &Row) -> String {
    if row.mode == 0 { return String::new(); }
    match model.detail.mode {
        Column::Size => model.row_size(index, row),
        Column::Modified => super::calendar::modified(row.modified),
        Column::Mode => format!("{:04o}", row.mode & 0o7777),
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
