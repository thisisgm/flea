//! Contents match lines held by listing until their rows present; an older gen never joins, and the drawn listing keeps its lines.
use super::contents::{Block, Held};
use flea::{
    protocol::RowFacts,
    ui_logic_file_search::{self as core, ContentsMatch, ContentsMatches, Scope},
};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct Matches {
    gen: u64,
    newest: u64,
    // Keyed by listing then row; retain the drawn listing and the newest pending listing.
    lines: BTreeMap<(u64, usize), ContentsMatch>,
}

/// Where a block's location caption is read from: the scope root, home and scope of the standing session.
pub struct Place<'a> {
    pub root: &'a str,
    pub home: &'a str,
    pub scope: Scope,
}

impl Matches {
    /// Holds a batch of the session's newest gen; an older gen, or a listing older than one held, is refused.
    pub fn take(&mut self, batch: ContentsMatches, gen: u64, drawn: usize) -> bool {
        if batch.gen != gen
            || batch.gen < self.gen
            || (batch.gen == self.gen && batch.listing < self.newest)
        {
            return false;
        }
        self.gen = batch.gen;
        self.newest = batch.listing;
        self.lines
            .retain(|(listing, _), _| *listing == drawn as u64 || *listing == batch.listing);
        self.lines.extend(
            batch
                .matches
                .into_iter()
                .map(|hit| ((batch.listing, hit.row), hit)),
        );
        true
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// The match line of one row of the listing these lines belong to.
    pub fn line(&self, listing: usize, row: usize) -> Option<&ContentsMatch> {
        self.lines.get(&(listing as u64, row))
    }

    /// Whether every one of `count` rows from `start` has its line from this listing.
    pub fn covers(&self, listing: usize, start: usize, count: usize) -> bool {
        count > 0 && (start..start + count).all(|row| self.line(listing, row).is_some())
    }

    /// The held rows as blocks, only when every row has its line from the same listing; else the old snapshot stays.
    pub fn blocks(
        &self,
        listing: usize,
        total: usize,
        start: usize,
        rows: &[RowFacts],
        place: &Place,
    ) -> Option<Held> {
        let blocks = rows
            .iter()
            .enumerate()
            .map(|(at, row)| {
                let hit = self.line(listing, start + at)?;
                Some(Block {
                    row: RowFacts {
                        name: core::base(&hit.path).to_owned(),
                        ..row.clone()
                    },
                    location: super::run::location(&hit.path, place.root, place.home, place.scope),
                    line_no: hit.line_no as u64,
                    line: hit.line.clone(),
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Held {
            listing,
            total,
            start,
            blocks,
        })
    }
}
