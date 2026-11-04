// Engine-stage adapters for the Findbest matching roster: every entrant answers one line protocol over one corpus.
use serde_json::json;
use std::io::{self, BufRead, Write};
use std::sync::Arc;
use std::time::Instant;

// Flea's own scorer, compiled from the product source so the anchor row is the shipped code, not a copy.
#[allow(dead_code)]
#[path = "../../../src/backend/fuzzy.rs"]
mod flea_fuzzy;

// The module path fuzzy.rs expects, so it compiles unchanged.
mod backend {
    // Only the final tie-break of rank_order reads this; byte order stands in for the Finder order, noted in contract.json.
    pub mod sort {
        pub fn name_order(a: &[u8], b: &[u8]) -> std::cmp::Ordering {
            a.cmp(b)
        }
    }
    pub(crate) use crate::flea_fuzzy as fuzzy;
}

mod adapters;

use adapters::{make, ENTRANTS};

// Process CPU in nanoseconds, every thread included, exited ones too.
fn cpu_ns() -> u64 {
    let mut spec = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: clock_gettime writes one timespec through a valid pointer.
    let rc = unsafe { libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut spec) };
    if rc != 0 {
        return 0;
    }
    spec.tv_sec as u64 * 1_000_000_000 + spec.tv_nsec as u64
}

fn emit(value: serde_json::Value) -> io::Result<()> {
    let mut out = io::stdout().lock();
    writeln!(out, "{value}")?;
    out.flush()
}

fn ms(at: Instant) -> f64 {
    at.elapsed().as_secs_f64() * 1000.0
}

// The driver loop: boot, O loads a corpus file, Q answers top-k, A audits one target's full rank, P pongs, X exits.
fn serve(name: &str, profile: &str, threads: usize) -> Result<(), String> {
    let cpus = std::thread::available_parallelism()
        .map_err(|e| e.to_string())?
        .get();
    if cpus != threads {
        return Err(format!(
            "CPU affinity {cpus} does not match the requested thread budget {threads}"
        ));
    }
    let identity = ENTRANTS
        .iter()
        .find(|e| e.0 == name)
        .ok_or(format!("unknown entrant {name}"))?;
    let mut adapter = make(name, profile, threads)?;
    let io_err = |e: io::Error| e.to_string();
    emit(
        json!({"t": "boot", "cpus": cpus, "engine": name, "profile": profile, "version": identity.1,
        "licence": identity.2, "parallel": identity.3}),
    )
    .map_err(io_err)?;
    let mut corpus: Arc<Vec<String>> = Arc::new(Vec::new());
    let mut last = String::new();
    for line in io::stdin().lock().lines() {
        let line = line.map_err(io_err)?;
        let fields: Vec<&str> = line.splitn(6, '\t').collect();
        match (fields[0], fields.len()) {
            ("O", 2) => {
                let at = Instant::now();
                let text = std::fs::read_to_string(fields[1])
                    .map_err(|e| format!("corpus {}: {e}", fields[1]))?;
                corpus = Arc::new(text.lines().map(str::to_string).collect());
                let cpu = cpu_ns();
                let build = Instant::now();
                adapter.load(corpus.clone());
                emit(json!({"t": "ready", "n": corpus.len(), "build_ms": ms(build), "read_ms": ms(at),
                    "build_cpu_ns": cpu_ns() - cpu}))
                .map_err(io_err)?;
            }
            ("Q", 6) => {
                let generation: u64 = fields[1].parse().map_err(|_| "bad generation")?;
                let limit: usize = fields[2].parse().map_err(|_| "bad limit")?;
                let offset: usize = fields[3].parse().map_err(|_| "bad offset")?;
                let text = fields[5];
                let cpu = cpu_ns();
                let at = Instant::now();
                let (hits, total) = adapter.query(text, limit.saturating_add(offset), true);
                let core_ms = ms(at);
                let cpu = cpu_ns() - cpu;
                let paths: Vec<&str> = hits
                    .iter()
                    .skip(offset)
                    .take(limit)
                    .map(|&i| corpus[i as usize].as_str())
                    .collect();
                emit(json!({"t": "result", "gen": generation, "n": total, "paths": paths, "core_ms": core_ms, "cpu_ns": cpu}))
                    .map_err(io_err)?;
                last = text.to_string();
            }
            ("A", 4) => {
                let generation: u64 = fields[1].parse().map_err(|_| "bad generation")?;
                let (target, text) = (fields[2], fields[3]);
                let at = Instant::now();
                let (hits, total) =
                    adapter.query(text, usize::MAX, text.starts_with(last.as_str()));
                let core_ms = ms(at);
                let rank = hits
                    .iter()
                    .position(|&i| corpus[i as usize] == target)
                    .map_or(0, |p| p + 1);
                emit(json!({"t": "audit", "gen": generation, "n": total, "rank": rank, "core_ms": core_ms})).map_err(io_err)?;
                last = text.to_string();
            }
            ("P", 1) => emit(json!({"t": "pong"})).map_err(io_err)?,
            ("X", 1) => break,
            _ => return Err(format!("invalid command: {}", fields[0])),
        }
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let result = match args.get(1).map(String::as_str) {
        Some("adapter") if args.len() == 5 => match args[4].parse::<usize>() {
            Ok(threads) if threads > 0 => serve(&args[2], &args[3], threads),
            _ => Err("threads must be a positive integer".to_string()),
        },
        Some("lock") => {
            print!(
                "{}",
                include_str!(concat!(env!("OUT_DIR"), "/resolved.lock"))
            );
            Ok(())
        }
        Some("entrants") => {
            for (name, version, licence, parallel) in ENTRANTS {
                println!("{name}\t{version}\t{licence}\t{parallel}");
            }
            Ok(())
        }
        Some("bench") => {
            // The orchestrator is Python beside this crate; it drives this same binary as every Rust entrant.
            let runner = concat!(env!("CARGO_MANIFEST_DIR"), "/runner.py");
            let exe = std::env::current_exe().map_err(|e| e.to_string());
            match exe {
                Ok(exe) => {
                    use std::os::unix::process::CommandExt;
                    let error = std::process::Command::new("python3")
                        .arg("-I")
                        .arg(runner)
                        .arg("--adapter-bin")
                        .arg(exe)
                        .args(&args[2..])
                        .exec();
                    Err(format!("python3 runner: {error}"))
                }
                Err(e) => Err(e),
            }
        }
        _ => Err(
            "usage: flea-matchbench adapter ENTRANT PROFILE THREADS | lock | entrants | bench ARGS"
                .to_string(),
        ),
    };
    if let Err(error) = result {
        eprintln!("flea-matchbench: {error}");
        std::process::exit(2);
    }
}
