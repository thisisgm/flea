use fff_search::{
    DirItem, FileItem, FilePicker, FilePickerOptions, FrecencyTracker, FuzzySearchOptions,
    GitRecencyConfig, MixedItemRef, MixedSearchConfig, PaginationArgs, QueryParser,
    SharedFilePicker, SharedFrecency,
};
use serde_json::json;
use std::error::Error;
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn emit(value: serde_json::Value) -> io::Result<()> {
    println!("{value}");
    io::stdout().flush()
}

fn prepare_prior_seed(scope: &Path, state: &Path, historical: bool) -> Result<(), Box<dyn Error>> {
    let at = Instant::now();
    let tracker = FrecencyTracker::open(state.join("frecency"))?;
    let history: Vec<String> = serde_json::from_slice(&std::fs::read(state.join("history.json"))?)?;
    let mut events = Vec::new();
    for rel in history {
        for _ in 0..(if historical { 3 } else { 1 }) {
            let before = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
            tracker.track_access(&scope.join(&rel))?;
            let after = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
            if before != after {
                return Err(
                    "prior replay crossed a timestamp second; rebuild the fixture seed".into(),
                );
            }
            events.push(json!({"path":rel.as_str(), "at_sec":before}));
        }
    }
    drop(tracker);
    emit(
        json!({"t":"priorseed", "replay_us":at.elapsed().as_secs_f64() * 1_000_000.0,
                "events":events, "prior_replay_inside_open":false}),
    )?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 5 {
        return Err("usage: fff-driver SCOPE MARKED_STATE bare|rich THREADS".into());
    }
    let scope = Path::new(&args[1]);
    let state = Path::new(&args[2]);
    if !scope.is_absolute() || !state.is_absolute() || !state.join(".findbench-root").is_file() {
        return Err("scope and marked state must be absolute".into());
    }
    if let Some(profile) = args[3].strip_prefix("seed-") {
        if !matches!(profile, "rich" | "equal-history") {
            return Err("invalid seed profile".into());
        }
        return prepare_prior_seed(scope, state, profile == "rich");
    }
    let historical = args[3] == "rich";
    let rich = match args[3].as_str() {
        "bare" => false,
        "rich" | "equal-history" => true,
        "fresh" => false,
        _ => return Err("profile must be fresh, equal-history, bare or rich".into()),
    };
    let threads: usize = args[4].parse()?;
    let cpus = std::thread::available_parallelism()?.get();
    if cpus != threads {
        return Err("CPU affinity does not match requested thread budget".into());
    }
    emit(json!({"t":"boot", "cpus":cpus, "scan_threads":(cpus / 2).max(2)}))?;
    let shared = SharedFilePicker::default();
    let frecency = if rich {
        SharedFrecency::default()
    } else {
        SharedFrecency::noop()
    };
    let parser = QueryParser::new(MixedSearchConfig);
    let mut opened = false;
    for line in io::stdin().lock().lines() {
        let line = line?;
        let fields: Vec<&str> = line.splitn(6, '\t').collect();
        match fields[0] {
            "O" if !opened => {
                let at = Instant::now();
                if rich {
                    let tracker = FrecencyTracker::open(state.join("frecency"))?;
                    if !state.join("fff-prior-seed.json").is_file() {
                        return Err("native prior seed was not prepared before index open".into());
                    }
                    frecency.init(tracker)?;
                }
                FilePicker::new_with_shared_state(
                    shared.clone(),
                    frecency.clone(),
                    FilePickerOptions {
                        base_path: args[1].clone(),
                        watch: true,
                        enable_content_indexing: false,
                        enable_mmap_cache: false,
                        follow_symlinks: false,
                        git_recency: GitRecencyConfig {
                            enabled: historical,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                )?;
                if !shared.wait_for_indexing_complete(Duration::from_secs(3600)) {
                    return Err("index build timed out".into());
                }
                let build_ms = at.elapsed().as_secs_f64() * 1000.0;
                if !shared.wait_for_watcher(Duration::from_secs(60)) {
                    return Err("watcher readiness timed out".into());
                }
                let guard = shared.read()?;
                let picker = guard.as_ref().ok_or("picker missing after scan")?;
                let dirs = picker.get_dirs();
                let root_dir = usize::from(
                    dirs.first()
                        .is_some_and(|dir| dir.relative_path(picker).is_empty()),
                );
                emit(json!({"t":"ready", "build_ms":build_ms,
                    "n":picker.live_file_count() + dirs.len() - root_dir,
                    "files":picker.live_file_count(), "arena_bytes":picker.arena_bytes(),
                    "file_struct_bytes":size_of::<FileItem>(), "dir_struct_bytes":size_of::<DirItem>(),
                    "has_git":picker.has_git_repo(), "git_status_off_supported":false,
                    "prior_replay_inside_open":false,
                    "watcher_ready_ms":at.elapsed().as_secs_f64() * 1000.0}))?;
                opened = true;
            }
            "Q" if opened && fields.len() == 6 => {
                let generation: usize = fields[1].parse()?;
                let limit: usize = fields[2].parse()?;
                let offset: usize = fields[3].parse()?;
                let at = Instant::now();
                let query = parser.parse(fields[5]);
                let guard = shared.read()?;
                let picker = guard.as_ref().ok_or("picker missing")?;
                let result = picker.fuzzy_search_mixed(
                    &query,
                    None,
                    FuzzySearchOptions {
                        max_threads: threads,
                        project_path: Some(scope),
                        pagination: PaginationArgs { offset, limit },
                        ..Default::default()
                    },
                );
                let core_ms = at.elapsed().as_secs_f64() * 1000.0;
                let paths: Vec<String> = result
                    .items
                    .iter()
                    .map(|item| match item {
                        MixedItemRef::File(file) => file.relative_path(picker),
                        MixedItemRef::Dir(dir) => {
                            dir.relative_path(picker).trim_end_matches('/').to_owned()
                        }
                    })
                    .collect();
                if let Some(target) = fields[4].strip_prefix("audit:") {
                    let rank = paths
                        .iter()
                        .position(|path| path == target)
                        .map_or(0, |rank| rank + 1);
                    emit(
                        json!({"t":"rank", "gen":generation, "rank":rank, "n":result.total_matched, "complete":paths.len() == result.total_matched}),
                    )?;
                    continue;
                }
                let comparable_core_ms = at.elapsed().as_secs_f64() * 1000.0;
                emit(
                    json!({"t":"result", "gen":generation, "n":result.total_matched,
                    "paths":paths, "core_ms":core_ms, "comparable_core_ms":comparable_core_ms}),
                )?;
            }
            "P" => emit(json!({"t":"pong"}))?,
            "X" => break,
            _ => return Err(format!("invalid command: {}", fields[0]).into()),
        }
    }
    shared.cancel();
    Ok(())
}
