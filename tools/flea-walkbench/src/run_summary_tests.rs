use super::*;

fn row(adapter: &str, round: usize, cpu: i64, hwm: i64, tool: i64) -> Row {
    Row {
        phase: "measured".into(),
        round,
        adapter: adapter.into(),
        mode: "index",
        trial: measure::Trial {
            wall_ns: 1_000_000,
            stage_ns: if adapter == "flea-search" {
                2_000_000
            } else {
                3_000_000
            },
            user_us: 0,
            sys_us: 0,
            maxrss_kib: 1,
            stage_cpu_us: cpu,
            hwm_kib: hwm,
            spawn_hwm_kib: -1,
            tool_maxrss_kib: tool,
            files: 10,
            dirs: 2,
            errors: 0,
            digest: "full".into(),
            exit: 0,
        },
        coverage: "ok",
    }
}

fn table(rows: &[Row]) -> Vec<Vec<String>> {
    let mut out = String::new();
    summarize(
        &mut out,
        "fixture",
        rows,
        "flea-search",
        20,
        &mut stats::Rng::new(7),
    );
    out.lines()
        .map(|line| line.split('\t').map(str::to_string).collect())
        .collect()
}

fn cell<'a>(table: &'a [Vec<String>], adapter: &str, metric: &str) -> &'a [String] {
    table
        .iter()
        .find(|row| row[1] == adapter && row[3] == metric)
        .unwrap()
}

#[test]
fn missing_cpu_and_memory_are_not_measured_but_timing_stays_valid() {
    let result = table(&[
        row("flea-search", 1, -1, -1, -1),
        row("rival", 1, 1000, 1024, 0),
    ]);
    for metric in ["stage_cpu_ms", "peak_rss_mib"] {
        let mine = cell(&result, "flea-search", metric);
        assert_eq!((mine[4].as_str(), mine[7].as_str()), ("0", "not-measured"));
        assert_eq!(&mine[8..11], ["-", "-", "-"]);
        let rival = cell(&result, "rival", metric);
        assert_eq!(rival[7], "complete");
        assert_eq!(rival[8], "1.000");
        assert_eq!(&rival[12..16], ["-", "-", "-", "0"]);
    }
    let mine = cell(&result, "flea-search", "stage_ms");
    assert_eq!(
        (mine[4].as_str(), mine[7].as_str(), mine[8].as_str()),
        ("1", "complete", "2.000")
    );
    let rival = cell(&result, "rival", "stage_ms");
    assert_eq!((rival[12].as_str(), rival[15].as_str()), ("1.000", "1"));
}

#[test]
fn missing_rounds_are_excluded_from_medians_and_paired_differences() {
    let result = table(&[
        row("flea-search", 1, -1, 1024, -1),
        row("flea-search", 2, 2000, 1024, 0),
        row("rival", 1, 9000, 9999, 0),
        row("rival", 2, 5000, 2048, 0),
    ]);
    let mine = cell(&result, "flea-search", "stage_cpu_ms");
    assert_eq!(
        (mine[4].as_str(), mine[7].as_str(), mine[8].as_str()),
        ("1", "incomplete", "2.000")
    );
    let rival = cell(&result, "rival", "stage_cpu_ms");
    assert_eq!(
        (rival[8].as_str(), rival[12].as_str(), rival[15].as_str()),
        ("7.000", "3.000", "1")
    );
    let memory = cell(&result, "rival", "peak_rss_mib");
    assert_eq!((memory[12].as_str(), memory[15].as_str()), ("1.000", "1"));
}

#[test]
fn zero_cpu_and_memory_remain_measured_values() {
    let result = table(&[row("flea-search", 1, 0, 0, 0), row("rival", 1, 0, 0, 0)]);
    for metric in ["stage_cpu_ms", "peak_rss_mib"] {
        for adapter in ["flea-search", "rival"] {
            let measured = cell(&result, adapter, metric);
            assert_eq!(
                (
                    measured[4].as_str(),
                    measured[7].as_str(),
                    measured[8].as_str()
                ),
                ("1", "complete", "0.000")
            );
        }
        let rival = cell(&result, "rival", metric);
        assert_eq!((rival[12].as_str(), rival[15].as_str()), ("0.000", "1"));
    }
}

#[test]
fn summary_reports_usable_failed_and_unavailable_counts() {
    let mut failed = row("flea-search", 3, -1, 1024, 0);
    failed.coverage = "error";
    failed.trial.exit = 1;
    let rows = [
        row("flea-search", 1, -1, 1024, 0),
        row("flea-search", 2, 2000, 1024, 0),
        failed,
    ];
    let mut output = SUMMARY_HEADER.to_string();
    summarize(
        &mut output,
        "fixture",
        &rows,
        "flea-search",
        20,
        &mut stats::Rng::new(7),
    );
    let mut lines = output.lines();
    let header: Vec<_> = lines.next().unwrap().split('\t').collect();
    let column = |name| {
        header
            .iter()
            .position(|value| *value == name)
            .expect("metric availability count is explicit")
    };
    let records: Vec<Vec<_>> = lines.map(|line| line.split('\t').collect()).collect();
    let mut metrics: Vec<_> = records
        .iter()
        .map(|fields| fields[column("metric")])
        .collect();
    metrics.sort_unstable();
    assert_eq!(records.len(), 3);
    assert_eq!(metrics, ["peak_rss_mib", "stage_cpu_ms", "stage_ms"]);
    assert_eq!(
        metrics
            .iter()
            .filter(|metric| **metric == "stage_cpu_ms")
            .count(),
        1
    );
    for fields in records {
        let usable: usize = fields[column("n_ok")].parse().unwrap();
        let failed: usize = fields[column("n_failed")].parse().unwrap();
        let missing: usize = fields[column("n_not_measured")].parse().unwrap();
        assert_eq!(usable + failed + missing, 3);
        if fields[column("metric")] == "stage_cpu_ms" {
            assert_eq!((usable, failed, missing), (1, 1, 1));
        } else {
            assert_eq!((usable, failed, missing), (2, 1, 0));
        }
    }
}
