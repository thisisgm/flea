//! ui/js/ColumnsCarry.js: a navigation leaving a listing seeds the columns it already holds, and the landing keeps the answered ones.
use super::geometry::{ancestor_parent, neighbour_asks};
use super::peeks::{sent_key, Facts, Lens, PeekRow, Peeks, PEEK_CAP};
use super::{PaneView, Ports};
use flea::protocol::RowFacts;
use std::rc::Rc;

/// The listing order a carried seed must match: a peek sorts name-first with folders first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListOrder<'a> {
    pub by: &'a str,
    pub desc: bool,
    pub folders_first: bool,
    pub group_by_kind: bool,
}

pub fn same_order(list: ListOrder<'_>, tag: &str) -> bool {
    list.by == "name"
        && !list.desc
        && list.folders_first
        && !list.group_by_kind
        && (tag.is_empty() || tag == "name0")
}

/// True when the target is the parent of the shown path: the one hop whose child is the folder left.
pub fn is_back(here: &str, target: &str) -> bool {
    target != here && target == ancestor_parent(here)
}

/// The held window as peek rows; a link or an executable seeds nothing, shaped differently in a peek.
pub fn peek_rows(rows: &[RowFacts], count: usize) -> Option<Rc<[PeekRow]>> {
    rows.get(..count)?
        .iter()
        .map(|row| {
            let plain = row.link.is_none()
                && !crate::row::is_symlink(row.mode)
                && (row.dir || row.icon != "application-x-executable");
            plain.then(|| PeekRow {
                name: row.name.clone(),
                dir: row.dir,
                icon: row.icon.clone(),
            })
        })
        .collect()
}

/// What a navigation's plan reads from the pane at its request.
#[derive(Clone, Copy, Debug)]
pub struct Leaving<'a> {
    pub here: &'a str,
    pub target: &'a str,
    pub tag: &'a str,
    pub window_size: usize,
    pub total: usize,
    pub held_start: usize,
    pub narrowed: bool,
    pub rows: &'a [RowFacts],
    pub order: ListOrder<'a>,
    pub facts: Facts,
}

/// A navigation's plan from its request to its flight's end.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub target: String,
    pub seed: String,
    pub tag: String,
    pub pre: Vec<String>,
    pub rows: Option<Rc<[PeekRow]>>,
    pub facts: Facts,
}

impl Plan {
    /// None before the first listing has anything to carry; narrowed is a filter or search over the rows.
    pub fn new(pane: &Leaving<'_>, asks: &[String]) -> Option<Self> {
        if pane.here.is_empty() || pane.target.is_empty() {
            return None;
        }
        let count = pane.window_size.min(PEEK_CAP).min(pane.total);
        let clean = pane.held_start == 0
            && !pane.narrowed
            && pane.rows.len() >= count
            && same_order(pane.order, pane.tag);
        Some(Self {
            target: pane.target.to_owned(),
            seed: pane.here.to_owned(),
            tag: pane.tag.to_owned(),
            pre: asks
                .iter()
                .filter(|path| *path != pane.target && *path != pane.here)
                .cloned()
                .collect(),
            rows: if clean {
                peek_rows(pane.rows, count)
            } else {
                None
            },
            facts: pane.facts,
        })
    }

    /// Carry.landing: answered drawn folders under the same key and order, the old third, and the seed from the listing left.
    pub fn land(
        carry: Option<&Self>,
        asks: &[String],
        shown_child: &str,
        peeks: &Peeks,
        lens: &Lens<'_>,
    ) -> Peeks {
        let (key, order) = (|path: &str| lens.key(path), |path: &str| lens.tag(path));
        let mut next = Peeks {
            version: peeks.version + 1,
            ..Peeks::default()
        };
        let Some(carry) = carry.filter(|carry| carry.target == lens.here) else {
            return next;
        };
        let mut paths = asks.to_vec();
        if !shown_child.is_empty() && !paths.iter().any(|path| path == shown_child) {
            paths.push(shown_child.to_owned());
        }
        for path in &paths {
            peeks.keep_into(&key(path), path, &order(path), &mut next);
        }
        let seed_key = key(&carry.seed);
        let carried = asks.contains(&carry.seed) || is_back(&carry.seed, &carry.target);
        if carried && carry.tag == order(&carry.seed) && next.rows(&seed_key).is_none() {
            if let Some(rows) = &carry.rows {
                next.seed(
                    seed_key,
                    &carry.seed,
                    Rc::clone(rows),
                    carry.tag.clone(),
                    carry.facts,
                );
            }
        }
        if carried {
            paths.push(carry.seed.clone());
        }
        // A watched refresh already out must still land after navigation, even when its folder's rows are kept.
        for path in &paths {
            let sent = sent_key(&key(path), lens.first, &order(path));
            peeks.pending.carry(&sent, &mut next.pending);
        }
        next
    }
}

/// The listing transitions a carry spans: the request going out, the path landing and the flight ending.
impl super::Area {
    /// onListingPathChanged: incoming ancestors are asked before the list goes out, each drawn column staying watched.
    pub fn listing_requested(&mut self, leaving: Leaving<'_>, lens: &Lens<'_>, now: u64) {
        let asks = neighbour_asks(leaving.target, self.layout.width, self.limit);
        let tag = lens.tag(leaving.here);
        let leaving = Leaving {
            tag: &tag,
            ..leaving
        };
        self.carry = if self.visible {
            Plan::new(&leaving, &asks)
        } else {
            None
        };
        let old = neighbour_asks(lens.here, self.layout.width, self.limit);
        self.landing
            .begin(self.carry.as_ref(), &asks, &old, &self.peeks, lens, now);
        let pre = self
            .carry
            .as_ref()
            .map(|carry| carry.pre.clone())
            .unwrap_or_default();
        for path in pre {
            if !self.drawn.contains(&path) {
                self.drawn.push(path.clone());
            }
            self.ask(lens, &path, true, false);
        }
    }

    /// onPathChanged: the landing keeps answered drawn folders, then one refresh asks what it lacks.
    pub fn path_landed(&mut self, pane: &PaneView<'_>, ports: &mut Ports<'_>, now: u64) {
        let lens = pane.lens;
        let asks = neighbour_asks(lens.here, self.layout.width, self.limit);
        self.peeks = Plan::land(
            self.carry.as_ref(),
            &asks,
            &self.host.shown.child.clone(),
            &self.peeks,
            &lens,
        );
        self.landing.landed(lens.here);
        self.refresh(pane, ports);
        self.sync(pane, ports, now);
    }

    /// The flight's end: the landed cursor moves the third column, then one refresh asks what the landing lacks.
    pub fn flight_ended(&mut self, pane: &PaneView<'_>, ports: &mut Ports<'_>, now: u64) {
        if pane.in_flight || self.carry.take().is_none() {
            return;
        }
        self.move_third(pane, ports, false, now);
        self.refresh(pane, ports);
        self.sync(pane, ports, now);
    }
}
