// The code preview: bat colours the asked lines inside the readonly jail, sgr.rs turns its output into role runs, and one request gets one whole reply or none.
use crate::backend::opsreq::OpMsg;
use crate::backend::regfile::open_regular;
use crate::backend::sgr::{self, Line};
use crate::backend::{jail, sandbox};
use crate::json::{escape, field_bool, field_str, field_usize};
use std::borrow::Cow;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::Duration;

#[path = "csvrange.rs"]
mod csvrange;

// Past this many bytes a file is too large to colour.
const LARGE_BYTES: usize = 1_048_576;
// A NUL among the first bytes makes the file binary.
const NUL_WINDOW: usize = 1024;
// A longer line shows the file as plain text, since colouring one costs more than it shows.
const LONG_LINE: usize = 5000;
const MAX_COUNT: usize = 400;
// Only the path where the jail has not exited yet polls at all, so this is that case's step.
const REAP_STEP: Duration = Duration::from_millis(1);
const BAT_FLAGS: [&str; 7] = ["--no-config", "--no-custom-assets", "--theme=ansi", "--color=always", "--style=plain", "--paging=never", "--wrap=never"];

pub struct Ask {
    token: usize,
    slot: String,
    source: Source,
    first: usize,
    count: usize,
    raw: bool,
    csv: Option<u8>,
    record: Option<usize>,
}

enum Source {
    File(String),
    // Bytes the client already holds; name picks the language when lang is empty.
    Text { text: String, name: String, lang: String },
}

impl Ask {
    // Sample input: {"c":"code","token":4,"slot":"look","path":"/home/gm/a.rs","first":40,"count":20}
    // and {"c":"code","token":5,"slot":"fence:2","text":"fn main() {}","name":"x.rs","lang":"","first":1,"count":10}.
    pub fn parse(line: &str) -> Ask {
        let source = match field_str(line, "text") {
            Some(text) => {
                let lang = field_str(line, "lang").unwrap_or_default();
                // A Markdown info string names the language in its first word.
                let lang = lang.split_whitespace().next().unwrap_or("").to_string();
                Source::Text { text, name: field_str(line, "name").unwrap_or_default(), lang }
            }
            None => Source::File(field_str(line, "path").unwrap_or_default()),
        };
        Ask {
            token: field_usize(line, "token").unwrap_or(0),
            slot: field_str(line, "slot").unwrap_or_default(),
            source,
            first: field_usize(line, "first").unwrap_or(1).max(1),
            // Absent is the whole text, which the cap then clamps.
            count: field_usize(line, "count").unwrap_or(MAX_COUNT).clamp(1, MAX_COUNT),
            raw: field_bool(line, "raw"),
            csv: csvrange::delimiter(line),
            record: field_usize(line, "record").map(|n| n.max(1)),
        }
    }

    // None when the window would end past usize::MAX, which a client can ask for and no file reaches.
    fn range(&self) -> Option<String> {
        Some(format!("{}:{}", self.first, self.first.checked_add(self.count - 1)?))
    }
}

// The jobs of one slot: a newer request on it ends the older job and the older reply is never sent.
enum Run {
    Idle,
    Live(jail::Jailed),
    Cancelled,
}

type Table = Mutex<HashMap<String, (u64, Arc<Mutex<Run>>)>>;
static SLOTS: OnceLock<Table> = OnceLock::new();

fn table() -> &'static Table {
    SLOTS.get_or_init(Default::default)
}

fn held<T>(lock: &Mutex<T>) -> MutexGuard<'_, T> {
    lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub struct Ticket {
    slot: String,
    seq: u64,
    run: Arc<Mutex<Run>>,
}

// Registers the request on its slot at once, in the order requests come in. The older job is marked cancelled here and its jail is ended off the loop, because ending a jail waits for it to go.
pub(super) fn begin(slot: &str) -> Ticket {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let seq = NEXT.fetch_add(1, Ordering::Relaxed);
    let run = Arc::new(Mutex::new(Run::Idle));
    let older = held(table()).insert(slot.to_string(), (seq, Arc::clone(&run)));
    if let Some((_, older)) = older {
        if let Run::Live(mut jailed) = std::mem::replace(&mut *held(&older), Run::Cancelled) {
            std::thread::spawn(move || jail::kill_tree(&mut jailed));
        }
    }
    Ticket { slot: slot.to_string(), seq, run }
}

pub(super) struct Cancellation(Arc<Mutex<Run>>);

impl Cancellation {
    pub(super) fn cancel(self) {
        let previous = std::mem::replace(&mut *held(&self.0), Run::Cancelled);
        if let Run::Live(mut jailed) = previous { jail::kill_tree(&mut jailed); }
    }
}

impl Ticket {
    pub(super) fn cancellation(&self) -> Cancellation { Cancellation(Arc::clone(&self.run)) }

    // A synchronous preview caller has no wire reply, but must release its completed slot.
    pub(super) fn discard(self) {
        let mut slots = held(table());
        if slots.get(&self.slot).is_some_and(|(seq, _)| *seq == self.seq) { slots.remove(&self.slot); }
    }

    pub(super) fn superseded(&self) -> bool {
        matches!(*held(&self.run), Run::Cancelled)
    }

    // Hands the running jail over so a newer request can end it; false when that already happened, and the jail is ended here.
    pub(super) fn attach(&self, mut jailed: jail::Jailed) -> bool {
        let mut run = held(&self.run);
        if matches!(*run, Run::Cancelled) {
            jail::kill_tree(&mut jailed);
            return false;
        }
        *run = Run::Live(jailed);
        true
    }

    // Polls under the lock the older job's end kills under, so a kill never lands on a pid already reaped; None when cancelled.
    fn reap(&self) -> Option<std::io::Result<ExitStatus>> {
        self.reap_until(None)
    }

    pub(super) fn reap_until(&self, deadline: Option<std::time::Instant>) -> Option<std::io::Result<ExitStatus>> {
        loop {
            {
                let mut run = held(&self.run);
                match &mut *run {
                    Run::Live(jailed) => match jailed.child.try_wait() {
                        Ok(None) if deadline.is_some_and(|end| std::time::Instant::now() >= end) => {
                            jail::kill_tree(jailed);
                            *run = Run::Idle;
                            return Some(Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "preview timed out")));
                        }
                        Ok(None) => {}
                        done => {
                            *run = Run::Idle;
                            return done.transpose();
                        }
                    },
                    _ => return None,
                }
            }
            std::thread::sleep(REAP_STEP);
        }
    }

    // The send and the slot check share one lock with begin, so a reply is either sent before a newer request registers or dropped.
    pub(super) fn finish(self, line: String, tx: &Sender<OpMsg>) {
        let mut slots = held(table());
        if slots.get(&self.slot).is_some_and(|(seq, _)| *seq == self.seq) {
            slots.remove(&self.slot);
            let _ = tx.send(OpMsg::Meta { line });
        }
    }
}

// Answered on a thread, because a read on a hung mount never returns and the loop waits on nothing.
pub fn spawn(ask: Ask, tx: Sender<OpMsg>) {
    let ticket = begin(&format!("code:{}", ask.slot));
    std::thread::spawn(move || {
        let bat = if sandbox::available() { bat_in(&std::env::var("PATH").unwrap_or_default()) } else { None };
        if let Some(line) = answer(&ask, bat.as_deref(), &ticket) {
            ticket.finish(line, &tx);
        }
    });
}

// The first bat on the path, only when the jail can see it: the jail binds /usr, so the file the link resolves to must lie under it.
// Sample input: "/usr/local/bin:/usr/bin" with /usr/bin/bat gives Some("/usr/bin/bat"); a bat under /home, or a link from /usr into it, gives None.
fn bat_in(path: &str) -> Option<String> {
    bat_under(path, Path::new("/usr"))
}

fn bat_under(path: &str, root: &Path) -> Option<String> {
    let found = path.split(':').filter(|d| !d.is_empty()).map(|d| Path::new(d).join("bat")).find(|p| p.is_file())?.canonicalize().ok()?;
    found.starts_with(root.canonicalize().ok()?).then(|| found.to_string_lossy().into_owned())
}

// None means a newer request took the slot while this one ran.
fn answer(ask: &Ask, bat: Option<&str>, ticket: &Ticket) -> Option<String> {
    let Some(range) = ask.range() else { return Some(reply(ask, "failed", 0, &[])) };
    let Some(data) = load(&ask.source) else { return Some(reply(ask, "failed", 0, &[])) };
    let formatted = takes_jq(&ask.source, &data);
    let verdict = judge(&data, formatted);
    match verdict {
        Verdict::Large => return Some(reply(ask, "large", 0, &[])),
        Verdict::Binary => return Some(reply(ask, "binary", 0, &[])),
        _ => {}
    }
    let Some(bat) = bat.filter(|_| !ask.raw && verdict == Verdict::Styled) else { return Some(plain(ask, &data)) };
    let line = match colour(ask, &data, (bat, &range), formatted, ticket)? {
        Colour::Lines { total, lines } => reply(ask, "styled", total.unwrap_or_else(|| line_count(&data)), &lines),
        Colour::Plain => plain(ask, &data),
        Colour::Failed => reply(ask, "failed", 0, &[]),
    };
    Some(line)
}

#[derive(Debug, PartialEq)]
enum Verdict {
    Large,
    Binary,
    Plain,
    Styled,
}

// The rules in their order. formatted is a one-line JSON file that jq breaks into lines, so its one long line is no reason for plain text.
// Sample input: 1,048,577 bytes is Large, a NUL at byte 1,023 is Binary, a 5,001 byte line is Plain.
fn judge(data: &[u8], formatted: bool) -> Verdict {
    if data.len() > LARGE_BYTES {
        Verdict::Large
    } else if data[..data.len().min(NUL_WINDOW)].contains(&0) {
        Verdict::Binary
    } else if !formatted && data.split(|&b| b == b'\n').any(|line| line.len() > LONG_LINE) {
        Verdict::Plain
    } else {
        Verdict::Styled
    }
}

// At most one byte past the large mark of a file is read; a file that cannot be read has no answer.
fn load(source: &Source) -> Option<Cow<'_, [u8]>> {
    match source {
        Source::Text { text, .. } => Some(Cow::Borrowed(text.as_bytes())),
        Source::File(path) if Path::new(path).is_absolute() => {
            let mut data = Vec::new();
            open_regular(Path::new(path))?.take(LARGE_BYTES as u64 + 1).read_to_end(&mut data).ok()?;
            Some(Cow::Owned(data))
        }
        Source::File(_) => None,
    }
}

// The same count src/backend/linecount.rs makes: newlines, plus one for bytes after the last; an empty file has none.
fn line_count(data: &[u8]) -> usize {
    data.iter().filter(|&&b| b == b'\n').count() + usize::from(!data.is_empty() && !data.ends_with(b"\n"))
}

// A file whose name ends .json and holds one line is minified, so jq formats it before bat colours it.
fn takes_jq(source: &Source, data: &[u8]) -> bool {
    matches!(source, Source::File(path) if is_json(path)) && line_count(data) == 1
}

fn is_json(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 5 && bytes[bytes.len() - 5..].eq_ignore_ascii_case(b".json")
}

fn plain(ask: &Ask, data: &[u8]) -> String {
    if let Some(delim) = ask.csv {
        let range = csvrange::align(data, ask.first, ask.count, delim, ask.record);
        let mut out = reply_at(ask, "plain", range.first, range.total, &plain_lines(data, range.first, range.count));
        out.insert_str(out.len() - 1, &format!(r#","records":{},"firstRecord":{}"#, range.records, range.first_record));
        return out;
    }
    reply_at(ask, "plain", ask.first, line_count(data), &plain_lines(data, ask.first, ask.count))
}

// The asked lines as they are, one foreground run each; a blank line has no run.
fn plain_lines(data: &[u8], first: usize, count: usize) -> Vec<Line> {
    if data.is_empty() {
        return Vec::new();
    }
    let body = data.strip_suffix(b"\n").unwrap_or(data);
    body.split(|&b| b == b'\n')
        .skip(first - 1)
        .take(count)
        .map(|line| {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            if line.is_empty() { Vec::new() } else { vec![(0, String::from_utf8_lossy(line).into_owned())] }
        })
        .collect()
}

enum Colour {
    // total is the formatted text's line count when jq made the text, and None when the data's own count stands.
    Lines { total: Option<usize>, lines: Vec<Line> },
    Plain,
    Failed,
}

// Runs bat (and jq for a one-line .json file) in one jail on the bytes the verdict was made on, which the jail gets on stdin and never opens the file; None when a newer request ended the job.
fn colour(ask: &Ask, data: &[u8], (bat, range): (&str, &str), formatted: bool, ticket: &Ticket) -> Option<Colour> {
    if ticket.superseded() {
        return None;
    }
    let inner = match &ask.source {
        Source::File(_) if formatted => json_argv(bat, range),
        Source::File(path) => text_argv(bat, path, "", range),
        Source::Text { name, lang, .. } => text_argv(bat, name, lang, range),
    };
    let mut full = sandbox::wrap_readonly_extra(&inner, &[]);
    sandbox::add_status(&mut full, inner.len());
    let spawned = jail::spawn_jailed(&full, |cmd| {
        cmd.env_clear().env("PATH", std::env::var_os("PATH").unwrap_or_default());
        cmd.env("LANG", std::env::var_os("LANG").unwrap_or_else(|| "C.UTF-8".into()));
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    });
    let Ok(mut jailed) = spawned else { return Some(Colour::Failed) };
    let (stdin, stdout) = (jailed.child.stdin.take(), jailed.child.stdout.take());
    if !ticket.attach(jailed) {
        return None;
    }
    // Written beside the read: bat may print before it has taken all of its input.
    if let Some(mut stdin) = stdin {
        let bytes = data.to_vec();
        std::thread::spawn(move || {
            let _ = stdin.write_all(&bytes);
        });
    }
    let mut out = Vec::new();
    if let Some(mut pipe) = stdout {
        let _ = pipe.read_to_end(&mut out);
    }
    let Ok(status) = ticket.reap()? else { return Some(Colour::Failed) };
    match status.code() {
        Some(0) => {}
        // bat exits 1 on a language it does not know, and that text is shown as it is.
        Some(1) if matches!(&ask.source, Source::Text { lang, .. } if !lang.is_empty()) => return Some(Colour::Plain),
        _ => return Some(Colour::Failed),
    }
    if !formatted {
        return Some(Colour::Lines { total: None, lines: sgr::parse(&out) });
    }
    // The script's first line is the formatted line count, or x when jq refused the file and bat coloured it as it is.
    let Some(split) = out.iter().position(|&b| b == b'\n') else { return Some(Colour::Failed) };
    let lines = sgr::parse(&out[split + 1..]);
    Some(match std::str::from_utf8(&out[..split]).map(str::trim) {
        Ok("x") => Colour::Lines { total: None, lines },
        Ok(count) => count.parse().map_or(Colour::Failed, |total| Colour::Lines { total: Some(total), lines }),
        Err(_) => Colour::Failed,
    })
}

// bat panics when it finds no home directory, which a jail has no passwd entry to give it for a user the system looks up elsewhere.
fn bat_cmd(bat: &str) -> Vec<String> {
    ["env", "HOME=/tmp", bat].iter().chain(BAT_FLAGS.iter()).map(|arg| arg.to_string()).collect()
}

fn text_argv(bat: &str, name: &str, lang: &str, range: &str) -> Vec<String> {
    let mut argv = bat_cmd(bat);
    if !lang.is_empty() {
        argv.push(format!("--language={lang}"));
    } else if !name.is_empty() {
        argv.push(format!("--file-name={name}"));
    }
    argv.push(format!("--line-range={range}"));
    argv
}

// The stdin bytes go to the jail's own /tmp (a private tmpfs in every jail) so jq's exit status is not lost in a pipe; a file jq refuses is coloured as it is.
fn json_argv(bat: &str, range: &str) -> Vec<String> {
    let flags = BAT_FLAGS.join(" ");
    let script = format!(
        "export HOME=/tmp; cat > /tmp/i; if jq --indent 2 . /tmp/i > /tmp/j 2>/dev/null; then wc -l < /tmp/j; exec \"$2\" {flags} --language=json --line-range=\"$1\" /tmp/j; \
         else echo x; exec \"$2\" {flags} --language=json --line-range=\"$1\" /tmp/i; fi"
    );
    ["sh", "-c", &script, "sh", range, bat].iter().map(|arg| arg.to_string()).collect()
}

// Sample output: {"t":"code","token":4,"slot":"look","state":"styled","first":1,"total":2,"lines":[[5,"fn",0," main"],[]]}
fn reply(ask: &Ask, state: &str, total: usize, lines: &[Line]) -> String {
    reply_at(ask, state, ask.first, total, lines)
}

fn reply_at(ask: &Ask, state: &str, first: usize, total: usize, lines: &[Line]) -> String {
    let mut out = format!(
        r#"{{"t":"code","token":{},"slot":"{}","state":"{}","first":{},"total":{},"lines":["#,
        ask.token,
        escape(&ask.slot),
        state,
        first,
        total
    );
    for (i, line) in lines.iter().enumerate() {
        out.push_str(if i == 0 { "[" } else { ",[" });
        for (j, (role, text)) in line.iter().enumerate() {
            out.push_str(if j == 0 { "" } else { "," });
            out.push((b'0' + role) as char);
            out.push_str(",\"");
            out.push_str(&escape(text));
            out.push('"');
        }
        out.push(']');
    }
    out.push_str("]}");
    out
}

#[cfg(test)]
#[path = "codereq_tests.rs"]
mod tests;
