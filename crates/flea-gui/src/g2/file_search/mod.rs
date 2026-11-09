//! Search: the query strip, its live backend session and the result presentation, over the core session.
pub mod contents;
pub(crate) mod input;
pub mod matches;
pub mod presented;
mod query;
pub mod run;
pub mod strip;

#[cfg(test)]
mod contents_tests;
#[cfg(test)]
mod fixlogic_ey505_tests;
#[cfg(test)]
mod pane_tests;
#[cfg(test)]
mod presented_tests;
#[cfg(test)]
mod strip_tests;
#[cfg(test)]
mod tests;

use flea::{
    i18n::I18n,
    ipc::wire::Reply,
    ui_logic_file_search::{self as core, Effect, Key, Kind, Line, Mode, Pane, Session, Wire},
};
use gpui::KeyDownEvent;

/// What the controller asks of the window's pane; G1 implements it at the join.
pub trait Host {
    fn pane(&self) -> HostPane<'_>;
    /// One request line to the pane's backend, written as is.
    fn send(&mut self, line: String);
    fn leave_recent(&mut self, path: &str);
    fn reset_landed(&mut self);
    /// Open `path` without history, holding the walk's drawn state until its rows land, the cursor on `select`.
    fn relist(&mut self, path: &str, select: &str);
    /// Open the folder holding a result with the cursor on it.
    fn reveal(&mut self, folder: &str, select: &str);
    fn restart_settle(&mut self);
    fn clipboard_text(&self) -> Option<String>;
    /// The pane's copy of this session, which its generation filter reads (`pane.swap.session`); only `Search` writes it.
    fn publish(&mut self, session: crate::pane::swap::Session);
}

/// The pane facts the session reads, borrowed from the host for one call.
#[derive(Clone, Copy, Debug)]
pub struct HostPane<'a> {
    pub path: &'a str,
    pub home: &'a str,
    pub hidden: bool,
    pub count: usize,
    /// The committed listing still supplying the drawn rows.
    pub listing: usize,
    pub sizes: usize,
    pub cursor_name: Option<&'a str>,
    pub recent: Option<&'a str>,
    pub now: u64,
}

impl<'a> From<HostPane<'a>> for Pane<'a> {
    fn from(pane: HostPane<'a>) -> Self {
        Pane {
            path: pane.path,
            home: pane.home,
            hidden: pane.hidden,
            count: pane.count,
            sizes: pane.sizes,
            cursor_name: pane.cursor_name,
            recent: pane.recent,
            now: pane.now,
        }
    }
}

/// One pane's search: the core session plus the exit's relist mark.
#[derive(Default)]
pub struct Search {
    pub session: Session,
    pub entry: std::rc::Rc<std::cell::RefCell<crate::path::entry::Entry>>,
    pub presented: presented::Presented,
    /// The contents producer's match lines, held until the rows of their gen and listing present.
    pub matches: matches::Matches,
    relisting: bool,
}

impl Search {
    pub fn new(wire: Wire) -> Self {
        Self {
            session: Session::new(wire),
            entry: Default::default(),
            presented: presented::Presented::default(),
            matches: matches::Matches::default(),
            relisting: false,
        }
    }

    pub fn active(&self) -> bool {
        self.session.mode != Mode::Off
    }

    pub fn typing(&self) -> bool {
        self.session.mode == Mode::Typing && !self.session.leaving
    }

    /// The listing actions this unit owns, from the generated key table and pointer bindings.
    pub fn action(&mut self, name: &str, host: &mut dyn Host) -> bool {
        let effects = {
            let pane = host.pane();
            match name {
                "search" => self.session.start(&pane.into()),
                "searchContents" => self.session.start_contents(&pane.into()),
                "reveal" => self.session.reveal(&pane.into(), pane.cursor_name),
                _ => return false,
            }
        };
        self.apply(effects, host);
        true
    }

    /// A key while the query line has the keyboard; every key is taken, as the strip owns them all.
    pub fn key(&mut self, key: StripKey, host: &mut dyn Host) {
        if self.query_key(&key, host) {
            return;
        }
        let clip = matches!(key, StripKey::Paste)
            .then(|| host.clipboard_text())
            .flatten();
        let effects = {
            let pane: Pane = host.pane().into();
            let key = match key {
                StripKey::Enter => Key::Enter,
                StripKey::Escape => Key::Escape,
                StripKey::Backspace => Key::Backspace,
                StripKey::Tab => Key::Tab,
                StripKey::Contents => Key::Contents,
                StripKey::Paste => Key::Paste(clip.as_deref()),
                StripKey::Text(ref text) => Key::Text(text),
                StripKey::Other => Key::Other,
            };
            self.session.type_key(&pane, key)
        };
        self.apply(effects, host);
    }

    /// A click on the scope words cycles the scope the way Tab does.
    pub fn click_scope(&mut self, host: &mut dyn Host) {
        self.commit_scope_draft();
        let effects = self.session.rescope(&host.pane().into());
        self.apply(effects, host);
    }

    /// A query set from outside the strip, a tag place among them.
    pub fn open_query(&mut self, query: &str, host: &mut dyn Host) {
        let effects = self.session.open_query(&host.pane().into(), query);
        self.apply(effects, host);
    }

    /// Back leaves the search immediately; Escape retains its separate stop-then-return behavior.
    pub fn back(&mut self, host: &mut dyn Host) {
        let path = self.session.from.clone();
        let select = self.session.pending_select.clone();
        let mut effects = self.session.leave_walk();
        effects.push(Effect::Relist { path, select });
        self.apply(effects, host);
    }

    /// Any navigation the pane makes drops the session first; results keep their strip until the next listing lands.
    pub fn leave(&mut self, host: &mut dyn Host) {
        let effects = self.session.leave_walk();
        self.apply(effects, host);
    }

    /// A search's own reply line; listed and rows ride the pane's generation path instead.
    pub fn line(&mut self, line: &str, host: &mut dyn Host) -> Result<bool, String> {
        let Some(parsed) = core::parse_line(line)? else {
            return Ok(false);
        };
        match parsed {
            Line::Searched { .. } => {
                let now = host.pane().now;
                let effects = self.session.searched(&parsed, now);
                self.apply(effects, host);
            }
            // Only a contents strip's newest gen holds lines; a stale or leaving batch is dropped whole.
            Line::ContentsMatches(batch)
                if self.session.kind == Kind::Contents && !self.session.leaving =>
            {
                self.matches
                    .take(batch, self.session.gen, host.pane().listing);
            }
            _ => {}
        }
        Ok(true)
    }

    /// A contents failure line: true when it answered the newest gen, which then stops scanning.
    pub fn contents_failed(&mut self, line: &str, host: &mut dyn Host) -> bool {
        let gen = flea::json::field_usize(line, "gen").unwrap_or(0) as u64;
        let effects = self.session.contents_failed(gen, host.pane().now);
        let settled = !effects.is_empty();
        self.apply(effects, host);
        settled
    }

    /// The newest gen's first rows were drawn.
    pub fn rows_drawn(&mut self, gen: usize, now: u64) {
        self.session.rows_landed(gen as u64, now);
    }

    /// The exit's folder landed: the drawn search state clears in the same turn as its rows.
    pub fn landed(&mut self, host: &mut dyn Host) {
        if self.session.leaving {
            self.session.close_leaving();
        }
        self.relisting = false;
        self.apply(Vec::new(), host);
    }

    /// The exit's own relist died, so no cursor file stays armed; a later listing's failure keeps its own.
    pub fn listing_failed(&mut self, host: &mut dyn Host) {
        if std::mem::take(&mut self.relisting) {
            self.session.pending_select.clear();
            if self.session.leaving {
                self.session.close_leaving();
            }
        }
        self.apply(Vec::new(), host);
    }

    pub fn relisting(&self) -> bool {
        self.relisting
    }

    fn apply(&mut self, effects: Vec<Effect>, host: &mut dyn Host) {
        for effect in effects {
            match effect {
                Effect::Send(request) => host.send(request.line()),
                Effect::LeaveRecent { path } => host.leave_recent(&path),
                Effect::ResetLanded => host.reset_landed(),
                Effect::Relist { path, select } => {
                    self.relisting = true;
                    host.relist(&path, &select);
                }
                Effect::Reveal { folder, select } => {
                    // A reveal is a navigation, which leaves the walk before the folder opens.
                    for close in self.session.leave_walk() {
                        if let Effect::Send(request) = close {
                            host.send(request.line());
                        }
                    }
                    host.reveal(&folder, &select);
                }
                Effect::RestartSettle => host.restart_settle(),
            }
        }
        // A closed session holds no lines; a running one keeps the drawn listing's until its next rows present.
        if self.session.mode == Mode::Off {
            self.matches.clear();
        }
        self.sync_entry();
        host.publish(self.pane_session());
    }

    /// The pane's generation filter copy, derived from the one core session after every change.
    pub fn pane_session(&self) -> crate::pane::swap::Session {
        let session = &self.session;
        crate::pane::swap::Session {
            open: session.mode != Mode::Off,
            leaving: session.leaving,
            generation: session.gen as usize,
            opened_generation: session.opened_gen as usize,
            query: session.query.clone(),
        }
    }

    /// The strip's drawn words, translated once per change and never in paint.
    pub fn words(&self, i18n: &I18n, path: &str, home: &str, shares_skipped: bool) -> strip::Words {
        let session = &self.session;
        let empty = session.query.is_empty();
        let path = if session.from.is_empty() {
            path
        } else {
            &session.from
        };
        let root = Session::scope_root(path, home, session.scope);
        strip::Words {
            query: if self.typing() && self.entry.borrow().editing {
                self.entry.borrow().text.clone()
            } else {
                session.query.clone()
            },
            scope: core::scope_label(i18n, root.as_deref(), home, session.scope, session.kind),
            note: core::note(i18n, session.total, session.running, empty, shares_skipped),
            way_out: core::way_out(i18n, session.running, empty),
            typing: self.typing(),
        }
    }

    /// The status bar's search half and the no-match headline, both empty when they say nothing.
    pub fn status(&self, i18n: &I18n) -> (String, String) {
        let session = &self.session;
        if !session.live() {
            return (String::new(), String::new());
        }
        let line = core::status_line(
            i18n,
            session.running,
            session.total,
            session.scanned,
            session.ms,
        );
        let headline = if session.total == 0 && !session.running {
            core::nothing_matches(i18n, &session.query)
        } else {
            String::new()
        };
        (line, headline)
    }
}

/// The match line Quick Look opens at: its number, its text and the typed run in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchLine {
    pub number: usize,
    pub text: String,
    pub run: Option<(usize, usize)>,
}

impl Search {
    pub fn match_line(&self, listing: usize, row: usize) -> Option<MatchLine> {
        let hit = self.matches.line(listing, row)?;
        Some(MatchLine {
            number: hit.line_no,
            text: hit.line.clone(),
            run: run::run_in(&hit.line, &self.session.query),
        })
    }

    /// Contents actions require the producer's exact identity; ordinary rows have no contents metadata.
    pub fn operable(&self, listing: usize, row: usize) -> bool {
        self.matches.line(listing, row).map_or_else(
            || !(self.active() && self.session.kind == Kind::Contents),
            |hit| hit.operable,
        )
    }
}

/// A platform key read for the query line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StripKey {
    Enter,
    Escape,
    Backspace,
    Tab,
    Contents,
    Paste,
    Text(String),
    Other,
}

impl StripKey {
    /// Ctrl+V and Shift+Insert paste, Ctrl+G turns contents, Tab and Shift+Tab rescope, a typed character extends.
    pub fn from_event(event: &KeyDownEvent) -> Self {
        let keystroke = &event.keystroke;
        let held = keystroke.modifiers;
        let only_control = held.control && !held.shift && !held.alt && !held.platform;
        let only_shift = held.shift && !held.control && !held.alt && !held.platform;
        match keystroke.key.as_str() {
            "v" if only_control => Self::Paste,
            "insert" if only_shift => Self::Paste,
            "g" if only_control => Self::Contents,
            "enter" => Self::Enter,
            "escape" => Self::Escape,
            "backspace" => Self::Backspace,
            "tab" => Self::Tab,
            _ if held.control || held.alt || held.platform => Self::Other,
            _ => keystroke
                .key_char
                .as_ref()
                .map_or(Self::Other, |text| Self::Text(text.clone())),
        }
    }
}

/// flea.searchStamps: the newest gen's stopwatch, answer and presented frames, read-only.
pub struct SearchAdapter<'a> {
    pub search: &'a Search,
}

impl crate::ipc::Adapter for SearchAdapter<'_> {
    fn call(&self, function: &str, _: &[String]) -> Option<Reply> {
        (function == "searchStamps")
            .then(|| Reply::value(self.search.presented.stamps_json(&self.search.session)))
    }
}
