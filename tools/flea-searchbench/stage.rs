// Bench-only entry points compile inside the pinned backend, never the packaged binary.
use super::searchindex::Index;
use super::searchsession::Session;
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::time::Instant;

fn paths(rows: &[(String, bool)]) -> String {
    rows.iter()
        .map(|(path, _)| format!("\"{}\"", crate::json::escape(path)))
        .collect::<Vec<_>>()
        .join(",")
}

fn say(value: &str) {
    println!("{value}");
    io::stdout().flush().unwrap();
}

fn cpu_ns() -> u64 {
    #[repr(C)]
    struct Stamp {
        seconds: std::os::raw::c_long,
        nanos: std::os::raw::c_long,
    }
    extern "C" {
        fn clock_gettime(clock: i32, stamp: *mut Stamp) -> i32;
    }
    let mut stamp = Stamp {
        seconds: 0,
        nanos: 0,
    };
    // SAFETY: Linux CLOCK_PROCESS_CPUTIME_ID writes one valid timespec.
    let rc = unsafe { clock_gettime(2, &mut stamp) };
    assert_eq!(rc, 0, "process clock unavailable");
    stamp.seconds as u64 * 1_000_000_000 + stamp.nanos as u64
}

fn memory() -> (usize, usize) {
    let body = std::fs::read_to_string("/proc/self/smaps_rollup").unwrap_or_default();
    let field = |name: &str| {
        body.lines()
            .find(|s| s.starts_with(name))
            .and_then(|s| s.split_whitespace().nth(1))
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0)
            * 1024
    };
    (field("Rss:"), field("Pss:"))
}

fn serve(threads: usize) -> Result<(), String> {
    let root = std::env::var("FLEA_STAGE_ROOT").map_err(|_| "FLEA_STAGE_ROOT required")?;
    let path = Path::new(&root);
    if !path.is_absolute() || !path.join(".findbench-root").is_file() {
        return Err("absolute marked stage root required".into());
    }
    let empty = path.join("stage-empty");
    std::fs::create_dir(&empty).map_err(|e| e.to_string())?;
    let (tx, _rx) = std::sync::mpsc::channel();
    let mut session = Session::open_async(
        empty.to_str().ok_or("non-UTF8 root")?,
        empty.to_str().unwrap(),
        false,
        tx,
    );
    while session.walking() {
        std::thread::yield_now();
    }
    session.poll_walked();
    let cpus = std::thread::available_parallelism()
        .map_err(|e| e.to_string())?
        .get();
    if cpus != threads {
        return Err("thread budget differs".into());
    }
    say(&format!(
        r#"{{"t":"boot","cpus":{cpus},"engine":"flea-search","profile":"native","version":"0ebbcffae","parallel":"product session","licence":"MIT"}}"#
    ));
    for line in io::stdin().lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        let fields: Vec<_> = line.splitn(6, '\t').collect();
        match (fields[0], fields.len()) {
            ("O", 2) => {
                let started = Instant::now();
                let text = std::fs::read_to_string(fields[1]).map_err(|e| e.to_string())?;
                let at = Instant::now();
                let cpu = cpu_ns();
                let mut index = Index::new();
                for path in text.lines() {
                    if !index.push(path, false) {
                        return Err("partial match index".into());
                    }
                }
                index.finish();
                let n = index.len();
                *session.index.lock().unwrap() = index;
                let ms = at.elapsed().as_secs_f64() * 1000.0;
                let cpu = cpu_ns() - cpu;
                let (rss, pss) = memory();
                say(&format!(
                    r#"{{"t":"ready","n":{n},"build_ms":{ms},"read_ms":{},"build_cpu_ns":{cpu},"rss":{rss},"pss":{pss}}}"#,
                    started.elapsed().as_secs_f64() * 1000.0
                ));
            }
            ("Q", 6) | ("A", 4) => {
                let audit = fields[0] == "A";
                let gen = fields[1].parse::<usize>().map_err(|_| "bad generation")?;
                let limit = if audit {
                    usize::MAX
                } else {
                    fields[2].parse::<usize>().map_err(|_| "bad limit")?
                };
                let offset = if audit {
                    0
                } else {
                    fields[3].parse::<usize>().map_err(|_| "bad offset")?
                };
                let text = fields[if audit { 3 } else { 5 }];
                let cpu = cpu_ns();
                let at = Instant::now();
                let answer = session
                    .query(gen, text, limit.saturating_add(offset))
                    .ok_or("stale generation")?;
                let core_ms = at.elapsed().as_secs_f64() * 1000.0;
                let cpu = cpu_ns() - cpu;
                if audit {
                    let rank = answer
                        .rows
                        .iter()
                        .position(|(path, _)| path == fields[2])
                        .map_or(0, |p| p + 1);
                    say(&format!(
                        r#"{{"t":"audit","gen":{gen},"n":{},"rank":{rank},"core_ms":{core_ms}}}"#,
                        answer.total
                    ));
                } else {
                    let rows: Vec<_> = answer.rows.into_iter().skip(offset).take(limit).collect();
                    say(&format!(
                        r#"{{"t":"result","gen":{gen},"n":{},"paths":[{}],"core_ms":{core_ms},"cpu_ns":{cpu}}}"#,
                        answer.total,
                        paths(&rows)
                    ));
                }
            }
            ("P", 1) => say(r#"{"t":"pong"}"#),
            ("X", 1) => break,
            _ => return Err("invalid stage command".into()),
        }
    }
    session.close();
    Ok(())
}

fn walk(root: &Path) -> Result<(), String> {
    if !root.is_absolute() {
        return Err("absolute tree required".into());
    }
    let body = super::mountinfo::read_body();
    let probes = super::searchwalk::WalkProbes::new();
    let remote = super::iomount::is_remote(root, &body);
    let at = Instant::now();
    let (index, _) = super::searchwalk::walk(root, false, remote, &body, &probes);
    let elapsed = at.elapsed().as_nanos();
    let (rss, pss) = memory();
    let errors = usize::from(index.partial.is_some());
    let mut files = 0;
    let mut dirs = 0;
    for row in 0..index.len() {
        if index.is_dir(row) {
            dirs += 1;
        } else {
            files += 1;
        }
    }
    say(&format!(
        r#"{{"t":"walk","elapsed_ns":{elapsed},"files":{files},"dirs":{dirs},"errors":{errors},"rss":{rss},"pss":{pss},"index_bytes":{},"identities":["#,
        index.bytes(true)
    ));
    let mut out = io::stdout().lock();
    for row in 0..index.len() {
        if row > 0 {
            write!(out, ",").map_err(|e| e.to_string())?;
        }
        write!(out, "[\"").map_err(|e| e.to_string())?;
        for byte in index.raw(row) {
            write!(out, "{byte:02x}").map_err(|e| e.to_string())?;
        }
        write!(out, "\",\"{}\"]", if index.is_dir(row) { "d" } else { "f" })
            .map_err(|e| e.to_string())?;
    }
    writeln!(out, "]}}").map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) fn main(args: &[String]) -> Result<(), String> {
    match args.get(2).map(String::as_str) {
        Some("match") => serve(
            args.get(3)
                .ok_or("threads required")?
                .parse()
                .map_err(|_| "bad threads")?,
        ),
        Some("walk") => walk(Path::new(args.get(3).ok_or("tree required")?)),
        _ => Err("stage match|walk required".into()),
    }
}
