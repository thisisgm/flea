//! ColumnsArea's peek bookkeeping: the asks out, the rows each answered key holds, and the order that sorted them.
use crate::navigation::folder_state::Order;
use flea::{json::escape, jsondoc::Json};
use std::{collections::HashMap, rc::Rc};

/// A peek answers at most this many rows, src/backend/peek.rs PEEK_CAP.
pub const PEEK_CAP: usize = 512;

/// The key a column's rows are kept under: the path plus hidden and hiddenLast, so a toggle never shows stale rows.
pub fn peek_key(path: &str, hidden: bool, hidden_last: bool) -> String {
    format!("{path}\n{}{}", u8::from(hidden), u8::from(hidden_last))
}

/// orderTag: "" for no order of the folder's own, else the key and 1 when descending.
pub fn order_tag(by: &str, desc: bool) -> String {
    if by.is_empty() {
        String::new()
    } else {
        format!("{by}{}", u8::from(desc))
    }
}

pub fn tag_of(order: Option<Order>) -> String {
    order.map_or_else(String::new, |order| order_tag(order.key, order.desc))
}

/// sentKey: the row count and order join the key only for an outstanding ask, so a resize never orphans a stored column.
pub fn sent_key(key: &str, first: usize, order: &str) -> String {
    if order.is_empty() {
        format!("{key}\n{first}")
    } else {
        format!("{key}\n{first}\n{order}")
    }
}

/// The asks this view made and still awaits, counted per sent key, so another client's reply never lands in a column.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pending(HashMap<String, u32>);

impl Pending {
    pub fn track(&mut self, sent: &str) {
        *self.0.entry(sent.to_owned()).or_insert(0) += 1;
    }

    pub fn has(&self, sent: &str) -> bool {
        self.count(sent) > 0
    }

    pub fn count(&self, sent: &str) -> u32 {
        self.0.get(sent).copied().unwrap_or(0)
    }

    /// One ask answered or superseded; a key asked twice keeps one.
    pub fn drop_one(&mut self, sent: &str) {
        if let Some(left) = self.0.get_mut(sent) {
            *left -= 1;
            if *left == 0 {
                self.0.remove(sent);
            }
        }
    }

    pub fn len(&self) -> usize {
        self.0.values().map(|count| *count as usize).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(super) fn carry(&self, sent: &str, into: &mut Self) {
        if let Some(count) = self.0.get(sent) {
            into.0.insert(sent.to_owned(), *count);
        }
    }
}

/// One peeked row: a peek is name-first and unstat'd, so it carries the name, the folder flag and the mark's icon name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeekRow {
    pub name: String,
    pub dir: bool,
    pub icon: String,
}

/// The device and writability a column's last peek named, which a drag from that column carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Facts {
    pub dev: u64,
    pub writable: bool,
}

/// What a drag from a peek column's row needs: the column's directory and its peeked device and writability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub dir: String,
    pub dev: u64,
    pub writable: bool,
}

/// One peek request, the fields Backend.qml peek() sends.
#[derive(Clone, Copy, Debug)]
pub struct Ask<'a> {
    pub path: &'a str,
    pub first: usize,
    pub hidden: bool,
    pub hidden_last: bool,
    pub order: Option<Order>,
    pub keep: &'a [String],
}

impl Ask<'_> {
    pub fn key(&self) -> String {
        peek_key(self.path, self.hidden, self.hidden_last)
    }

    pub fn sent(&self) -> String {
        sent_key(&self.key(), self.first, &tag_of(self.order))
    }

    /// The wire line: a columns ask always watches and names every drawn directory, so the backend unwatches the rest.
    pub fn line(&self) -> String {
        let keep: Vec<String> = self
            .keep
            .iter()
            .map(|path| format!("\"{}\"", escape(path)))
            .collect();
        let order = self
            .order
            .map(|order| format!(",\"by\":\"{}\",\"desc\":{}", escape(order.key), order.desc))
            .unwrap_or_default();
        format!(
            "{{\"c\":\"peek\",\"path\":\"{}\",\"first\":{},\"hidden\":{},\"hiddenLast\":{},\"watch\":true,\"keep\":[{}]{order}}}",
            escape(self.path),
            self.first,
            self.hidden,
            self.hidden_last,
            keep.join(",")
        )
    }
}

/// What the area asks under: the pane's path, its hidden flags, its window size and each folder's own order.
#[derive(Clone, Copy)]
pub struct Lens<'a> {
    pub here: &'a str,
    pub hidden: bool,
    pub hidden_last: bool,
    pub first: usize,
    pub orders: &'a dyn Fn(&str) -> Option<Order>,
}

impl Lens<'_> {
    pub fn key(&self, path: &str) -> String {
        peek_key(path, self.hidden, self.hidden_last)
    }

    pub fn tag(&self, path: &str) -> String {
        tag_of((self.orders)(path))
    }

    pub fn ask<'b>(&'b self, path: &'b str, keep: &'b [String]) -> Ask<'b> {
        Ask {
            path,
            first: self.first,
            hidden: self.hidden,
            hidden_last: self.hidden_last,
            order: (self.orders)(path),
            keep,
        }
    }

    /// A column answered under the order its folder holds now, which a landing keeps and a stand-in waits on.
    pub fn ready(&self, peeks: &Peeks, path: &str) -> bool {
        let key = self.key(path);
        peeks.rows(&key).is_some() && peeks.order(&key) == Some(self.tag(path).as_str())
    }
}

/// One peeked reply, every field the request echoed plus the answer.
#[derive(Clone, Debug, PartialEq)]
pub struct Peeked {
    pub path: String,
    pub hidden: bool,
    pub hidden_last: bool,
    pub first: usize,
    pub order: String,
    pub total: usize,
    pub facts: Facts,
    /// The denial's directory mode, 0 when the stat failed too; None for a read that answered.
    pub denied: Option<u32>,
    pub rows: Rc<[PeekRow]>,
}

impl Peeked {
    pub fn key(&self) -> String {
        peek_key(&self.path, self.hidden, self.hidden_last)
    }

    pub fn sent(&self) -> String {
        sent_key(&self.key(), self.first, &self.order)
    }

    /// A parsed {"t":"peeked"} document; any other document answers None, and a malformed peeked line its catalog key.
    pub fn from_json(document: &Json) -> Option<Result<Self, &'static str>> {
        (document.get("t").and_then(Json::as_str) == Some("peeked")).then(|| Self::read(document))
    }

    fn read(document: &Json) -> Result<Self, &'static str> {
        let flag = |key| document.get(key).and_then(Json::as_bool).unwrap_or(false);
        let count = |key| {
            document
                .get(key)
                .and_then(Json::as_f64)
                .filter(|n| *n >= 0.)
                .map_or(0, |n| n as u64)
        };
        let path = document
            .get("path")
            .and_then(Json::as_str)
            .ok_or("columns.peek_path")?;
        let rows = document
            .get("rows")
            .and_then(Json::as_array)
            .ok_or("columns.peek_rows")?;
        let rows = rows
            .iter()
            .map(|row| {
                Ok(PeekRow {
                    name: row
                        .get("n")
                        .and_then(Json::as_str)
                        .ok_or("columns.peek_row_name")?
                        .to_owned(),
                    dir: row.get("d").and_then(Json::as_bool).unwrap_or(false),
                    icon: row.get("i").and_then(Json::as_str).unwrap_or("").to_owned(),
                })
            })
            .collect::<Result<Rc<[PeekRow]>, &'static str>>()?;
        let by = document.get("by").and_then(Json::as_str).unwrap_or("");
        Ok(Self {
            path: path.to_owned(),
            hidden: flag("hidden"),
            hidden_last: flag("hiddenLast"),
            first: count("first") as usize,
            order: order_tag(by, flag("desc")),
            total: count("n") as usize,
            facts: Facts {
                dev: count("v"),
                writable: document.get("w").and_then(Json::as_bool) != Some(false),
            },
            denied: flag("failed").then(|| count("mode") as u32),
            rows,
        })
    }
}

/// What a reply did to the store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Landed {
    /// Another client's reply, or one this view already dropped.
    Unasked,
    /// Rows sorted under an order the folder has since left; the column asks again under its new order.
    StaleOrder,
    Stored,
}

/// The answered columns: rows, order, denial and facts per key, and the asks still out.
#[derive(Clone, Debug, Default)]
pub struct Peeks {
    pub(super) rows: HashMap<String, Rc<[PeekRow]>>,
    pub(super) orders: HashMap<String, String>,
    pub(super) denials: HashMap<String, u32>,
    pub(super) facts: HashMap<String, Facts>,
    pub pending: Pending,
    /// Bumped on every landing, the trigger a drawn column re-reads on.
    pub version: u64,
}

impl Peeks {
    /// A peek still out answers zero rows the way an empty directory does, so only a landed reply counts as answered.
    pub fn answered(&self, key: &str, path: &str) -> bool {
        !path.is_empty() && self.rows.contains_key(key)
    }

    pub fn rows(&self, key: &str) -> Option<&Rc<[PeekRow]>> {
        self.rows.get(key)
    }

    pub fn order(&self, key: &str) -> Option<&str> {
        self.orders.get(key).map(String::as_str)
    }

    /// The mode a denied peek named; None for a column that was not denied, so mode 0 stays its own answer.
    pub fn denied(&self, key: &str) -> Option<u32> {
        self.denials.get(key).copied()
    }

    pub fn source(&self, path: &str) -> Option<Source> {
        let facts = self.facts.get(path).copied().unwrap_or(Facts {
            dev: 0,
            writable: true,
        });
        (!path.is_empty()).then(|| Source {
            dir: path.to_owned(),
            dev: facts.dev,
            writable: facts.writable,
        })
    }

    /// ask(): again re-asks a held column, rearm asks with the first ask still out, and a changed order always re-asks.
    pub fn should_ask(&self, ask: &Ask<'_>, again: bool, rearm: bool) -> bool {
        let key = ask.key();
        let order = tag_of(ask.order);
        !ask.path.is_empty()
            && (again || !self.rows.contains_key(&key) || self.order(&key) != Some(order.as_str()))
            && (rearm || !self.pending.has(&ask.sent()))
    }

    /// onPeeked: only a reply this view asked for under the folder's current order lands.
    pub fn land(&mut self, reply: &Peeked, current_order: &str) -> Landed {
        let sent = reply.sent();
        if !self.pending.has(&sent) {
            return Landed::Unasked;
        }
        self.pending.drop_one(&sent);
        if reply.order != current_order {
            return Landed::StaleOrder;
        }
        let key = reply.key();
        self.rows.insert(key.clone(), Rc::clone(&reply.rows));
        self.orders.insert(key.clone(), reply.order.clone());
        self.facts.insert(reply.path.clone(), reply.facts);
        match reply.denied {
            Some(mode) => self.denials.insert(key, mode),
            None => self.denials.remove(&key),
        };
        self.version += 1;
        Landed::Stored
    }

    /// Store one column directly, the carry's seed from the listing it leaves.
    pub(super) fn seed(
        &mut self,
        key: String,
        path: &str,
        rows: Rc<[PeekRow]>,
        order: String,
        facts: Facts,
    ) {
        self.rows.insert(key.clone(), rows);
        self.orders.insert(key, order);
        self.facts.insert(path.to_owned(), facts);
    }

    /// Copy one held column across a landing when its order still matches.
    pub(super) fn keep_into(&self, key: &str, path: &str, order: &str, into: &mut Self) {
        let (Some(rows), Some(held)) = (self.rows.get(key), self.orders.get(key)) else {
            return;
        };
        if held != order {
            return;
        }
        into.rows.insert(key.to_owned(), Rc::clone(rows));
        into.orders.insert(key.to_owned(), held.clone());
        if let Some(mode) = self.denials.get(key) {
            into.denials.insert(key.to_owned(), *mode);
        }
        if let Some(facts) = self.facts.get(path) {
            into.facts.insert(path.to_owned(), *facts);
        }
    }

    /// The names a folder last read, drawn or not, "|" joined: Ipc columnPeekNames.
    pub fn names(&self, key: &str) -> String {
        self.rows
            .get(key)
            .map(|rows| join_names(rows))
            .unwrap_or_default()
    }
}

pub fn join_names(rows: &[PeekRow]) -> String {
    rows.iter()
        .map(|row| row.name.as_str())
        .collect::<Vec<_>>()
        .join("|")
}
