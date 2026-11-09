// The image probe behind flashimage: kind from the name confirmed by magic, sizes in the sandbox.
use crate::backend::flash::IMAGE_SUFFIXES;
use flea::bytename::ExactText as _;
use std::path::Path;
use std::time::Duration;

// The probe's own deadline, short: xz --list on a local file answers in milliseconds.
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
// 1 MiB of listing output, far past any header this probe reads.
const CAPTURE_CAP: usize = 1024 * 1024;
// ISO 9660's signature and where it sits.
const ISO_OFFSET: usize = 32769;
const ISO_MAGIC: &[u8] = b"CD001";
// Enough head to hold every magic below plus the ISO offset.
const HEAD_BYTES: usize = 32774;
// One read off a tool's stdout pipe.
const READ_BYTES: usize = 8192;
// How often a wait on the tool looks at the clock again.
const POLL_STEP: Duration = Duration::from_millis(20);

pub struct Probe {
    pub kind: String,
    pub bytes: u64,
    pub unpacked: u64,
    pub known: bool,
    pub member: String,
}

// Sample name: "raspios-arm64.img.xz" answers xz when its head holds the xz magic too.
pub fn probe(path: &Path) -> Probe {
    let bytes = path.metadata().map(|m| m.len()).unwrap_or(0);
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_lowercase();
    let named = kind_of_name(&name);
    if named.is_empty() {
        return Probe { kind: String::new(), bytes, unpacked: 0, known: false, member: String::new() };
    }
    let Some(head) = read_head(path) else {
        return Probe { kind: String::new(), bytes, unpacked: 0, known: false, member: String::new() };
    };
    if !magic_holds(named, &head) {
        return Probe { kind: String::new(), bytes, unpacked: 0, known: false, member: String::new() };
    }
    match named {
        "iso" | "img" => Probe { kind: named.to_string(), bytes, unpacked: bytes, known: true, member: String::new() },
        "gz" | "bz2" => Probe { kind: named.to_string(), bytes, unpacked: 0, known: false, member: String::new() },
        "xz" => {
            let (unpacked, known) = xz_size(path);
            Probe { kind: named.to_string(), bytes, unpacked, known, member: String::new() }
        }
        "zst" => {
            let (unpacked, known) = crate::backend::flashzstd::unpacked_size(path);
            Probe { kind: named.to_string(), bytes, unpacked, known, member: String::new() }
        }
        "zip" => zip_probe(path, bytes),
        _ => Probe { kind: String::new(), bytes, unpacked: 0, known: false, member: String::new() },
    }
}

// The suffix alone, before magic confirms it; a name and magic that disagree refuse the row.
pub(crate) fn kind_of_name(name: &str) -> &str {
    IMAGE_SUFFIXES.iter().find(|(suffix, _)| name.ends_with(suffix)).map_or("", |(_, kind)| kind)
}

// True when the head holds CD001 at ISO 9660's own offset; both the .iso file and a zipped member ask this.
fn iso_signature(head: &[u8]) -> bool {
    head.get(ISO_OFFSET..ISO_OFFSET + ISO_MAGIC.len()) == Some(ISO_MAGIC)
}

// Sample head: FD 37 7A 58 5A 00 answers xz; PK 03 04 answers zip.
fn magic_holds(kind: &str, head: &[u8]) -> bool {
    match kind {
        "iso" => iso_signature(head),
        "img" => true,
        "xz" => head.starts_with(&[0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00]),
        "gz" => head.starts_with(&[0x1F, 0x8B]),
        "zst" => head.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]),
        "bz2" => head.starts_with(b"BZh"),
        "zip" => head.starts_with(&[0x50, 0x4B, 0x03, 0x04]),
        _ => false,
    }
}

// None for anything but a regular file: a fifo's open never returns and would hold the loop.
fn read_head(path: &Path) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut buf = vec![0u8; HEAD_BYTES];
    let mut file = crate::backend::regfile::open_regular(path)?;
    let mut got = 0;
    while got < HEAD_BYTES {
        match file.read(&mut buf[got..]) {
            Ok(0) => break,
            Ok(n) => got += n,
            Err(_) => break,
        }
    }
    buf.truncate(got);
    Some(buf)
}

// Sample robot line: "file\t1\t1\t100072\t100000\t..." answers 100000.
fn parse_xz_robot(text: &str) -> Option<u64> {
    for line in text.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.first() == Some(&"file") && fields.len() > 4 {
            if let Ok(n) = fields[4].trim().parse::<u64>() {
                return Some(n);
            }
        }
    }
    None
}

fn xz_size(path: &Path) -> (u64, bool) {
    let inner = vec![
        "xz".to_string(),
        "--robot".to_string(),
        "--list".to_string(),
        path.exact_text().to_string(),
    ];
    match capture(&crate::backend::sandbox::wrap_readonly(&inner, path)) {
        Some(text) => match parse_xz_robot(&text) {
            Some(n) => (n, true),
            None => (0, false),
        },
        None => (0, false),
    }
}

// The one file member beside any folders when it alone is an image, else nothing.
pub(crate) fn zip_image_member(names: &[crate::backend::archivelist::Entry]) -> String {
    let mut files = names.iter().filter(|e| !e.is_dir);
    let Some(only) = files.next() else { return String::new(); };
    if files.next().is_some() {
        return String::new();
    }
    let lower = only.name.to_lowercase();
    if lower.ends_with(".iso") || lower.ends_with(".img") || lower.ends_with(".raw") { only.name.clone() } else { String::new() }
}

// The name list is capped while the count is exact, so a listing that names fewer rows than it counted could hide a second file past the cap; only a whole listing names one member.
pub(crate) fn zip_member(contents: &crate::backend::archivelist::Contents) -> String {
    if contents.entries != contents.names.len() {
        return String::new();
    }
    zip_image_member(&contents.names)
}

// A zip qualifies only with exactly one file member, and that member an image by the same rules.
fn zip_probe(path: &Path, bytes: u64) -> Probe {
    let refused = Probe {
        kind: String::new(),
        bytes,
        unpacked: 0,
        known: false,
        member: String::new(),
    };
    let inner = vec![
        "bsdtar".to_string(),
        "-tvf".to_string(),
        path.exact_text().to_string(),
    ];
    let Some(text) = capture(&crate::backend::sandbox::wrap_readonly(&inner, path)) else {
        return refused;
    };
    let contents = crate::backend::archivelist::parse_reader(
        std::io::BufReader::new(text.as_bytes()),
        &crate::backend::archivespec::tar_spec(),
    );
    if contents.failed {
        return refused;
    }
    let member = zip_member(&contents);
    if member.is_empty() {
        return refused;
    }
    if member.to_lowercase().ends_with(".iso") && !zip_member_is_iso(path, &member) {
        return refused;
    }
    Probe { kind: "zip".to_string(), bytes, unpacked: contents.unpacked, known: true, member }
}

// An .iso member earns its kind only when its head holds CD001, read through the member pipe.
fn zip_member_is_iso(path: &Path, member: &str) -> bool {
    let inner = vec![
        "bsdtar".to_string(),
        "-xOf".to_string(),
        path.exact_text().to_string(),
        crate::backend::flashpipe::escape_member(member),
    ];
    let Some(head) = capture_head(
        &crate::backend::sandbox::wrap_readonly(&inner, path),
        HEAD_BYTES,
    ) else {
        return false;
    };
    iso_signature(&head)
}

// One sandboxed tool run with its stdout captured up to the cap; None never parsed as a size.
fn capture(argv: &[String]) -> Option<String> {
    spawn_capture(argv, CAPTURE_CAP, false).and_then(|bytes| String::from_utf8(bytes).ok())
}

// The first cap bytes of a tool's stdout; the tool may write on, and a full head ends it.
fn capture_head(argv: &[String], cap: usize) -> Option<Vec<u8>> {
    spawn_capture(argv, cap, true)
}

fn spawn_capture(argv: &[String], cap: usize, head_only: bool) -> Option<Vec<u8>> {
    if !crate::backend::sandbox::available() {
        return None;
    }
    let jailed = crate::backend::jail::spawn_jailed(argv, |cmd| {
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::null());
    }).ok()?;
    collect(jailed, cap, head_only, PROBE_TIMEOUT)
}

// Reads the tool's stdout up to cap: a full head is taken and the tool ended when head_only, anything shorter needs a clean exit.
fn collect(mut jailed: crate::backend::jail::Jailed, cap: usize, head_only: bool, limit: Duration) -> Option<Vec<u8>> {
    let mut reader = jailed.child.stdout.take();
    let worker = std::thread::spawn(move || {
        let mut out = Vec::new();
        if let Some(pipe) = reader.as_mut() {
            use std::io::Read;
            let mut chunk = [0u8; READ_BYTES];
            while out.len() < cap {
                match pipe.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => out.extend_from_slice(&chunk[..n]),
                    Err(_) => break,
                }
            }
        }
        out
    });
    let deadline = std::time::Instant::now() + limit;
    loop {
        if worker.is_finished() {
            let out = worker.join().unwrap_or_default();
            if out.len() >= cap {
                crate::backend::jail::kill_tree(&mut jailed);
                return head_only.then_some(out);
            }
            return exited_cleanly(&mut jailed, deadline).then_some(out);
        }
        if std::time::Instant::now() >= deadline {
            crate::backend::jail::kill_tree(&mut jailed);
            let _ = worker.join();
            return None;
        }
        std::thread::sleep(POLL_STEP);
    }
}

// True when the tool exits with status zero before the deadline; a tool still running at it is killed.
fn exited_cleanly(jailed: &mut crate::backend::jail::Jailed, deadline: std::time::Instant) -> bool {
    loop {
        match jailed.child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if std::time::Instant::now() < deadline => std::thread::sleep(POLL_STEP),
            _ => {
                crate::backend::jail::kill_tree(jailed);
                return false;
            }
        }
    }
}

#[cfg(test)]
#[path = "flashimage_tests.rs"]
mod tests;
