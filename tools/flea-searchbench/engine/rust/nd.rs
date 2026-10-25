#[cfg(not(target_os = "linux"))]
compile_error!("this portable-core bench adapter requires Linux");

#[path = "/sources/fsearch-nd/src/index.rs"]
mod index;
#[path = "/sources/fsearch-nd/src/live.rs"]
mod live;
#[path = "/sources/fsearch-nd/src/query.rs"]
mod query;
#[path = "linux_walk.rs"]
mod walk;

use serde_json::json;
use std::error::Error;
use std::io::{self, BufRead, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::time::Instant;

fn no_materialize() {}

fn emit(value: serde_json::Value) -> io::Result<()> {
    println!("{value}");
    io::stdout().flush()
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        return Err("usage: fsearch-nd-driver ABSOLUTE_SCOPE THREADS".into());
    }
    let root = Path::new(&args[1]);
    if !root.is_absolute() {
        return Err("scope must be absolute".into());
    }
    let threads: usize = args[2].parse()?;
    let cpus = std::thread::available_parallelism()?.get();
    if cpus != threads {
        return Err("CPU affinity does not match requested thread budget".into());
    }
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()?;
    emit(json!({"t":"boot", "cpus":cpus, "watcher":"unsupported"}))?;
    let mut current: Option<live::Live> = None;
    for line in io::stdin().lock().lines() {
        let line = line?;
        let fields: Vec<&str> = line.splitn(6, '\t').collect();
        match fields[0] {
            "O" if current.is_none() => {
                let at = Instant::now();
                let listings = walk::rooted_scan(root, threads);
                let index = pool.install(|| {
                    index::Index::build(listings, 0, query::now_secs(), root.as_os_str().as_bytes())
                });
                let name_bytes = index.bytes();
                let sections = index.sections();
                let instance = live::Live::new(index);
                let mut scope_query =
                    query::Query::parse("", &args[1]).map_err(io::Error::other)?;
                scope_query.scope = Some(root.as_os_str().as_bytes().to_vec());
                let (begin, end) = query::Searcher { live: &instance }
                    .scope_range(&scope_query)
                    .ok_or("scope missing")?;
                emit(
                    json!({"t":"ready", "n":end - begin, "name_bytes":name_bytes,
                    "sections":sections, "build_ms":at.elapsed().as_secs_f64() * 1000.0,
                    "watcher":"unsupported", "crawler":"linux-read_dir-plus-lstat"}),
                )?;
                current = Some(instance);
            }
            "Q" if fields.len() == 6 => {
                let generation: usize = fields[1].parse()?;
                let limit: usize = fields[2].parse()?;
                let offset: usize = fields[3].parse()?;
                let at = Instant::now();
                let mut q = query::Query::parse(fields[5], &args[1]).map_err(io::Error::other)?;
                q.scope = Some(root.as_os_str().as_bytes().to_vec());
                q.limit = limit.checked_add(offset).ok_or("pagination overflow")?;
                let instance = current.as_ref().ok_or("not opened")?;
                let hits = pool.install(|| query::Searcher { live: instance }.search(&q));
                let core_ms = at.elapsed().as_secs_f64() * 1000.0;
                let mut paths = Vec::new();
                for hit in hits.into_iter().skip(offset).take(limit) {
                    let mut absolute = Vec::new();
                    if let Some(path) = hit.over {
                        absolute = path;
                    } else {
                        instance.base.path(hit.idx as usize, &mut absolute);
                    }
                    let rel = absolute
                        .strip_prefix(root.as_os_str().as_bytes())
                        .ok_or("result escaped scope")?;
                    let rel = rel.strip_prefix(b"/").unwrap_or(rel);
                    paths.push(String::from_utf8(rel.to_vec())?);
                }
                if let Some(target) = fields[4].strip_prefix("audit:") {
                    let rank = paths
                        .iter()
                        .position(|path| path == target)
                        .map_or(0, |rank| rank + 1);
                    emit(
                        json!({"t":"rank", "gen":generation, "rank":rank, "n":paths.len(), "complete":paths.len() < limit}),
                    )?;
                    continue;
                }
                let comparable_core_ms = at.elapsed().as_secs_f64() * 1000.0;
                emit(
                    json!({"t":"result", "gen":generation, "n":null, "paths":paths,
                    "core_ms":core_ms, "comparable_core_ms":comparable_core_ms, "total_matches":"not exposed by top-k API"}),
                )?;
            }
            "R" if fields.len() == 2 => {
                let dir = Path::new(fields[1]);
                if !dir.starts_with(root) {
                    return Err("relist escaped scope".into());
                }
                let at = Instant::now();
                current
                    .as_mut()
                    .ok_or("not opened")?
                    .apply_dir(dir.as_os_str().as_bytes(), false);
                emit(json!({"t":"patched", "ms":at.elapsed().as_secs_f64() * 1000.0}))?;
            }
            "P" => emit(json!({"t":"pong"}))?,
            "X" => break,
            _ => return Err(format!("invalid command: {}", fields[0]).into()),
        }
    }
    Ok(())
}
