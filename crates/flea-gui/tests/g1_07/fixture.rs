//! Fixtures for driving the Columns model: a pane snapshot, peek replies built from wire text, and producers that record.
use flea::protocol::RowFacts;
use flea_gui::columns::{
    peeks::{Lens, PeekRow, Peeked},
    preview_host::{PreviewProducer, PreviewSettings, ThirdSwap},
    Area, PaneView, Ports,
};
use flea_gui::navigation::folder_state::Order;
use std::collections::HashMap;

pub fn row(name: &str, dir: bool) -> RowFacts {
    RowFacts {
        listing_index: None,
        source: None,
        depth: 0,
        open: false,
        name: name.into(),
        dir,
        size: if dir { 4096 } else { 13 },
        mtime: 1_760_600_000,
        mode: if dir { 0o040755 } else { 0o100644 },
        icon: if dir { "folder" } else { "text-plain" }.into(),
        link: None,
        real_modes: true,
        thumbless: false,
        kind: String::new(),
        inline: None,
        tags: None,
    }
}

pub fn peek_row(name: &str, dir: bool) -> PeekRow {
    PeekRow {
        name: name.into(),
        dir,
        icon: if dir { "folder" } else { "text-plain" }.into(),
    }
}

/// The pane state a Columns event reads, owned so a test can change it between events.
pub struct Pane {
    pub here: String,
    pub base: String,
    pub rows: Vec<RowFacts>,
    pub cursor: usize,
    pub listing: usize,
    pub in_flight: bool,
    pub filter_hid_cursor: bool,
    pub hidden: bool,
    pub hidden_last: bool,
    pub first: usize,
    /// A folder's remembered order; a folder absent here sorts by the default name order.
    pub orders: HashMap<String, Order>,
}

impl Pane {
    pub fn at(here: &str, rows: Vec<RowFacts>) -> Self {
        Self {
            here: here.into(),
            base: here.into(),
            rows,
            cursor: 0,
            listing: 1,
            in_flight: false,
            filter_hid_cursor: false,
            hidden: false,
            hidden_last: false,
            first: 512,
            orders: HashMap::new(),
        }
    }

    /// Moves to another listing with the cursor on its first row.
    pub fn go(&mut self, here: &str, rows: Vec<RowFacts>) {
        self.here = here.into();
        self.base = here.into();
        self.rows = rows;
        self.cursor = 0;
        self.listing += 1;
    }

    pub fn cursor_row(&self) -> Option<&RowFacts> {
        if self.filter_hid_cursor {
            None
        } else {
            self.rows.get(self.cursor)
        }
    }

    /// Runs one Columns call with this pane's snapshot.
    pub fn with<R>(&self, call: impl FnOnce(PaneView<'_>) -> R) -> R {
        let orders = |path: &str| self.orders.get(path).copied();
        let lens = Lens {
            here: &self.here,
            hidden: self.hidden,
            hidden_last: self.hidden_last,
            first: self.first,
            orders: &orders,
        };
        call(PaneView {
            lens,
            base: &self.base,
            cursor_row: self.cursor_row(),
            cursor: self.cursor,
            listing: self.listing,
            in_flight: self.in_flight,
            filter_hid_cursor: self.filter_hid_cursor,
        })
    }
}

/// Every call the third column's producers received, in order.
#[derive(Default)]
pub struct Recorder {
    pub calls: Vec<String>,
    pub holding: bool,
}

impl ThirdSwap for Recorder {
    fn idle(&self) -> bool {
        !self.holding
    }
    fn hold(&mut self, key: &str) {
        self.holding = true;
        self.calls.push(format!("hold {key}"));
    }
    fn start(&mut self, file_load: bool) {
        self.calls.push(format!("start {file_load}"));
    }
    fn stop_cap(&mut self) {
        self.calls.push("stopCap".into());
    }
    fn cancel(&mut self) {
        self.holding = false;
        self.calls.push("cancel".into());
    }
}

/// The preview side of the third column, recording separately from the swap.
#[derive(Default)]
pub struct Preview {
    pub calls: Vec<String>,
    pub ready: bool,
}

impl Preview {
    pub fn count(&self, call: &str) -> usize {
        self.calls
            .iter()
            .filter(|seen| seen.as_str() == call)
            .count()
    }
}

impl PreviewProducer for Preview {
    fn ready(&self) -> bool {
        self.ready
    }
    fn follow_selection(&mut self) {
        self.calls.push("followSelection".into());
    }
    fn arm_settle(&mut self) {
        self.calls.push("armSettle".into());
    }
    fn ask_cursor_thumb(&mut self) {
        self.calls.push("askThumb".into());
    }
    fn ipc(&self, _: &str, _: &[String]) -> Option<String> {
        None
    }
}

/// The two producers a test keeps, lent to the area per call.
#[derive(Default)]
pub struct Producers {
    pub swap: Recorder,
    pub preview: Preview,
}

impl Producers {
    pub fn ports(&mut self) -> Ports<'_> {
        Ports {
            swap: &mut self.swap,
            preview: &mut self.preview,
        }
    }
}

pub const SETTINGS: PreviewSettings = PreviewSettings {
    column: true,
    automatic: true,
    manual_hold: false,
};

/// A 1200 by 800 area at the shipped limit and the default third.
pub fn area() -> Area {
    Area::new(1200., 800., None, 0., SETTINGS)
}

/// The paths every peek line in the outbox asks for, in order, and empties it.
pub fn drain_asks(area: &mut Area) -> Vec<String> {
    area.outbox
        .drain(..)
        .map(|line| {
            let doc = flea::jsondoc::parse(&line).expect("peek line is JSON");
            doc.get("path")
                .and_then(|path| path.as_str())
                .unwrap_or("")
                .to_owned()
        })
        .collect()
}

/// A peeked reply for this path under the default order, answering these rows.
pub fn reply(path: &str, rows: &[(&str, bool)]) -> Peeked {
    reply_with(path, rows, false, false, 512, "")
}

/// A peeked reply in wire form, so it parses through the same reader the transport uses; order is ",\"by\":..." or "".
pub fn reply_with(
    path: &str,
    rows: &[(&str, bool)],
    hidden: bool,
    hidden_last: bool,
    first: usize,
    order: &str,
) -> Peeked {
    let rows_json: Vec<String> = rows
        .iter()
        .map(|(name, dir)| {
            format!(
                "{{\"n\":\"{}\",\"d\":{dir},\"i\":\"{}\"}}",
                flea::json::escape(name),
                if *dir { "folder" } else { "text-plain" }
            )
        })
        .collect();
    let text = format!(
        "{{\"t\":\"peeked\",\"path\":\"{}\",\"hidden\":{hidden},\"hiddenLast\":{hidden_last},\"first\":{first}{order},\"n\":{},\"v\":1,\"w\":true,\"rows\":[{}]}}",
        flea::json::escape(path),
        rows.len(),
        rows_json.join(",")
    );
    let doc = flea::jsondoc::parse(&text).expect("reply is JSON");
    Peeked::from_json(&doc)
        .expect("a peeked reply")
        .expect("a well formed reply")
}

/// The built-in English words the list prints, with no Omarchy or language file to read.
pub fn words() -> flea_gui::row::format::Words {
    flea_gui::row::format::Words::from_catalog(&flea::i18n::I18n::load(flea::i18n::Paths {
        omarchy: "/nonexistent/omarchy".into(),
        language_file: "/nonexistent/language".into(),
        catalogs: Vec::new(),
    }))
}
