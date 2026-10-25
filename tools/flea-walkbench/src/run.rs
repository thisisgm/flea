//! The paired interleaved campaign: fixtures, warm-up, two noise batches, N measured rounds and the summary.
use crate::{adapters, default_threads, first_line, fixture, measure, self_exe, stats, Args};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

const HEADER: &str = "host\tfixture\tphase\tround\torder\tadapter\tmode\tthreads\tcache\twall_ns\tstage_ns\tuser_us\tsys_us\tmaxrss_kib\tstage_cpu_us\thwm_kib\tspawn_hwm_kib\ttool_maxrss_kib\tfiles\tdirs\tcensus_files\tcensus_dirs\terrors\texit\tcoverage\tdigest";

struct Row {
    phase: String,
    round: usize,
    adapter: String,
    mode: &'static str,
    trial: measure::Trial,
    coverage: &'static str,
}

struct Census {
    files: u64,
    dirs: u64,
    full: String,
    files_only: String,
}

fn census_of(tree: &Path) -> Result<Census, String> {
    let census = fixture::census(tree)?;
    if census.hidden != 0 {
        return Err(format!(
            "{}: fixture holds {} hidden names; coverage would not be comparable",
            tree.display(),
            census.hidden
        ));
    }
    let (files, dirs) = (census.files(), census.dirs());
    let mut items = census.items;
    let full = fixture::digest(&mut items);
    items.retain(|item| item.1 == b'f');
    Ok(Census {
        files,
        dirs,
        full,
        files_only: fixture::digest(&mut items),
    })
}

fn census_line(c: &Census) -> String {
    format!(
        "files={}\tdirs={}\tidentity_sha256={}\tfiles_sha256={}\n",
        c.files, c.dirs, c.full, c.files_only
    )
}

/// A new fixture, or with --reuse-fixtures an existing one whose census still matches its recorded line.
fn prepare(root: &Path, label: &str, args: &Args) -> Result<(PathBuf, Census), String> {
    let dir = root.join(format!("fixture-{label}"));
    if args.has("reuse-fixtures") && dir.join(fixture::FIXTURE_MARKER).is_file() {
        let census = census_of(&dir.join("tree"))?;
        let recorded = fs::read_to_string(dir.join("census.tsv")).map_err(|e| e.to_string())?;
        if recorded != census_line(&census) {
            return Err(format!(
                "reused fixture {label} differs from its recorded census"
            ));
        }
        return Ok((dir.join("tree"), census));
    }
    let tree = if label == "flea-source" {
        let repo = args.path("repo").unwrap_or_else(|| PathBuf::from("."));
        fixture::flea_source(root, &repo, &args.or("rev", "0ebbcffae"))?
    } else {
        fixture::synthetic(
            root,
            label
                .parse()
                .map_err(|_| format!("unknown fixture {label}"))?,
        )?
    };
    let census = census_of(&tree)?;
    fs::write(dir.join("census.tsv"), census_line(&census)).map_err(|e| e.to_string())?;
    Ok((tree, census))
}

fn host_info(args: &Args, threads: usize, tools: &[(String, Option<PathBuf>)]) -> String {
    let read = |p: &str| fs::read_to_string(p).unwrap_or_default().trim().to_string();
    let cpu = read("/proc/cpuinfo")
        .lines()
        .find(|l| l.starts_with("model name"))
        .unwrap_or("")
        .to_string();
    let mut out = format!(
        "host={}\nkernel={}\n{cpu}\nnproc={}\nthreads={threads}\nload_start={}\n",
        read("/proc/sys/kernel/hostname"),
        read("/proc/sys/kernel/osrelease"),
        default_threads(),
        read("/proc/loadavg")
    );
    let _ = writeln!(out, "cache={}\nclock=CLOCK_MONOTONIC ns (std::time::Instant), stage from first walk call to last entry taken", args.or("cache", "warm"));
    let _ = writeln!(out, "stage_cpu=getrusage self+reaped children at the stage stop; peak_rss=own VmHWM at the stage stop plus the external tool's wait4 ru_maxrss");
    let _ = writeln!(
        out,
        "harness_sha256={}",
        self_exe()
            .ok()
            .and_then(|p| fs::read(p).ok())
            .map_or("unavailable".into(), |b| fixture::sha256_hex(&b))
    );
    for (name, tool) in tools {
        if let Some(path) = tool {
            let _ = writeln!(
                out,
                "{name}={} {}",
                path.display(),
                first_line(path, "--version")
            );
        }
    }
    out
}

pub fn run(args: &Args) -> Result<(), String> {
    let root = args.path("root").ok_or("--root required")?;
    if args.has("create-root") {
        if !root.is_absolute() {
            return Err("--root must be absolute".into());
        }
        fs::create_dir(&root).map_err(|e| format!("{}: {e}", root.display()))?;
        fs::write(root.join(fixture::SANDBOX_MARKER), "flea-walkbench\n")
            .map_err(|e| e.to_string())?;
    }
    let root = fixture::marked_root(&root)?;
    let threads = args.number("threads", default_threads())?;
    let (runs, batches, noise) = (
        args.number("runs", 30)?,
        args.number("noise-batches", 2)?,
        args.number("noise-runs", 5)?,
    );
    let seed = args.number("seed", 20261025)? as u64;
    let cache = args.or("cache", "warm");
    let names: Vec<String> = args.get("adapters").map_or_else(
        || {
            adapters::ADAPTERS
                .iter()
                .map(|s| s.to_string())
                .chain(["harness-floor".into()])
                .collect()
        },
        |list| list.split(',').map(str::to_string).collect(),
    );
    let tools: Vec<(String, Option<PathBuf>)> = names
        .iter()
        .map(|n| {
            (
                n.clone(),
                match n.as_str() {
                    "fd" => args.path("fd"),
                    "ripgrep" => args.path("rg"),
                    _ => None,
                },
            )
        })
        .collect();
    if let Some((name, _)) = tools
        .iter()
        .find(|(n, t)| (n == "fd" || n == "ripgrep") && t.is_none())
    {
        return Err(format!(
            "{name} is on the roster but its binary was not given; refusing to drop it silently"
        ));
    }
    let out = root.join(format!("out-{seed}-{}", std::process::id()));
    fs::create_dir(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut info = host_info(args, threads, &tools);
    let host = info
        .lines()
        .next()
        .unwrap_or("host=?")
        .trim_start_matches("host=")
        .to_string();
    let exe = self_exe()?;
    let mut rng = stats::Rng::new(seed);
    let (mut raw, mut summary) = (
        format!("{HEADER}\n"),
        String::from(crate::run::SUMMARY_HEADER),
    );
    for label in args.or("fixtures", "100000").split(',') {
        let (tree, census) = prepare(&root, label, args)?;
        let _ = write!(
            info,
            "fixture={label}\troot={}\t{}",
            tree.display(),
            census_line(&census)
        );
        let mut configs: Vec<(String, &'static str)> = Vec::new();
        for (name, _) in &tools {
            if name != "flea-search" {
                configs.push((name.clone(), "walk"));
            }
            if name != "harness-floor" {
                configs.push((name.clone(), "index"));
            }
        }
        let mut rounds: Vec<(String, usize)> = (1..=batches)
            .flat_map(|b| (1..=noise).map(move |r| (format!("noise-{b}"), r)))
            .collect();
        rounds.extend((1..=runs).map(|r| ("measured".to_string(), r)));
        if cache == "warm" {
            rounds.insert(0, ("warmup".into(), 0));
        }
        let mut rows = Vec::new();
        for (phase, round) in rounds {
            rng.shuffle(&mut configs);
            for (order, (name, mode)) in configs.iter().enumerate() {
                let gate = measure::cache_gate(&cache, args.get("cold-hook"))?;
                let tool = tools
                    .iter()
                    .find(|(n, _)| n == name)
                    .and_then(|(_, t)| t.as_deref());
                let trial = measure::trial(&exe, name, &tree, threads, mode, tool)?;
                let coverage = if name == "harness-floor" {
                    "control"
                } else {
                    measure::coverage(
                        &trial,
                        census.files,
                        census.dirs,
                        &census.full,
                        &census.files_only,
                        name != "ripgrep",
                        *mode == "index",
                    )
                };
                let t = &trial;
                let _ = writeln!(raw, "{host}\t{label}\t{phase}\t{round}\t{order}\t{name}\t{mode}\t{threads}\t{gate}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{coverage}\t{}",
                    t.wall_ns, t.stage_ns, t.user_us, t.sys_us, t.maxrss_kib, t.stage_cpu_us, t.hwm_kib, t.spawn_hwm_kib, t.tool_maxrss_kib, t.files, t.dirs, census.files, census.dirs, t.errors, t.exit, t.digest);
                rows.push(Row {
                    phase: phase.clone(),
                    round,
                    adapter: name.clone(),
                    mode,
                    trial,
                    coverage,
                });
            }
        }
        if census_of(&tree)?.full != census.full {
            return Err(format!("fixture {label} changed during the run"));
        }
        summarize(
            &mut summary,
            label,
            &rows,
            &args.or("reference", "flea-search"),
            args.number("resamples", 10000)?,
            &mut rng,
        );
    }
    let info = format!(
        "{info}load_end={}\n",
        fs::read_to_string("/proc/loadavg")
            .unwrap_or_default()
            .trim()
    );
    for (name, body) in [
        ("RUN.txt", &info),
        ("RAW.tsv", &raw),
        ("SUMMARY.tsv", &summary),
    ] {
        fs::write(out.join(name), body).map_err(|e| e.to_string())?;
        if args.has("stdout") {
            println!("=== {name}\n{body}");
        }
    }
    println!("OUT={}", out.display());
    Ok(())
}

// n_ok counts metric-usable rounds; n_failed counts failed coverage/execution; n_not_measured counts covered rounds without that metric.
pub const SUMMARY_HEADER: &str = "fixture\tadapter\tmode\tmetric\tn_ok\tn_failed\tn_not_measured\tcoverage\tmedian\tci95_low\tci95_high\treference\tpaired_median_diff\tpaired_ci95_low\tpaired_ci95_high\tpaired_n\n";

type Metric = fn(&measure::Trial) -> Option<f64>;

#[cfg(test)]
#[path = "run_summary_tests.rs"]
mod summary_tests;

/// Only rounds with full coverage enter a speed, CPU or memory interval; failures stay counted.
fn summarize(
    out: &mut String,
    label: &str,
    rows: &[Row],
    reference: &str,
    resamples: usize,
    rng: &mut stats::Rng,
) {
    // Stage clock, stage CPU and peak memory stop before coverage work; process wall is walk mode only for that reason.
    let metrics: [(&str, Metric); 4] = [
        ("stage_ms", |t| Some(t.stage_ns as f64 / 1e6)),
        ("process_wall_ms", |t| Some(t.wall_ns as f64 / 1e6)),
        ("stage_cpu_ms", |t| {
            (t.stage_cpu_us >= 0).then_some(t.stage_cpu_us as f64 / 1e3)
        }),
        ("peak_rss_mib", |t| {
            (t.hwm_kib >= 0 && t.tool_maxrss_kib >= 0)
                .then_some((t.hwm_kib as f64 + t.tool_maxrss_kib as f64) / 1024.0)
        }),
    ];
    let mut keys: Vec<(&str, &str)> = rows.iter().map(|r| (r.adapter.as_str(), r.mode)).collect();
    keys.sort_unstable();
    keys.dedup();
    for (adapter, mode) in keys {
        let select = |name: &str| -> Vec<&Row> {
            rows.iter()
                .filter(|r| r.phase == "measured" && r.adapter == name && r.mode == mode)
                .collect()
        };
        let mine = select(adapter);
        let good: Vec<&&Row> = mine
            .iter()
            .filter(|r| r.coverage == "ok" || r.coverage == "control")
            .collect();
        let theirs: Vec<&Row> = select(reference)
            .into_iter()
            .filter(|r| r.coverage == "ok")
            .collect();
        for (metric, value) in metrics {
            if metric == "process_wall_ms" && mode == "index" {
                continue;
            }
            let series: Vec<(usize, f64)> = good
                .iter()
                .filter_map(|r| value(&r.trial).map(|value| (r.round, value)))
                .collect();
            let plain: Vec<f64> = series.iter().map(|s| s.1).collect();
            let status = if series.is_empty() && !good.is_empty() {
                "not-measured"
            } else if series.len() == mine.len() {
                "complete"
            } else {
                "incomplete"
            };
            let (m, lo, hi) = if plain.is_empty() {
                ("-".into(), "-".into(), "-".into())
            } else {
                let (m, lo, hi) = stats::bootstrap(&plain, resamples, rng);
                (format!("{m:.3}"), format!("{lo:.3}"), format!("{hi:.3}"))
            };
            let refs: Vec<(usize, f64)> = theirs
                .iter()
                .filter_map(|r| value(&r.trial).map(|value| (r.round, value)))
                .collect();
            let pair = if adapter == reference {
                None
            } else {
                stats::paired(&series, &refs, resamples, rng)
            };
            let (pm, plo, phi, pn) = pair
                .map_or(("-".into(), "-".into(), "-".into(), 0), |(a, b, c, n)| {
                    (format!("{a:.3}"), format!("{b:.3}"), format!("{c:.3}"), n)
                });
            let _ = writeln!(out, "{label}\t{adapter}\t{mode}\t{metric}\t{}\t{}\t{}\t{status}\t{m}\t{lo}\t{hi}\t{reference}\t{pm}\t{plo}\t{phi}\t{pn}",
                series.len(), mine.len() - good.len(), good.len() - series.len());
        }
    }
}
