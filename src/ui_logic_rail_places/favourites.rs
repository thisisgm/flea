//! The Favorites writer every caller shares: one operation in flight, the CLI locks and re-reads before editing.
use super::places::stringify;
use crate::jsondoc::{self, Json};
use std::collections::HashMap;

// The most indices one inspection names, the CLI's own MAX_VISIBLE.
pub const INSPECT_MAX: usize = 128;
// An inspection on a dead mount stops after this long, so it never holds the one-at-a-time guard; ui/js/Startup.js INSPECT_WAIT_MS.
pub const INSPECT_WAIT_MS: u64 = 10_000;

#[derive(Clone, Debug, PartialEq)]
pub enum Operation {
    Add { label: String, path: String },
    Remove { index: usize },
    Move { index: usize, to: usize },
    Rename { index: usize, label: String },
}

fn num(n: usize) -> Json {
    Json::Num(n.to_string())
}

impl Operation {
    // The JSON `flea --favourites` takes; every edit but add carries the records it was decided against.
    pub fn wire(&self, records: &[Json]) -> String {
        let expected = ("expected".to_owned(), Json::Arr(records.to_vec()));
        let fields = match self {
            Self::Add { label, path } => vec![
                ("op".to_owned(), Json::Str("add".into())),
                (
                    "record".to_owned(),
                    Json::Obj(vec![
                        ("label".into(), Json::Str(label.clone())),
                        ("path".into(), Json::Str(path.clone())),
                    ]),
                ),
            ],
            Self::Remove { index } => vec![
                ("op".into(), Json::Str("remove".into())),
                ("index".into(), num(*index)),
                expected,
            ],
            Self::Move { index, to } => {
                vec![
                    ("op".into(), Json::Str("move".into())),
                    ("index".into(), num(*index)),
                    ("to".into(), num(*to)),
                    expected,
                ]
            }
            Self::Rename { index, label } => vec![
                ("op".into(), Json::Str("rename".into())),
                ("index".into(), num(*index)),
                ("label".into(), Json::Str(label.clone())),
                expected,
            ],
        };
        stringify(&Json::Obj(fields))
    }

    // The records this operation should leave, as ui/js/UiState.js favouritesAfter predicts them.
    pub fn after(&self, records: &[Json]) -> Vec<Json> {
        let mut next = records.to_vec();
        match self {
            Self::Add { label, path } => next.push(Json::Obj(vec![
                ("label".into(), Json::Str(label.clone())),
                ("path".into(), Json::Str(path.clone())),
            ])),
            Self::Remove { index } if *index < next.len() => {
                next.remove(*index);
            }
            Self::Move { index, to } if *index < next.len() => {
                let moved = next.remove(*index);
                next.insert((*to).min(next.len()), moved);
            }
            Self::Rename { index, label } => {
                if let Some(Json::Obj(pairs)) = next.get_mut(*index) {
                    match pairs.iter_mut().find(|(key, _)| key == "label") {
                        Some((_, value)) => *value = Json::Str(label.clone()),
                        None => pairs.push(("label".into(), Json::Str(label.clone()))),
                    }
                }
            }
            _ => {}
        }
        next
    }
}

// Why a write is refused or how it failed; a CLI line is an opaque diagnostic carried inside a localized sentence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    Busy,
    NotSaved,
    NotSavedBecause(String),
    SavedUnread,
    InspectUnread,
    ChangedReopen,
}

impl Failure {
    pub fn key(&self) -> &'static str {
        match self {
            Self::Busy => "places.favorite.busy",
            Self::NotSaved => "places.favorite.save_failed",
            Self::NotSavedBecause(_) => "places.favorite.save_failed_reason",
            Self::SavedUnread => "places.favorite.saved_unread",
            Self::InspectUnread => "places.favorite.inspect_failed",
            Self::ChangedReopen => "places.favorite.changed_reopen",
        }
    }

    pub fn args(&self) -> Vec<&str> {
        match self {
            Self::NotSavedBecause(line) => vec![line.as_str()],
            _ => Vec::new(),
        }
    }
}

// How a finished write lands: the failure to say, whether the write committed, and whether others must re-read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finished {
    pub failure: Option<Failure>,
    pub committed: bool,
    pub external_change: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Favourites {
    pub records: Vec<Json>,
    // Inspection diagnostics by record index, cleared whenever the records change.
    pub statuses: HashMap<usize, String>,
    pub busy: bool,
    pub inspecting: bool,
    before: Vec<Json>,
    expected: Vec<Json>,
}

impl Favourites {
    // New records from the state file; a change nobody here made tells the callers to re-read their rows.
    pub fn records_changed(&mut self, records: Vec<Json>) -> bool {
        self.statuses.clear();
        self.records = records;
        !self.busy
    }

    // Start one write; the caller runs `flea --favourites <wire>` off the UI thread.
    pub fn apply(&mut self, operation: &Operation) -> Result<String, Failure> {
        if self.busy {
            return Err(Failure::Busy);
        }
        self.before = self.records.clone();
        self.expected = operation.after(&self.records);
        self.busy = true;
        Ok(operation.wire(&self.records))
    }

    // Exit zero adopts the state the CLI printed (committed even if unreadable); a refused write adopts refreshed, the re-read state.
    pub fn finish(
        &mut self,
        code: Option<i32>,
        stdout: &str,
        stderr: &str,
        refreshed: Option<Vec<Json>>,
    ) -> Finished {
        self.busy = false;
        if code != Some(0) {
            if let Some(records) = refreshed {
                self.adopt(records);
            }
            let line = super::js_trim(stderr).split('\n').next().unwrap_or("");
            let line = line.strip_prefix("flea: ").unwrap_or(line);
            let failure = if line.is_empty() {
                Failure::NotSaved
            } else {
                Failure::NotSavedBecause(line.to_owned())
            };
            return Finished {
                failure: Some(failure),
                committed: false,
                external_change: self.records != self.before,
            };
        }
        let printed = jsondoc::parse(stdout).ok().and_then(|state| {
            state
                .get("places")
                .and_then(|p| p.get("favourites"))
                .and_then(Json::as_array)
                .map(<[Json]>::to_vec)
        });
        match printed {
            Some(records) => {
                self.adopt(records);
                Finished {
                    failure: None,
                    committed: true,
                    external_change: self.records != self.expected,
                }
            }
            None => Finished {
                failure: Some(Failure::SavedUnread),
                committed: true,
                external_change: self.records != self.before,
            },
        }
    }

    fn adopt(&mut self, records: Vec<Json>) {
        if records != self.records {
            self.statuses.clear();
            self.records = records;
        }
    }

    // Unchecked or unavailable visible indices, asked again only on a viewport change or explicit activation.
    pub fn inspect(&mut self, visible: &[usize]) -> Option<String> {
        let pending: Vec<Json> = visible
            .iter()
            .filter(|index| {
                self.statuses
                    .get(index)
                    .map_or(true, |status| !status.is_empty())
            })
            .take(INSPECT_MAX)
            .map(|index| num(*index))
            .collect();
        if pending.is_empty() || self.inspecting {
            return None;
        }
        self.inspecting = true;
        Some(stringify(&Json::Obj(vec![
            ("op".into(), Json::Str("inspect".into())),
            ("indices".into(), Json::Arr(pending)),
        ])))
    }

    // An inspection answer lands only for records that still read the same; a timeout or bad exit lands nothing.
    pub fn inspected(&mut self, code: Option<i32>, answer: &str) -> Option<Failure> {
        self.inspecting = false;
        if code != Some(0) {
            return None;
        }
        let parsed = jsondoc::parse(answer).ok();
        let Some(rows) = parsed
            .as_ref()
            .and_then(|doc| doc.get("statuses"))
            .and_then(Json::as_array)
        else {
            return Some(Failure::InspectUnread);
        };
        // The whole answer reads before any status lands, as v0.3.12 assigns its map only after the loop.
        let mut next = self.statuses.clone();
        for row in rows {
            let index = row
                .get("index")
                .and_then(Json::as_f64)
                .filter(|n| *n >= 0.0 && n.fract() == 0.0);
            let (Some(index), Some(record)) = (index.map(|n| n as usize), row.get("record")) else {
                return Some(Failure::InspectUnread);
            };
            if self.records.get(index) == Some(record) {
                let error = row
                    .get("error")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_owned();
                next.insert(index, error);
            }
        }
        self.statuses = next;
        None
    }

    // A place menu's remove, checked against the path it opened on, since the rail rebuilds under it.
    pub fn remove_at(&mut self, index: usize, path: &str) -> Result<String, Failure> {
        match self
            .records
            .get(index)
            .and_then(|record| record.get("path"))
            .and_then(Json::as_str)
        {
            Some(saved) if saved == path => self.apply(&Operation::Remove { index }),
            _ => Err(Failure::ChangedReopen),
        }
    }

    // The rail's older key, "favourite:<index>:<record JSON>", removes only while that record still stands there.
    pub fn remove_keyed(&mut self, key: &str) -> Result<String, Failure> {
        let rest = key
            .strip_prefix("favourite:")
            .ok_or(Failure::ChangedReopen)?;
        let (index, record) = rest.split_once(':').ok_or(Failure::ChangedReopen)?;
        let index: usize = index.parse().map_err(|_| Failure::ChangedReopen)?;
        match self.records.get(index) {
            Some(current) if stringify(current) == record => {
                self.apply(&Operation::Remove { index })
            }
            _ => Err(Failure::ChangedReopen),
        }
    }
}

// A middle click's new tab: a remote favourite must mount first, so it says it opens in this tab only.
pub const OPENS_HERE_KEY: &str = "places.favorite.opens_here";
