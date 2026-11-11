//! One serialized contents worker, with query generations independent of listing numbering.
#[path = "contentsparse.rs"]
pub(super) mod parse;
#[path = "contentsworker.rs"]
mod worker;
use super::events::{Event, Sender};
use crate::jsondoc::{self, Json};
use flea::ui_logic_file_search::ContentsMatch;
#[cfg(test)]
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread::JoinHandle;

pub(super) const MAX_WINDOW: usize = 4096;

#[derive(Clone)]
pub struct Ask {
    pub path: String,
    pub query: String,
    pub hidden: bool,
    pub gen: usize,
    pub count: usize,
}

impl Ask {
    // Sample input: {"c":"searchcontents","path":"/tmp/example","query":"needle","gen":1,"count":10}
    pub fn parse(line: &str) -> Result<Self, String> {
        let doc = jsondoc::parse(line)?;
        let text = |key| {
            doc.get(key)
                .and_then(Json::as_str)
                .map(str::to_owned)
                .ok_or_else(|| format!("invalid {key}"))
        };
        let integer = |key| match doc.get(key) {
            Some(Json::Num(n)) => n.parse::<usize>().map_err(|_| format!("invalid {key}")),
            _ => Err(format!("invalid {key}")),
        };
        let path = text("path")?;
        let query = text("query")?;
        let gen = integer("gen")?;
        let count = match doc.get("count") {
            None => super::searchsession::FIRST_WINDOW_DEFAULT,
            Some(_) => integer("count")?,
        };
        let hidden = match doc.get("hidden") {
            None => false,
            Some(Json::Bool(value)) => *value,
            _ => return Err("invalid hidden".into()),
        };
        if !flea::bytename::view(&path).is_absolute()
            || flea::bytename::is_refused(&path)
            || query.is_empty()
            || query.contains(['\0', '\n', '\r'])
            || gen == 0
            || count > MAX_WINDOW
        {
            return Err("invalid contents request".into());
        }
        Ok(Self {
            path,
            query,
            hidden,
            gen,
            count,
        })
    }
}

pub struct Snapshot {
    pub ask: Ask,
    pub matches: Vec<ContentsMatch>,
    pub scanned: usize,
    pub dev: u64,
    pub writable: bool,
    pub ms: f64,
    /// Some path under the root could not be read, so the matches cover only what could.
    pub partial: bool,
}

pub enum Outcome {
    Ready(Snapshot),
    Failed(String),
    Cancelled,
}
pub struct Done {
    pub ticket: u64,
    pub result: Outcome,
}

struct Live {
    ticket: u64,
    ask: Ask,
    stop: Arc<AtomicBool>,
    join: JoinHandle<()>,
}

#[derive(Default)]
pub struct Slot {
    ticket: u64,
    newest: usize,
    live: Option<Live>,
    pending: Option<Ask>,
    cancelled: Option<Ask>,
    scope: Option<Ask>,
    closing: bool,
    muted: bool,
    pub held: Option<Snapshot>,
    #[cfg(test)]
    pub probe: Arc<worker::Probe>,
    #[cfg(test)]
    pub program: Option<PathBuf>,
}

impl Slot {
    pub fn accepts(&self, gen: usize) -> bool {
        gen > self.newest
    }
    pub fn busy(&self) -> bool {
        self.live.is_some() || self.pending.is_some()
    }
    pub fn has_scope(&self) -> bool {
        self.scope.is_some()
    }

    pub fn submit(&mut self, ask: Ask, tx: &Sender<Event>) -> Result<(), String> {
        if ask.gen <= self.newest {
            return Ok(());
        }
        self.newest = ask.gen;
        self.closing = false;
        self.muted = false;
        self.cancelled = None;
        self.scope = Some(ask.clone());
        self.pending = Some(ask);
        if let Some(live) = &self.live {
            live.stop.store(true, Ordering::Release);
        }
        self.start(tx)
    }

    fn start(&mut self, tx: &Sender<Event>) -> Result<(), String> {
        if self.live.is_some() {
            return Ok(());
        }
        let Some(ask) = self.pending.take() else {
            return Ok(());
        };
        self.ticket += 1;
        let ticket = self.ticket;
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let work = ask.clone();
        let tx = tx.clone();
        #[cfg(test)]
        let probe = self.probe.clone();
        #[cfg(test)]
        let program = self.program.clone();
        let join = std::thread::Builder::new()
            .name("flea-contents".into())
            .spawn(move || {
                let result = worker::run(
                    &work,
                    &worker_stop,
                    #[cfg(test)]
                    program,
                    #[cfg(test)]
                    &probe,
                );
                let _ = tx.send(Event::Contents(Done { ticket, result }));
            })
            .map_err(|error| error.to_string())?;
        self.live = Some(Live {
            ticket,
            ask,
            stop,
            join,
        });
        Ok(())
    }

    pub fn cancel(&mut self, close: bool) -> Option<(Ask, bool)> {
        if self.closing {
            return None;
        }
        let pending = self.pending.take();
        if self.cancelled.is_none() {
            self.cancelled = pending.or_else(|| self.live.as_ref().map(|live| live.ask.clone()));
        }
        self.closing = close;
        if close {
            self.cancelled = self.scope.take().or_else(|| self.cancelled.take());
        }
        if let Some(live) = &self.live {
            live.stop.store(true, Ordering::Release);
            None
        } else {
            if close {
                self.newest = 0;
            }
            if close {
                self.cancelled.take().map(|ask| (ask, true))
            } else {
                None
            }
        }
    }

    pub fn finish(
        &mut self,
        done: Done,
        tx: &Sender<Event>,
    ) -> Result<Option<(Ask, Outcome, bool)>, String> {
        if !self
            .live
            .as_ref()
            .is_some_and(|live| live.ticket == done.ticket)
        {
            return Ok(None);
        }
        let live = self.live.take().expect("matched contents ticket");
        let _ = live.join.join();
        if self.pending.is_some() {
            self.start(tx)?;
            return Ok(None);
        }
        let cancelled = live.stop.load(Ordering::Acquire);
        let close = self.closing;
        if close {
            self.newest = 0;
        }
        if self.muted {
            return Ok(None);
        }
        Ok(Some((
            self.cancelled.take().unwrap_or(live.ask),
            if cancelled {
                Outcome::Cancelled
            } else {
                done.result
            },
            close,
        )))
    }

    pub fn clear(&mut self) {
        if let Some(live) = &self.live {
            live.stop.store(true, Ordering::Release);
        }
        self.pending = None;
        self.cancelled = None;
        self.scope = None;
        self.held = None;
        self.newest = 0;
        self.muted = true;
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.clear();
        if let Some(live) = self.live.take() {
            let _ = live.join.join();
        }
    }
}

pub fn message(key: &str, reason: &str) -> String {
    crate::i18n::I18n::load(crate::i18n::Paths::default()).tr_args(key, &[reason])
}
