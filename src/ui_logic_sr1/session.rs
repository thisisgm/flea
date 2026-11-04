// The live session as ui/js/Search.js runs it: the strip opens a backend session, each key sends a newer gen, Esc returns.
use super::matching::{clean_paste, is_printable};
use super::types::{Effect, Key, Kind, ListingState, Mode, Pane, Scope, Stamps};
use super::wire::{Line, Request, Wire};

#[derive(Clone, Debug, Default)]
pub struct Session {
    pub wire: Wire,
    pub mode: Mode,
    pub kind: Kind,
    pub scope: Scope,
    pub query: String,
    pub gen: u64,
    pub opened_gen: u64,
    pub running: bool,
    pub cancelled: bool,
    pub leaving: bool,
    pub scanned: usize,
    pub partial: Option<String>,
    // The gen whose answer scanned, total, running, cancelled and partial now hold.
    pub answered: u64,
    pub ms: f64,
    pub total: usize,
    pub from: String,
    pub base: String,
    pub pending_select: String,
    pub stamps: Stamps,
}

pub(crate) fn join(base: &str, name: &str) -> String {
    if base == "/" {
        format!("/{name}")
    } else {
        format!("{base}/{name}")
    }
}

impl Session {
    pub fn new(wire: Wire) -> Self {
        Self { wire, ..Self::default() }
    }

    // The root a scope walks; Everywhere has no single root and no open shape at this base.
    pub fn scope_root(path: &str, home: &str, scope: Scope) -> Option<String> {
        let inside = !home.is_empty() && (path == home || path.strip_prefix(home).is_some_and(|rest| rest.starts_with('/')));
        match scope {
            Scope::Folder => Some(path.to_owned()),
            Scope::Home if inside => Some(home.to_owned()),
            Scope::Home => Some(path.to_owned()),
            Scope::Everywhere => None,
        }
    }

    // Tab's order: this folder, Home, everywhere, and everywhere only once its producer is wired.
    pub fn next_scope(&self) -> Scope {
        match self.scope {
            Scope::Folder => Scope::Home,
            Scope::Home if self.wire.everywhere => Scope::Everywhere,
            Scope::Home | Scope::Everywhere => Scope::Folder,
        }
    }

    // f opens the session with no rows touched; from Recent it leaves first, so the scope is the real folder.
    pub fn start(&mut self, pane: &Pane) -> Vec<Effect> {
        let mut out = Vec::new();
        match self.mode {
            Mode::Results => {
                self.mode = Mode::Typing;
                return out;
            }
            Mode::Typing => return out,
            Mode::Off => {}
        }
        self.pending_select = pane.cursor_name.filter(|name| !name.is_empty()).map(|name| join(pane.path, name)).unwrap_or_default();
        let mut path = pane.path.to_owned();
        if let Some(from) = pane.recent {
            // The history rows keep their own base until the folder's rows, asked below, replace them.
            self.base = pane.path.to_owned();
            if !from.is_empty() {
                path = from.to_owned();
            }
            out.push(Effect::LeaveRecent { path: path.clone() });
        }
        self.from.clone_from(&path);
        self.mode = Mode::Typing;
        let pane = Pane { path: &path, ..*pane };
        self.open_scope(&pane, &mut out);
        if pane.recent.is_some() {
            self.send_query(&pane, &mut out);
        }
        out
    }

    // The backend session with the drawn state of a fresh walk; gens keep rising, so a closed session's late line never matches.
    fn open_scope(&mut self, pane: &Pane, out: &mut Vec<Effect>) {
        self.opened_gen = self.gen;
        out.push(Effect::ResetLanded);
        self.query.clear();
        self.running = true;
        self.cancelled = false;
        self.scanned = 0;
        self.partial = None;
        self.answered = 0;
        self.send_open(pane, out);
    }

    fn send_open(&self, pane: &Pane, out: &mut Vec<Effect>) {
        if let Some(scope) = Self::scope_root(pane.path, pane.home, self.scope) {
            out.push(Effect::Send(Request::Open { scope, folder: pane.path.to_owned(), hidden: pane.hidden }));
        }
    }

    // A contents strip asks the contents producer over the scope root; a names strip asks the names walk.
    fn send_query(&mut self, pane: &Pane, out: &mut Vec<Effect>) {
        let root = Self::scope_root(pane.path, pane.home, self.scope);
        // Everywhere has no single root for the producer at this base, so a contents query is not sent.
        if self.kind == Kind::Contents && root.is_none() {
            return;
        }
        self.gen += 1;
        self.stamps = Stamps { gen: self.gen, key_at: pane.now.max(1), first_rows_at: 0, settled_at: 0 };
        self.partial = None;
        let query = self.query.clone();
        out.push(Effect::Send(match root {
            Some(path) if self.kind == Kind::Contents => Request::Contents { path, query, hidden: pane.hidden, gen: self.gen, count: pane.count },
            _ => Request::Query { query, gen: self.gen, count: pane.count, sizes: pane.sizes },
        }));
    }

    pub fn extend(&mut self, pane: &Pane, text: &str) -> Vec<Effect> {
        let mut out = Vec::new();
        self.query.push_str(text);
        self.send_query(pane, &mut out);
        out
    }

    // Backspace to empty sends "" and the folder rows come back, with the strip still open.
    pub fn shrink(&mut self, pane: &Pane) -> Vec<Effect> {
        let mut out = Vec::new();
        if self.query.pop().is_some() {
            self.send_query(pane, &mut out);
        }
        out
    }

    // Enter keeps the results and moves the keys to them, with no new request.
    pub fn commit(&mut self, pane: &Pane) -> Vec<Effect> {
        if self.query.is_empty() {
            return self.abort(pane);
        }
        self.mode = Mode::Results;
        Vec::new()
    }

    pub fn takes_query(&self) -> bool {
        !self.leaving
    }

    // A query set from outside the strip, such as a tag place, lands as results over the Home scope.
    pub fn open_query(&mut self, pane: &Pane, query: &str) -> Vec<Effect> {
        let mut out = Vec::new();
        if !self.takes_query() {
            return out;
        }
        let mut path = pane.path.to_owned();
        if self.mode == Mode::Off {
            self.scope = Scope::Home;
            out = self.start(pane);
            path.clone_from(&self.from);
        } else if self.cancelled {
            self.scope = Scope::Home;
            self.open_scope(pane, &mut out);
        } else if self.scope != Scope::Home {
            // The old query is dropped first, so the rescope sends nothing and the new query is the one request.
            self.query.clear();
            self.scope = Scope::Home;
            self.send_open(pane, &mut out);
        }
        self.query = query.to_owned();
        self.send_query(&Pane { path: &path, ..*pane }, &mut out);
        out.extend(self.commit(pane));
        out
    }

    // Tab moves to the next scope, opens it at once and re-sends the standing query.
    pub fn rescope(&mut self, pane: &Pane) -> Vec<Effect> {
        // Landed search rows use their scope root; rescope keeps the folder that opened the search.
        let from = self.from.clone();
        let pane = &Pane { path: if from.is_empty() { pane.path } else { &from }, ..*pane };
        let mut out = Vec::new();
        self.scope = self.next_scope();
        self.send_open(pane, &mut out);
        if !self.query.is_empty() {
            self.send_query(pane, &mut out);
        }
        out
    }

    // Ctrl+G turns the strip between names and contents once the contents producer is wired.
    pub fn toggle_contents(&mut self, pane: &Pane) -> Vec<Effect> {
        if !self.wire.contents {
            return Vec::new();
        }
        self.kind = if self.kind == Kind::Names { Kind::Contents } else { Kind::Names };
        let mut out = Vec::new();
        if !self.query.is_empty() {
            self.send_query(pane, &mut out);
        }
        out
    }

    // Ctrl+G in the listing opens the strip on this folder already in contents; an open strip turns as Ctrl+G inside it.
    pub fn start_contents(&mut self, pane: &Pane) -> Vec<Effect> {
        if !self.wire.contents {
            return Vec::new();
        }
        if self.mode != Mode::Off {
            return self.toggle_contents(pane);
        }
        self.kind = Kind::Contents;
        self.scope = Scope::Folder;
        self.start(pane)
    }

    // Esc stops a running walk first, then leaves: at once when nothing was sent, else by re-listing the folder.
    pub fn abort(&mut self, pane: &Pane) -> Vec<Effect> {
        if self.leaving {
            return Vec::new();
        }
        if self.running && !self.query.is_empty() {
            return vec![Effect::Send(Request::Cancel)];
        }
        let back = if self.from.is_empty() { pane.path.to_owned() } else { self.from.clone() };
        if !self.owes_relist() {
            let out = self.close_session();
            self.pending_select.clear();
            return out;
        }
        let mut out = self.leave_walk();
        out.push(Effect::Relist { path: back, select: self.pending_select.clone() });
        out
    }

    // Any gen this session sent may have drawn rows the folder does not own, so leaving it re-lists the folder.
    pub fn owes_relist(&self) -> bool {
        self.mode != Mode::Off && self.gen > self.opened_gen
    }

    // A navigation drops the session; results on screen keep their strip until the next listing lands.
    pub fn leave_walk(&mut self) -> Vec<Effect> {
        if self.mode == Mode::Off || self.leaving {
            return Vec::new();
        }
        if self.live() {
            self.leaving = true;
            return vec![Effect::Send(Request::Close)];
        }
        self.close_session()
    }

    // Closing is one searchclose; a list closes the session too, so a doubled close is silence.
    pub fn close_session(&mut self) -> Vec<Effect> {
        if self.mode == Mode::Off {
            return Vec::new();
        }
        self.clear();
        vec![Effect::Send(Request::Close)]
    }

    fn clear(&mut self) {
        self.leaving = false;
        self.mode = Mode::Off;
        self.kind = Kind::Names;
        self.query.clear();
        self.running = false;
        self.cancelled = false;
        self.scanned = 0;
        self.partial = None;
        self.answered = 0;
        self.from.clear();
        self.base.clear();
    }

    // An exit landing clears the drawn search state in the same turn its rows do; the backend already closed.
    pub fn close_leaving(&mut self) {
        self.clear();
    }

    // A terminal line answers only the newest gen this session sent; older gens drop with anything gen-less.
    pub fn accepts(&self, gen: u64) -> bool {
        self.mode != Mode::Off && gen == self.gen && gen > self.opened_gen
    }

    // The answered counts land; the first window rides its own rows line, so nothing is asked for.
    pub fn searched(&mut self, line: &Line, now: u64) -> Vec<Effect> {
        let Line::Searched { n: total, scanned, ms, cancelled, gen, walking, ref partial } = *line else { return Vec::new() };
        if self.leaving || !self.accepts(gen) {
            return Vec::new();
        }
        self.running = walking;
        self.cancelled = cancelled;
        if !walking {
            self.stamps.settled_at = now.max(1);
        }
        self.total = total;
        self.scanned = scanned;
        self.partial.clone_from(partial);
        self.answered = gen;
        self.ms = ms;
        vec![Effect::RestartSettle]
    }

    // A contents failure lands its gen as an empty finished walk: the drawn rows stay and the strip stops scanning.
    pub fn contents_failed(&mut self, gen: u64, now: u64) -> Vec<Effect> {
        if self.kind != Kind::Contents { return Vec::new(); }
        self.searched(&Line::Searched { n: 0, scanned: 0, ms: 0., cancelled: false, gen, walking: false, partial: None }, now)
    }

    // The first rows of the newest gen landed, the stopwatch's middle stamp.
    pub fn rows_landed(&mut self, gen: u64, now: u64) {
        if self.accepts(gen) && self.stamps.first_rows_at == 0 {
            self.stamps.first_rows_at = now.max(1);
        }
    }

    // Query-line keys: Enter keeps, Esc leaves, Tab rescopes, printables extend; keys during an exit wait for the folder.
    pub fn type_key(&mut self, pane: &Pane, key: Key) -> Vec<Effect> {
        if self.leaving {
            return Vec::new();
        }
        match key {
            Key::Paste(text) => {
                let pasted = clean_paste(text.unwrap_or(""));
                if pasted.is_empty() {
                    Vec::new()
                } else {
                    self.extend(pane, &pasted)
                }
            }
            Key::Enter => self.commit(pane),
            Key::Escape => self.abort(pane),
            Key::Backspace => self.shrink(pane),
            Key::Tab => self.rescope(pane),
            Key::Contents => self.toggle_contents(pane),
            Key::Text(text) if is_printable(text) => self.extend(pane, text),
            Key::Text(_) | Key::Other => Vec::new(),
        }
    }

    // o on a result opens the folder that holds it with the cursor on the row, the design's reveal.
    pub fn reveal(&mut self, pane: &Pane, row: Option<&str>) -> Vec<Effect> {
        let Some(name) = row else { return Vec::new() };
        let full = join(self.base_of(pane.path), name);
        let cut = match full.rfind('/') {
            Some(cut) if cut > 0 => cut,
            _ => return Vec::new(),
        };
        let mut out = Vec::new();
        if pane.recent.is_some() {
            out.push(Effect::LeaveRecent { path: pane.path.to_owned() });
        }
        self.pending_select.clone_from(&full);
        out.push(Effect::Reveal { folder: full[..cut].to_owned(), select: full });
        out
    }

    // Rows on screen belong to a standing query: results in, folder out, whatever mode holds them.
    pub fn live(&self) -> bool {
        self.mode != Mode::Off && !self.query.is_empty()
    }

    // What a pointer activation means right now: a result reveals, any other row opens.
    pub fn reveals_on_activate(&self) -> bool {
        self.live()
    }

    // A walk with no matches yet is still working, so the area keeps the crawl rather than flashing empty.
    pub fn listing_state(&self, total: usize) -> ListingState {
        match (total > 0, self.running) {
            (true, _) => ListingState::Ready,
            (false, true) => ListingState::Loading,
            (false, false) => ListingState::Empty,
        }
    }

    // Row facilities join a row against the listing's base, the scope or the folder, never the pane's path mid-session.
    pub fn base_of<'a>(&'a self, path: &'a str) -> &'a str {
        if self.base.is_empty() { path } else { &self.base }
    }

    // A live re-answer keeps the cursor's file when the new window still names it, else the window start.
    pub fn kept_index<'n>(name: Option<&str>, names: impl IntoIterator<Item = &'n str>, start: usize) -> usize {
        let Some(name) = name else { return start };
        names.into_iter().position(|row| row == name).map_or(start, |at| start + at)
    }
}
