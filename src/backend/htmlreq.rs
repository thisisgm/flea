use super::{archivework::Work, codereq, htmlcache, htmldeps, jail, opsreq::OpMsg, regfile, sandbox, thumbcache};
use crate::json::{escape, field_bool, field_str, field_usize};
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Stdio;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant, SystemTime};

const MAX_BYTES: u64 = 1_048_576;
const DEFAULT_WIDTH: usize = 984;
const STDERR_CAP: usize = htmldeps::MAX_RECORD_BYTES + STDERR_CHUNK;
const STDERR_CHUNK: usize = 8 * 1024;
const HELPER: &str = include_str!("htmlrender.qml");
const LIMIT: Duration = Duration::from_secs(15);

pub struct Ask {
    token: usize,
    slot: String,
    path: String,
    width: usize,
    height: usize,
    cancel: bool,
}

impl Ask {
    // Sample input: {"c":"html","token":3,"slot":"look","path":"/home/gm/index.html","width":984,"height":625}.
    pub fn parse(line: &str) -> Self {
        let requested_width = field_usize(line, "width").unwrap_or(DEFAULT_WIDTH).max(1);
        let width = requested_width.min(htmlcache::MAX_WIDTH);
        let height = field_usize(line, "height").map(|height| height.max(1).saturating_mul(width).div_ceil(requested_width)
            .clamp(1, htmlcache::MAX_HEIGHT as usize)).unwrap_or(htmlcache::DEFAULT_HEIGHT);
        Self { token: field_usize(line, "token").unwrap_or(0), slot: field_str(line, "slot").unwrap_or_default(),
            path: field_str(line, "path").unwrap_or_default(), width, height,
            cancel: field_bool(line, "cancel") }
    }
}

pub fn spawn(ask: Ask, tx: Sender<OpMsg>) {
    let ticket = codereq::begin(&format!("html:{}", ask.slot));
    std::thread::spawn(move || {
        let answer = if ask.cancel { Some(reply(&ask, "cancelled", "", 0, "")) }
            else { render(&ask, &ticket).or_else(|| (!ticket.superseded()).then(|| reply(&ask, "failed", "", 0, ""))) };
        if let Some(line) = answer { ticket.finish(line, &tx) }
    });
}

// Drain to EOF, retaining only the last bounded line for each helper marker, regardless of console noise.
fn drain_stderr(mut reader: impl Read) -> Vec<u8> {
    let mut markers: [Vec<u8>; 2] = Default::default();
    let mut line = Vec::new();
    let mut chunk = [0; STDERR_CHUNK];
    let mut oversized = false;
    let keep = |line: &[u8], markers: &mut [Vec<u8>; 2]| {
        for (index, marker) in [b"HTML_META ".as_slice(), b"HTML_RESOURCES ".as_slice()].iter().enumerate() {
            if line.windows(marker.len()).any(|part| part == *marker) { markers[index] = line.to_vec() }
        }
    };
    loop {
        let count = match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        for &byte in &chunk[..count] {
            if line.len() < STDERR_CAP { line.push(byte) } else { oversized = true }
            if byte != b'\n' { continue }
            if !oversized { keep(&line, &mut markers) }
            line.clear();
            oversized = false;
        }
    }
    if !oversized { keep(&line, &mut markers) }
    markers.concat()
}

fn exclusive(path: &Path, bytes: &[u8]) -> Option<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path).ok()?;
    file.write_all(bytes).ok()
}

// The policy is installed before the page parses; runtime files and other URL schemes never become subresources.
fn confined_html(bytes: &[u8]) -> Vec<u8> {
    let policy = b"<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; img-src file://*/page/ data:; style-src 'unsafe-inline' file://*/page/; font-src file://*/page/ data:; media-src file://*/page/; form-action 'none'; base-uri file://*/page/\">";
    let bom = if bytes.starts_with(b"\xef\xbb\xbf") { 3 } else { 0 };
    let mut head = bom;
    loop {
        while bytes.get(head).is_some_and(u8::is_ascii_whitespace) { head += 1; }
        if !bytes[head..].starts_with(b"<!--") { break }
        if bytes[head + 4..].starts_with(b">") { head += 5; continue }
        if bytes[head + 4..].starts_with(b"->") { head += 6; continue }
        let Some(end) = bytes[head + 4..].windows(3).position(|part| part == b"-->") else { break };
        head += 4 + end + 3;
    }
    let doctype = bytes.get(head..head + 9).is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"<!doctype"));
    // WHATWG 13.2.5.58/59/64/65 emits the doctype at the first '>', even inside a quoted identifier.
    let at = if doctype { bytes[head..].iter().position(|b| *b == b'>').map_or(bom, |end| head + end + 1) } else { bom };
    let mut out = Vec::with_capacity(bytes.len() + policy.len());
    out.extend_from_slice(&bytes[..at]);
    out.extend_from_slice(policy);
    out.extend_from_slice(&bytes[at..]);
    out
}

fn render(ask: &Ask, ticket: &codereq::Ticket) -> Option<String> {
    if ticket.superseded() || !Path::new(&ask.path).is_absolute() || !sandbox::available() { return None }
    let path = Path::new(&ask.path).canonicalize().ok()?;
    let (file, meta) = regfile::open_if_regular_with_meta(&path, 0).ok()?;
    if meta.len() > MAX_BYTES { return None }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_BYTES || bytes.contains(&0) { return None }
    let total = bytes.iter().filter(|&&b| b == b'\n').count() + usize::from(!bytes.is_empty() && !bytes.ends_with(b"\n"));
    let cache = htmlcache::root();
    let dest = cache.join(format!("{}.png", htmlcache::key(&path, &meta, ask.width, ask.height)));
    let folder = path.parent()?;
    if htmlcache::hit(&dest, ask.width, ask.height, folder) { return Some(reply(ask, "ready", &dest.to_string_lossy(), total, "")) }
    // Checked before the renderer runs, so a document this cache cannot replace costs no child; publish checks again.
    if !htmlcache::replaceable(&dest) { return None }
    std::fs::create_dir_all(&cache).ok()?;
    let work = Work::new(&cache, "html").ok()?;
    std::fs::set_permissions(&work.dir, std::fs::Permissions::from_mode(0o700)).ok()?;
    exclusive(&work.dir.join("render.qml"), HELPER.as_bytes())?;
    exclusive(&work.dir.join("input.html"), &confined_html(&bytes))?;
    exclusive(&work.dir.join("page.png"), &[])?;
    if ticket.superseded() { return None }
    let url = thumbcache::uri_for(&Path::new("/page").join(path.file_name()?));
    let inner = vec!["/usr/bin/qml6".to_string(), "/output/render.qml".to_string(), "--".to_string(), format!("--width={}", ask.width),
        format!("--height={}", ask.height), format!("--page={url}"), format!("--max-height={}", htmlcache::MAX_HEIGHT), format!("--max-resources={}", htmldeps::MAX_RESOURCES)];
    let mut argv = sandbox::wrap_html(&inner, &path, &work.dir);
    sandbox::add_status(&mut argv, inner.len());
    let started = SystemTime::now();
    let mut jailed = jail::spawn_jailed(&argv, |cmd| {
        cmd.env_clear().env("PATH", std::env::var_os("PATH").unwrap_or_default());
        cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    }).ok()?;
    let stderr = jailed.child.stderr.take();
    if !ticket.attach(jailed) { return None }
    let reader = std::thread::spawn(move || stderr.map(drain_stderr).unwrap_or_default());
    if !ticket.reap_until(Some(Instant::now() + LIMIT))?.ok()?.success() { return None }
    let png = work.dir.join("page.png");
    if !htmlcache::valid_png(&png, ask.width, ask.height) || ticket.superseded() { return None }
    let log = String::from_utf8_lossy(&reader.join().ok()?).into_owned();
    // Sample input: qml: HTML_META {"title":"Static HTML"}.
    let title = log.lines().find_map(|line| line.split_once("HTML_META ").and_then(|(_, text)| field_str(text, "title"))).unwrap_or_default();
    // Sample input: qml: HTML_RESOURCES {"resources":[{"url":"file:///page/style.css","failed":false}],"overflow":false}.
    let dependencies = log.lines().find_map(|line| line.split_once("HTML_RESOURCES ").and_then(|(_, text)| htmldeps::record(text, folder, started)));
    let record = work.dir.join("page.deps");
    exclusive(&record, dependencies.as_deref().unwrap_or(b"null\n"))?;
    htmlcache::publish(&png, &record, &dest).ok()?;
    htmlcache::evict(&cache);
    Some(reply(ask, "ready", &dest.to_string_lossy(), total, &title))
}

// Sample output: {"t":"html","slot":"look","token":3,"state":"ready","file":"/cache/x.png","total":61,"width":984,"title":"Page"}.
fn reply(ask: &Ask, state: &str, file: &str, total: usize, title: &str) -> String {
    format!(r#"{{"t":"html","slot":"{}","token":{},"state":"{}","file":"{}","total":{},"width":{},"title":"{}"}}"#,
        escape(&ask.slot), ask.token, state, escape(file), total, ask.width, escape(title))
}

#[cfg(test)]
#[path = "htmlreq_tests.rs"]
mod tests;
