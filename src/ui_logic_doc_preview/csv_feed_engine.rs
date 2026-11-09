//! A stand-in code engine for the feed tests.
use super::*;
use crate::ui_logic_doc_preview::csv::records;

/// The code engine's raw table answers, aligned to whole records as src/backend/csvrange.rs aligns them.
pub struct Engine {
    pub lines: Vec<String>,
    pub delim: char,
    pub state: Answer,
    pub next: u64,
    pub asks: Vec<Ask>,
}

impl Engine {
    pub fn new(text: &str, delim: char) -> Self {
        Engine {
            lines: csv::plain_lines(text),
            delim,
            state: Answer::Plain,
            next: 0,
            asks: Vec::new(),
        }
    }

    pub fn refusing(state: Answer) -> Self {
        Engine {
            state,
            ..Engine::new("", ',')
        }
    }

    pub fn reply(&mut self, ask: &Ask) -> Reply {
        self.next += 1;
        self.asks.push(ask.clone());
        if self.state.refused() {
            return Reply {
                token: self.next,
                state: self.state,
                first: 1,
                total: 0,
                records: 0,
                first_record: 0,
                lines: vec![],
            };
        }
        let mut scanned = self.lines.clone();
        csv::strip_bom(&mut scanned);
        let recs = records(&scanned, Some(self.delim));
        let mut starts = Vec::new();
        let mut at = 1;
        for rec in &recs {
            starts.push((at, rec.span));
            at += rec.span;
        }
        let Some(&(first, _)) = starts.get(ask.record - 1) else {
            return Reply {
                token: self.next,
                state: self.state,
                first: ask.first,
                total: self.lines.len(),
                records: recs.len(),
                first_record: ask.record,
                lines: vec![],
            };
        };
        let wanted_end = first + ask.count - 1;
        let mut last = first;
        for &(start, span) in &starts[ask.record - 1..] {
            last = start + span - 1;
            if last >= wanted_end {
                break;
            }
        }
        let last = last.min(self.lines.len());
        Reply {
            token: self.next,
            state: self.state,
            first,
            total: self.lines.len(),
            records: recs.len(),
            first_record: ask.record,
            lines: self.lines[first - 1..last].to_vec(),
        }
    }
}

// Caption 12 with a 16 px line, as the board draws the table, and a 14 px row padding.
pub fn look() -> Geometry {
    Geometry::new(true, 12., 16., 14., 1.)
}

pub fn column() -> Geometry {
    Geometry::new(false, 12., 16., 14., 1.)
}

// A monospace advance at the caption, as JetBrains Mono's at 12 px.
pub fn measure(text: &str, _: Face) -> f64 {
    text.chars().count() as f64 * 7.2
}

// Every ask the view needs, answered and parsed in turn, as the view's pump runs them; answers the asks sent.
pub fn pump(feed: &mut Feed, engine: &mut Engine) -> usize {
    let mut sent = 0;
    while let Some(ask) = feed.ask() {
        sent += 1;
        let reply = engine.reply(&ask);
        feed.sent(reply.token);
        assert!(feed.answered(&reply), "the ask in flight takes its reply");
        let table = feed.job().run();
        feed.apply(table);
        feed.measure(f64::INFINITY, &mut measure);
        assert!(sent < 500, "the asks end");
    }
    sent
}

pub fn bench() -> String {
    let mut text = String::from("fixture,files,folders,depth\n");
    for i in 0..24 {
        text.push_str(&format!("tree{i},{},{},{}\n", 1000 + i, 10 + i, i % 5));
    }
    text
}

pub fn open(text: &str, delim: char, geometry: Geometry, rows: usize) -> (Feed, Engine) {
    let mut feed = Feed::new(Some(delim), geometry);
    feed.set_view(rows, 2);
    (feed, Engine::new(text, delim))
}
