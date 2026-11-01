//! The Actions menu lifecycle (discovery, preview, run, refusals): one menu open sends one `actions list` with the menu's own snapshot, a preview names the cursor row alone, a choice sends `run`, the Jobs card owns every line after the handshake, and preview state belongs to preview rendering, never the menu.
use flea::jsondoc::{self, Json};
use flea::ui_logic_mn1::entry::Entry;
use flea::ui_logic_mn2::{plugins, rows, scripts, wire};

// A sentence the status bar says: a catalog key, with the backend's or OS diagnostic only ever as an argument, never as the sentence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Said {
    Key {
        key: &'static str,
        args: Vec<String>,
    },
    /// A failed script's own last output line, already a whole sentence (Scripts.qml Model.failure).
    Line(String),
}

// The line a chosen leaf sends: a preview names the cursor row, a run names the selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sent {
    Preview(String),
    Run(String),
}

// The template for a diagnostic no backend sentence covers: terminal launch errors, the preview's `symlink_metadata` reason, an unmapped listing error.
pub const START_KEY: &str = "actions.error.start";

// The run ids a window remembers for their refusals; an older run's late refusal is no longer said.
const HELD_RUNS: usize = 64;

#[derive(Clone, Debug, Default)]
pub struct Actions {
    next: usize,
    pending_list: Option<usize>,
    listing: Option<wire::Listing>,
    pending_preview: Option<usize>,
    preview: Option<String>,
    contributions: Vec<plugins::Contribution>,
    runs: Vec<(usize, usize)>,
    // Script runs by request id, then by the job id their start answered, so a failed job says the script's own line.
    script_runs: Vec<(usize, usize, String)>,
    script_jobs: Vec<(usize, usize, String)>,
    said: Vec<Said>,
}

impl Actions {
    // One menu open, one discovery request with the menu's own snapshot, which the host sends; selections wider than the backend answers skip discovery and stay quiet, and the preview is preview rendering's own and is left alone.
    pub fn menu_opened(&mut self, paths: Vec<String>) -> Option<String> {
        if paths.len() > wire::MAX_SELECTION {
            self.pending_list = None;
            self.listing = None;
            return None;
        }
        self.next += 1;
        let id = self.next;
        self.pending_list = Some(id);
        self.listing = None;
        Some(wire::list_request(id, &paths))
    }

    // The window's menu join sends discovery itself under its snapshot id once the backend has resolved the rows, so the wait is armed for that id.
    pub fn listing_requested(&mut self, id: usize) {
        self.pending_list = Some(id);
        self.listing = None;
    }

    // A new menu snapshot forgets the last menu's listing before anything is asked.
    pub fn listing_forgotten(&mut self) {
        self.pending_list = None;
        self.listing = None;
    }

    // The plugin host's latest enabled and healthy contributions.
    pub fn set_contributions(&mut self, contributions: Vec<plugins::Contribution>) {
        self.contributions = contributions;
    }

    // A choice on the Actions flyout: the run goes out under its own id, which the host sends.
    pub fn run(&mut self, leaf: &str, paths: Vec<String>) -> String {
        self.run_on(0, leaf, paths)
    }

    pub fn run_on(&mut self, side: usize, leaf: &str, paths: Vec<String>) -> String {
        self.next += 1;
        self.run_named_on(side, self.next, leaf);
        wire::run_request(self.next, leaf, &paths)
    }

    // A run under its leaf: a script leaf (script:NAME) is followed to its job, whose failure says the script's last line.
    pub fn run_named(&mut self, id: usize, leaf: &str) {
        self.run_named_on(0, id, leaf);
    }

    pub fn run_named_on(&mut self, side: usize, id: usize, leaf: &str) {
        self.run_sent_on(side, id);
        if let Some(name) = leaf.strip_prefix("script:") {
            hold(&mut self.script_runs, (side, id, name.to_owned()));
        }
    }

    // A run this window sent, under its id: mn2's own or mn1's runScript; only a held id's refusal is said.
    pub fn run_sent(&mut self, id: usize) {
        self.run_sent_on(0, id);
    }

    fn run_sent_on(&mut self, side: usize, id: usize) {
        hold(&mut self.runs, (side, id));
    }

    // A flyout choice on the Actions row: the leaf's mode, read off the listing this menu was built from, routes it (preview names the cursor row, run names the selection), and a leaf that listing no longer names is said stale and sends nothing.
    pub fn choose(&mut self, leaf: &str, paths: Vec<String>, cursor: String) -> Option<Sent> {
        self.choose_on(0, leaf, paths, cursor)
    }

    pub fn choose_on(
        &mut self,
        side: usize,
        leaf: &str,
        paths: Vec<String>,
        cursor: String,
    ) -> Option<Sent> {
        let previews = self
            .listing
            .as_ref()
            .and_then(|listing| rows::leaf_previews(listing, leaf));
        match previews {
            Some(true) => Some(Sent::Preview(self.preview(leaf, cursor))),
            Some(false) => Some(Sent::Run(self.run_on(side, leaf, paths))),
            None => {
                self.say(Some(wire::STALE_KEY), Vec::new(), "");
                None
            }
        }
    }

    // The plugin and action a chosen plugin row names, while the held contributions still offer it.
    pub fn plugin_choice(&self, action: &str) -> Option<(&str, &str)> {
        self.contributions
            .iter()
            .find(|contribution| plugins::leaf(contribution) == action)
            .map(|contribution| (contribution.plugin.as_str(), contribution.action.as_str()))
    }

    // A preview through preview rendering: the cursor row alone, explicit like a run.
    pub fn preview(&mut self, leaf: &str, cursor: String) -> String {
        self.next += 1;
        self.pending_preview = Some(self.next);
        self.preview = None;
        wire::preview_request(self.next, leaf, &[cursor])
    }

    // The preview wait ends explicitly, like the preview begins: closing a menu never drops a preview the menu does not own.
    pub fn preview_closed(&mut self) {
        self.pending_preview = None;
    }

    // Closing the menu ends the listing wait; a late reply lands on nothing.
    pub fn menu_closed(&mut self) {
        self.pending_list = None;
    }

    // Every backend line the host reads reaches here; mn2 keeps its own ids and ignores every other family.
    pub fn receive(&mut self, line: &str) {
        self.receive_on(0, line);
    }

    pub fn receive_on(&mut self, side: usize, line: &str) {
        if let Some(back) = wire::parse_listing(line) {
            if self.pending_list == Some(back.id) {
                self.pending_list = None;
                if back.error.is_empty() {
                    self.listing = Some(back.clone());
                    for error in &back.errors {
                        self.say_discovery(&error.file, &error.error);
                    }
                } else {
                    self.say_refusal(&back.error);
                }
            }
            return;
        }
        if let Some(preview) = wire::parse_preview(line) {
            if self.pending_preview == Some(preview.id) {
                self.pending_preview = None;
                if preview.ok {
                    self.preview = Some(preview.text);
                } else {
                    self.say_refusal(&preview.error);
                }
            }
            return;
        }
        if let Some(started) = wire::parse_started(line) {
            let held = self
                .runs
                .iter()
                .position(|&(origin, id)| origin == side && id == started.request);
            let script = self
                .script_runs
                .iter()
                .position(|&(origin, id, _)| origin == side && id == started.request)
                .map(|at| self.script_runs.remove(at).2);
            if let (Some(at), false) = (held, started.ok) {
                self.runs.remove(at);
                self.say_refusal(&started.error);
            } else if let (Some(_), Some(name)) = (held, script) {
                hold(&mut self.script_jobs, (side, started.id, name));
            }
            return;
        }
        self.job_failed(side, line);
    }

    // Sample: {"t":"job","job":{"id":40,"state":"failed","output":["no such page"],"reason":"Action exited with exit status: 2.","cancelled":false}}.
    fn job_failed(&mut self, side: usize, line: &str) {
        let Ok(doc) = jsondoc::parse(line) else {
            return;
        };
        if doc.get("t").and_then(Json::as_str) != Some("job") {
            return;
        }
        let Some(job) = doc.get("job") else {
            return;
        };
        let id = job.get("id").and_then(Json::as_f64).unwrap_or(0.0) as usize;
        let Some(at) = self
            .script_jobs
            .iter()
            .position(|&(origin, held, _)| origin == side && held == id)
        else {
            return;
        };
        let state = job.get("state").and_then(Json::as_str).unwrap_or("");
        if job.get("cancelled").and_then(Json::as_bool) == Some(true) || state == "done" {
            self.script_jobs.remove(at);
            return;
        }
        if state != "failed" {
            return;
        }
        let name = self.script_jobs.remove(at).2;
        let output: Vec<&str> = job
            .get("output")
            .and_then(Json::as_array)
            .map(|lines| lines.iter().filter_map(Json::as_str).collect())
            .unwrap_or_default();
        let reason = job.get("reason").and_then(Json::as_str).unwrap_or("");
        // Sample reason: "Action exited with exit status: 2."; the status is its last number.
        let code = reason
            .rsplit(|c: char| !c.is_ascii_digit())
            .find(|word| !word.is_empty())
            .and_then(|word| word.parse().ok())
            .unwrap_or(1);
        match scripts::failure(&name, &output.join("\n"), code) {
            scripts::Failure::Line(line) => self.said.push(Said::Line(line)),
            scripts::Failure::Exited { name, code } => self.said.push(Said::Key {
                key: scripts::EXITED_KEY,
                args: vec![name, code.to_string()],
            }),
        }
    }

    // A run, preview or listing refusal: the keyed sentence with its argument, so an exited status never renders as an unfilled hole.
    fn say_refusal(&mut self, error: &str) {
        let key = wire::refusal_key(error);
        let args = wire::refusal_args(error);
        self.say(key, args, error);
    }

    fn say(&mut self, key: Option<&'static str>, args: Vec<String>, error: &str) {
        match key {
            Some(key) => self.said.push(Said::Key { key, args }),
            None => self.said.push(Said::Key {
                key: START_KEY,
                args: vec![error.into()],
            }),
        }
    }

    // A discovery refusal off the list reply, said on the status bar: the keyed sentence where the backend names one, the syntax template otherwise.
    fn say_discovery(&mut self, file: &str, error: &str) {
        match wire::manifest_key(file, error) {
            Some((key, args)) => self.said.push(Said::Key { key, args }),
            None => self.said.push(Said::Key {
                key: wire::SYNTAX_KEY,
                args: vec![file.into(), error.into()],
            }),
        }
    }

    // The background menu's agent row; the file menu never asks for it.
    pub fn agent_entry(&self, hidden: &[String]) -> Option<Entry> {
        rows::agent_row(hidden)
    }

    // The file menu's Actions row, from the latest listing for this snapshot.
    pub fn actions_entry(&self, hidden: &[String]) -> Option<Entry> {
        rows::actions_row(hidden, self.listing.as_ref()?)
    }

    // The plugin rows for this menu, fail closed: only eligible contributions draw.
    pub fn plugin_entries(
        &self,
        hidden: &[String],
        background: bool,
        items: &[plugins::Item],
    ) -> Vec<Entry> {
        let shown: Vec<plugins::Contribution> =
            plugins::eligible(&self.contributions, background, items)
                .into_iter()
                .filter(|c| {
                    !hidden
                        .iter()
                        .any(|id| id == &plugins::leaf(c) || id == &c.plugin)
                })
                .cloned()
                .collect();
        rows::plugin_rows(&shown)
    }

    // The sentences the host has not shown yet; taking drains them, so nothing accumulates across menu opens.
    pub fn take_said(&mut self) -> Vec<Said> {
        std::mem::take(&mut self.said)
    }

    pub fn preview_text(&self) -> Option<&str> {
        self.preview.as_deref()
    }
}

// Keeps the newest HELD_RUNS entries, as the run ids are kept.
fn hold<T>(list: &mut Vec<T>, entry: T) {
    if list.len() == HELD_RUNS {
        list.remove(0);
    }
    list.push(entry);
}
