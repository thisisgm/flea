// Network discovery, reimplemented from @nixfred's contribution in issue #32.
use super::events::Event;
use super::nearbyrows::{self, Row, SERVICES};
use super::nearbytool::Tool;
use crate::json::{escape, field_str};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};

pub const LIMIT: Duration = Duration::from_secs(3);
pub const LOOKING: &str = "";
pub const NOTHING: &str = "";
const STEP: Duration = Duration::from_millis(10);

enum Work {
    Start { id: usize, ticket: u64, deadline: Instant },
    Stop,
}

// No helper or thread exists until the dialog asks; closing joins group termination, never child reaping.
#[derive(Default)]
pub struct Discovery {
    worker: Option<Sender<Work>>,
    coordinator: Option<std::thread::JoinHandle<()>>,
    ticket: u64,
    active: Option<(usize, u64)>,
}

impl Discovery {
    pub fn request(&mut self, op: &str, id: usize, replies: crate::backend::events::Sender<Event>) -> String {
        self.request_with_path(op, id, replies, None)
    }

    fn request_with_path(&mut self, op: &str, id: usize, replies: crate::backend::events::Sender<Event>, path: Option<PathBuf>) -> String {
        match op {
            "start" => {
                self.ticket = self.ticket.wrapping_add(1);
                self.active = Some((id, self.ticket));
                let deadline = Instant::now() + LIMIT;
                if self.worker.is_none() {
                    let (tx, rx) = mpsc::channel();
                    self.coordinator = Some(std::thread::spawn(move || worker(rx, replies, path)));
                    self.worker = Some(tx);
                }
                if self.worker.as_ref().expect("discovery worker").send(Work::Start { id, ticket: self.ticket, deadline }).is_err() {
                    self.close();
                    return line(id, "empty", NOTHING, &[], &["Nearby discovery could not start.".into()]);
                }
                line(id, "looking", LOOKING, &[], &[])
            }
            "stop" => {
                if self.active.is_some_and(|(active, _)| active == id) {
                    self.active = None;
                    if let Some(worker) = &self.worker { let _ = worker.send(Work::Stop); }
                }
                line(id, "stopped", "", &[], &[])
            }
            _ => line(id, "error", "Unknown nearby operation.", &[], &[]),
        }
    }

    pub fn accept(&mut self, ticket: u64) -> bool {
        if self.active.is_some_and(|(_, active)| active == ticket) {
            self.active = None;
            true
        } else { false }
    }

    pub fn close(&mut self) {
        self.active = None;
        self.worker = None;
        if let Some(coordinator) = self.coordinator.take() { let _ = coordinator.join(); }
    }
}

impl Drop for Discovery {
    fn drop(&mut self) { self.close(); }
}

// Discovery lifecycle requests must bypass a directory scan's deferred row requests, especially stop.
pub fn is_request(line: &str) -> bool {
    (line.contains("nearby") || line.contains("networkaddress"))
        && matches!(field_str(line, "c").as_deref(), Some("nearby" | "networkaddress"))
}

fn worker(rx: Receiver<Work>, replies: crate::backend::events::Sender<Event>, path: Option<PathBuf>) {
    let mut next = None;
    loop {
        let command = match next.take().or_else(|| rx.recv().ok()) {
            Some(command) => command,
            None => return,
        };
        let Work::Start { id, ticket, deadline } = command else { continue; };
        // Coalesce rapid opens before spawning their six browsers and one status helper.
        match rx.try_recv() {
            Ok(newer) => { next = Some(newer); continue; }
            Err(TryRecvError::Disconnected) => return,
            Err(TryRecvError::Empty) => {}
        }
        let (mut tools, mut errors) = match start_tools(path.as_deref(), &rx, deadline) {
            Ok(tools) => tools,
            Err(Some(command)) => { next = Some(command); continue; }
            Err(None) => return,
        };
        // Completed results belong only to this scan and survive another helper's timeout.
        let mut rows = Vec::new();
        let mut closed = false;
        loop {
            match rx.try_recv() {
                Ok(command) => { next = Some(command); break; }
                Err(TryRecvError::Disconnected) => return,
                Err(TryRecvError::Empty) => {}
            }
            let timed_out = Instant::now() >= deadline;
            if !timed_out { tools.iter_mut().for_each(Tool::poll); }
            let ready = tools.iter().position(|tool| tool.service.is_none() && tool.ended())
                .or_else(|| tools.iter().position(Tool::ended));
            let completed: Vec<_> = if timed_out {
                // Kill every remaining group before processing any output at the cutoff.
                tools.drain(..).map(|mut tool| (tool.service, tool.finish(true))).collect()
            } else if let Some(index) = ready {
                let mut tool = tools.remove(index);
                vec![(tool.service, tool.finish(false))]
            } else { Vec::new() };
            for (service, (output, problem)) in completed {
                let succeeded = problem.is_none();
                if let Some(problem) = problem {
                    let name = service.unwrap_or("tailscale");
                    errors.push(format!("{name}: {problem}"));
                }
                if let Some(service) = service {
                    rows.extend(nearbyrows::avahi(&output, service));
                } else if succeeded {
                    let peers = nearbyrows::tailscale_checked(&output, &mut || {
                        if next.is_some() || closed { return false; }
                        match rx.try_recv() {
                            Ok(command) => next = Some(command),
                            Err(TryRecvError::Disconnected) => closed = true,
                            Err(TryRecvError::Empty) => {}
                        }
                        next.is_none() && !closed && Instant::now() < deadline
                    });
                    match peers {
                        Ok(peers) => rows.extend(peers),
                        Err(error) => errors.push(error.into()),
                    }
                }
                if next.is_some() || closed { break; }
            }
            if closed { return; }
            if next.is_some() { break; }
            if tools.is_empty() {
                let rows = nearbyrows::deduplicate(rows);
                errors.sort();
                errors.dedup();
                let (state, message) = if rows.is_empty() { ("empty", NOTHING) } else { ("found", "") };
                let _ = replies.send(Event::Nearby { ticket, line: line(id, state, message, &rows, &errors) });
                break;
            }
            // Recheck cancellation and the deadline before processing another completed source.
            if tools.iter().any(Tool::ended) || Instant::now() >= deadline { continue; }
            match rx.recv_timeout(STEP.min(deadline.saturating_duration_since(Instant::now()))) {
                Ok(command) => { next = Some(command); break; }
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    }
}

fn start_tools(path: Option<&std::path::Path>, rx: &Receiver<Work>, deadline: Instant) -> Result<(Vec<Tool>, Vec<String>), Option<Work>> {
    let mut tools = Vec::new();
    let mut errors = Vec::new();
    for service in SERVICES.into_iter().map(Some).chain([None]) {
        match rx.try_recv() {
            Ok(command) => return Err(Some(command)),
            Err(TryRecvError::Disconnected) => return Err(None),
            Err(TryRecvError::Empty) => {}
        }
        if Instant::now() >= deadline { break; }
        // -k prevents the service database from replacing protocol identifiers with display names.
        let result = if let Some(service) = service {
            Tool::spawn("avahi-browse", &["-p", "-r", "-t", "-k", "--", service], Some(service), path)
        } else {
            Tool::spawn("tailscale", &["status", "--json"], None, path)
        };
        match result {
            Ok(tool) => tools.push(tool),
            Err(error) => errors.push(error),
        }
    }
    Ok((tools, errors))
}

fn line(id: usize, state: &str, message: &str, rows: &[Row], errors: &[String]) -> String {
    let rows = rows.iter().map(Row::json).collect::<Vec<_>>().join(",");
    let errors = errors.iter().map(|s| format!("\"{}\"", escape(s))).collect::<Vec<_>>().join(",");
    format!(r#"{{"t":"nearby","id":{id},"state":"{state}","message":"{}","rows":[{rows}],"errors":[{errors}]}}"#, escape(message))
}

#[cfg(test)]
#[path = "nearby_tests.rs"]
mod tests;
