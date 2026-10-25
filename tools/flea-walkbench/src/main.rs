//! flea-walkbench: traversal and index-build stage comparison for the Flea search roster.
mod adapters;
mod fixture;
mod measure;
mod run;
mod stats;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

const USAGE: &str = "usage:
  flea-walkbench run --root ABS [--create-root] --fixtures 100000,1000000,flea-source [--runs 30]
      [--noise-batches 2] [--noise-runs 5] [--threads N] [--cache warm|cold] [--cold-hook CMD]
      [--adapters LIST] [--reference ADAPTER] [--fd PATH] [--rg PATH] [--repo DIR] [--rev REV]
      [--seed N] [--resamples 10000] [--reuse-fixtures] [--stdout]
  flea-walkbench adapter NAME --tree ABS --mode walk|index [--threads N] [--tool PATH]
  flea-walkbench census --tree ABS
  flea-walkbench print-lock";

/// Flags as a map; a flag with no value maps to an empty string.
pub struct Args(HashMap<String, String>, Vec<String>);

impl Args {
    fn parse(raw: &[String]) -> Result<Args, String> {
        let (mut flags, mut positional, mut i) = (HashMap::new(), Vec::new(), 0);
        while i < raw.len() {
            if let Some(name) = raw[i].strip_prefix("--") {
                let value = raw.get(i + 1).filter(|v| !v.starts_with("--")).cloned();
                i += usize::from(value.is_some());
                if flags
                    .insert(name.to_string(), value.unwrap_or_default())
                    .is_some()
                {
                    return Err(format!("--{name} given twice"));
                }
            } else {
                positional.push(raw[i].clone());
            }
            i += 1;
        }
        Ok(Args(flags, positional))
    }
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.get(name).map(String::as_str)
    }
    pub fn has(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }
    pub fn or(&self, name: &str, default: &str) -> String {
        self.get(name).unwrap_or(default).to_string()
    }
    pub fn number(&self, name: &str, default: usize) -> Result<usize, String> {
        self.get(name).map_or(Ok(default), |v| {
            v.parse().map_err(|e| format!("--{name}: {e}"))
        })
    }
    pub fn path(&self, name: &str) -> Option<PathBuf> {
        self.get(name).filter(|v| !v.is_empty()).map(PathBuf::from)
    }
}

pub fn default_threads() -> usize {
    std::thread::available_parallelism().map_or(1, usize::from)
}

fn adapter(args: &Args) -> Result<(), String> {
    let name = args.1.get(1).ok_or("adapter name required")?;
    let tree = args.path("tree").ok_or("--tree required")?;
    if !tree.is_absolute() {
        return Err("--tree must be absolute".into());
    }
    let mode = match args.get("mode") {
        Some("walk") => adapters::Mode::Walk,
        Some("index") => adapters::Mode::Index,
        _ => return Err("--mode walk|index required".into()),
    };
    let threads = args.number("threads", default_threads())?;
    if name == "harness-floor" {
        println!("RESULT\t0\t0\t0\t0\t-\t0\t{}\t0\t0", adapters::hwm_kib());
        return Ok(());
    }
    let outcome = adapters::run(name, &tree, threads, mode, args.path("tool").as_deref())?;
    let digest = if mode == adapters::Mode::Index {
        fixture::digest(&mut outcome.sink.items())
    } else {
        "-".into()
    };
    let o = &outcome;
    println!(
        "RESULT\t{}\t{}\t{}\t{}\t{digest}\t{}\t{}\t{}\t{}",
        o.elapsed_ns,
        o.sink.files,
        o.sink.dirs,
        o.errors,
        o.cpu_us,
        o.hwm_kib,
        o.spawn_hwm_kib,
        o.tool_maxrss_kib
    );
    Ok(())
}

fn census(args: &Args) -> Result<(), String> {
    let tree = args.path("tree").ok_or("--tree required")?;
    let census = fixture::census(&tree)?;
    let (files, dirs, hidden) = (census.files(), census.dirs(), census.hidden);
    let mut items = census.items;
    let full = fixture::digest(&mut items);
    items.retain(|item| item.1 == b'f');
    println!("entries={}\tfiles={files}\tdirs={dirs}\thidden={hidden}\tidentity_sha256={full}\tfiles_sha256={}",
        files + dirs, fixture::digest(&mut items));
    Ok(())
}

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let result = Args::parse(&raw).and_then(|args| match args.1.first().map(String::as_str) {
        Some("run") => run::run(&args),
        Some("adapter") => adapter(&args),
        Some("census") => census(&args),
        Some("print-lock") => {
            print!(
                "{}",
                include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.lock"))
            );
            Ok(())
        }
        _ => Err(USAGE.into()),
    });
    if let Err(error) = result {
        eprintln!("flea-walkbench: {error}");
        std::process::exit(2);
    }
}

/// Executable of this process, so every trial runs the exact same harness bytes.
pub fn self_exe() -> Result<PathBuf, String> {
    std::fs::read_link("/proc/self/exe").map_err(|e| e.to_string())
}

pub fn first_line(path: &Path, arg: &str) -> String {
    std::process::Command::new(path)
        .arg(arg)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.lines().next().map(str::to_string))
        .unwrap_or_else(|| "unavailable".into())
}
