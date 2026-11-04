use super::*;

#[test]
#[ignore]
fn search_match_scale_probe() {
    for count in [100_000, 1_000_000] {
        let (index, _) = super::fixture_tests::synthetic(count, false);
        let rows: Vec<_> = (0..index.len()).collect();
        for query in ["render_frame.rs", "redner_frame.rs", "editor/src/components/render", ".svg", "archive/2024/client/reports/quarterly"] {
            let matcher = crate::backend::searchmatch::Matcher::new(query);
            let start = std::time::Instant::now();
            let mut hits = score_all(&matcher, &rows, &index);
            let matches = hits.len();
            let window = matches.min(350);
            let _rest = select_head(&mut hits, window, |hit| index.path_len(hit.1), |a, b| rank_cmp(&index, a, b));
            println!("SEARCH_MATCH_PROBE entries={count} query={query:?} matches={matches} core_us={:.3}", start.elapsed().as_secs_f64() * 1e6);
        }
    }
}

// FS-2 query root-cause probe: per typed prefix, full rescan against narrowing from the previous prefix's matches.
#[test]
#[ignore]
fn search_fs2_query_phase_probe() {
    let med = |mut v: Vec<f64>| { v.sort_by(|a, b| a.partial_cmp(b).unwrap()); v[v.len() / 2] };
    for count in [100_000, 1_000_000] {
        let (index, _) = super::fixture_tests::synthetic(count, false);
        let rows: Vec<_> = (0..index.len()).collect();
        let (mut full_scratch, mut narrow_scratch) = (QueryScratch::default(), QueryScratch::default());
        for query in ["render_frame.rs", "redner_frame.rs", "editor/src/components/render", ".svg", "archive/2024/client/reports/quarterly"] {
            let mut previous: Option<Vec<usize>> = None;
            for end in 1..=query.len() {
                let text = &query[..end];
                let matcher = crate::backend::searchmatch::Matcher::new(text);
                let (mut score, mut select, mut clone, mut narrow) = (vec![], vec![], vec![], vec![]);
                let mut full_ids = vec![];
                let mut narrow_ids = vec![];
                for _ in 0..7 {
                    let t = std::time::Instant::now();
                    let mut hits = score_cached(&matcher, &rows, &index, &mut full_scratch);
                    score.push(t.elapsed().as_secs_f64() * 1e3);
                    let t = std::time::Instant::now();
                    let all: Vec<usize> = hits.iter().map(|(_, row)| *row).collect();
                    clone.push(t.elapsed().as_secs_f64() * 1e3);
                    let window = hits.len().min(350);
                    let t = std::time::Instant::now();
                    let _rest = select_head(&mut hits, window, |hit| index.path_len(hit.1), |a, b| rank_cmp(&index, a, b));
                    select.push(t.elapsed().as_secs_f64() * 1e3);
                    full_ids = all;
                    if let Some(prev) = &previous {
                        let t = std::time::Instant::now();
                        let hits = score_cached(&matcher, prev, &index, &mut narrow_scratch);
                        narrow.push(t.elapsed().as_secs_f64() * 1e3);
                        narrow_ids = hits.iter().map(|(_, row)| *row).collect();
                    }
                }
                let (mut a, mut b) = (full_ids.clone(), narrow_ids.clone());
                a.sort_unstable();
                b.sort_unstable();
                let lost = if previous.is_some() { a.iter().filter(|r| b.binary_search(r).is_err()).count() } else { 0 };
                println!("FS2_QPROBE entries={count} query={text:?} can_narrow_now={} hits={} score_ms={:.3} clone_ms={:.3} select_ms={:.3} narrow_score_ms={} narrow_lost={lost}",
                    previous.is_some() && crate::backend::searchmatch::Matcher::can_narrow(&query[..end - 1], text),
                    full_ids.len(), med(score), med(clone), med(select),
                    if narrow.is_empty() { "-".to_string() } else { format!("{:.3}", med(narrow)) });
                previous = Some(full_ids);
            }
        }
    }
}

#[path = "searchnarrow_tests.rs"]
mod narrow_tests;

// Diagnostic only: real typed-query chains, with work counts instead of timing gates.
#[test]
#[ignore]
fn l_search_tail_probe() {
    use crate::backend::searchsession::Session;
    use crate::backend::testdir::TestDir;
    use std::hash::{Hash, Hasher};
    use std::sync::atomic::Ordering;
    let dir = TestDir::new("l-search-tail");
    let scope = dir.path().to_str().unwrap();
    for count in [4902, 100_000, 1_000_000] {
        let start = std::time::Instant::now();
        let index = if count == 4902 { l_search_source_index() } else { super::fixture_tests::synthetic(count, false).0 };
        let index_ms = start.elapsed().as_secs_f64() * 1000.0;
        let mut session = Session::open_sync(scope, scope, false);
        session.priors = None;
        session.score_scratch.pinned_cores = Some(2);
        *session.index.lock().unwrap() = index;
        let ready = l_search_resources();
        println!("L_SEARCH_INDEX entries={count} index_ms={index_ms:.3} pss_kib={}", ready.1);
        let mut gen = 0;
        for round in 0..7 {
            let before = l_search_resources();
            let queries = if count == 4902 { ["fuzzy.rs", "fzuzy.rs", "backend/searchindex", ".qml", "src/backend/searchw"] }
                else { ["render_frame.rs", "redner_frame.rs", "editor/src/components/render", ".svg", "archive/2024/client/reports/quarterly"] };
            for query in queries {
                session.last_query.clear();
                for end in 1..=query.len() {
                    let text = &query[..end];
                    {
                        let index = session.index.lock().unwrap();
                        index.path_dp.store(0, Ordering::Relaxed);
                        index.name_dp.store(0, Ordering::Relaxed);
                    }
                    gen += 1;
                    let start = std::time::Instant::now();
                    let answer = session.query(gen, text, 350).unwrap();
                    let ms = start.elapsed().as_secs_f64() * 1000.0;
                    let phases = session.counters.phases;
                    let index = session.index.lock().unwrap();
                    let mut hash = std::collections::hash_map::DefaultHasher::new();
                    answer.rows.hash(&mut hash);
                    session.last_matches.hash(&mut hash);
                    println!("L_SEARCH entries={count} round={round} query={text:?} ms={ms:.4} score_ms={:.4} resolve_ms={:.4} select_ms={:.4} total={} names={} paths={} path_dp={} name_dp={} digest={:016x}",
                        phases.score.as_secs_f64() * 1000.0, phases.resolve.as_secs_f64() * 1000.0,
                        phases.select.as_secs_f64() * 1000.0, answer.total, index.names_scored.load(Ordering::Relaxed),
                        index.paths_scored.load(Ordering::Relaxed), index.path_dp.load(Ordering::Relaxed),
                        index.name_dp.load(Ordering::Relaxed), hash.finish());
                }
            }
            let after = l_search_resources();
            println!("L_SEARCH_CHAIN entries={count} round={round} cpu_ticks={} pss_kib={}", after.0 - before.0, after.1);
        }
    }
}

fn l_search_source_index() -> Index {
    let manifest = crate::jsondoc::parse(include_str!("../../tools/flea-searchbench/source-manifest.json")).unwrap();
    let mut index = Index::new();
    let entries = manifest.as_array().unwrap();
    assert_eq!(entries.len(), 4902, "the certified source inventory must stay fixed");
    for entry in entries {
        let entry = entry.as_array().unwrap();
        assert!(index.push(entry[0].as_str().unwrap(), entry[1].as_str() == Some("d")));
    }
    index.finish();
    assert_eq!(index.len(), entries.len());
    index
}

// Linux process CPU ticks include every scoring thread; PSS is the whole test process, not a product field claim.
fn l_search_resources() -> (u64, u64) {
    // Sample input: "123 (flea-test) R 1 123 123 0 -1 0 0 0 0 0 17 5" gives 22 CPU ticks.
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap();
    let fields: Vec<_> = stat.rsplit_once(')').unwrap().1.split_whitespace().collect();
    let ticks = fields[11].parse::<u64>().unwrap() + fields[12].parse::<u64>().unwrap();
    // Sample input: "Pss:                1234 kB\n" gives 1234 KiB.
    let memory = std::fs::read_to_string("/proc/self/smaps_rollup").unwrap();
    let pss = memory.lines().find(|line| line.starts_with("Pss:")).unwrap().split_whitespace().nth(1).unwrap().parse().unwrap();
    (ticks, pss)
}

// The certified 100k held-out targets, drawn with seed 20261027.
const L_SEARCH_HELDOUT: [(&str, &str); 64] = [
    ("render_window_00020054_detail_detail_detail_detail.zip", "projects/folder_02/folder_05/folder_03/render_window_00020054_detail_detail_detail_detail.zip"),
    ("reoprt_meeting_41.rs", "Music/folder_02/folder_05/folder_01/report_meeting_41.rs"),
    ("folder_07/folder_07/meeting_model_00016961", "projects/folder_04/folder_07/folder_07/meeting_model_00016961_detail.flac"),
    ("Music/folder_05/folder_03/folder_00/holiday", "Music/folder_05/folder_03/folder_00/holiday_holiday_61.rs"),
    ("report_model_68_detail_detail_detail_detail.jpg", "Videos/folder_04/folder_05/folder_07/report_model_68_detail_detail_detail_detail.jpg"),
    ("drfat_parser_00024587_detail_detail.h", "Documents/folder_05/folder_03/draft_parser_00024587_detail_detail.h"),
    ("folder_02/folder_06/window_parser_76", "Pictures/folder_04/folder_02/folder_06/window_parser_76_detail_detail_detail.py"),
    ("shared/folder_05/folder_05/folder_06/budget", "shared/folder_05/folder_05/folder_06/budget_report_00013255.mp4"),
    ("draft_test_00029795.qml", "shared/folder_04/folder_05/folder_05/draft_test_00029795.qml"),
    ("bugdet_invoice_00003367_detail_detail.txt", "Documents/folder_05/folder_02/folder_02/budget_invoice_00003367_detail_detail.txt"),
    ("folder_06/folder_05/invoice_render_00003628", "Pictures/folder_05/folder_06/folder_05/invoice_render_00003628_detail_detail_detail.json"),
    ("archive/folder_01/folder_02/folder_07/test", "archive/folder_01/folder_02/folder_07/test_render_00064546_detail.c"),
    ("draft_holiday_00055235.qml", "archive/folder_05/folder_07/folder_07/draft_holiday_00055235.qml"),
    ("reoprt_window_45_detail_detail_detail_detail.jpg", "projects/folder_04/folder_03/folder_01/report_window_45_detail_detail_detail_detail.jpg"),
    ("folder_07/folder_03/report_holiday_92", "Videos/folder_07/folder_03/report_holiday_92_detail_detail_detail.py"),
    ("Downloads/folder_01/folder_07/folder_05/parser", "Downloads/folder_01/folder_07/folder_05/parser_notes_00005348_detail_detail_detail.pdf"),
    ("holiday_invoice_93_detail_detail_detail.py", "Downloads/folder_04/folder_01/folder_06/holiday_invoice_93_detail_detail_detail.py"),
    ("drfat_render_00095507_detail_detail.h", "projects/folder_04/folder_07/folder_01/draft_render_00095507_detail_detail.h"),
    ("folder_04/folder_06/meeting_render_00056765", "Downloads/folder_04/folder_06/meeting_render_00056765.qml"),
    ("Videos/folder_02/folder_04/folder_02/model", "Videos/folder_02/folder_04/folder_02/model_draft_67.rs"),
    ("model_window_49.rs", "archive/folder_04/folder_05/folder_02/model_window_49.rs"),
    ("drfat_test_00092435.qml", "Downloads/folder_02/folder_05/folder_00/draft_test_00092435.qml"),
    ("folder_06/folder_06/meeting_meeting_00027137", "Documents/folder_01/folder_06/folder_06/meeting_meeting_00027137_detail_detail.h"),
    ("projects/folder_05/folder_05/folder_05/model", "projects/folder_05/folder_05/folder_05/model_model_60.rs"),
    ("holiday_report_63_detail.md", "Pictures/folder_04/folder_07/folder_03/holiday_report_63_detail.md"),
    ("pasrer_invoice_00022664_detail_detail_detail_detail.zip", "Videos/folder_02/folder_00/folder_05/parser_invoice_00022664_detail_detail_detail_detail.zip"),
    ("folder_07/folder_05/render_budget_00026294", "Music/folder_01/folder_07/folder_05/render_budget_00026294_detail_detail_detail_detail.zip"),
    ("Music/folder_05/folder_06/parser", "Music/folder_05/folder_06/parser_parser_00063608_detail_detail_detail.pdf"),
    ("holiday_render_21_detail_detail_detail.py", "Videos/folder_04/folder_03/folder_04/holiday_render_21_detail_detail_detail.py"),
    ("redner_notes_00053438_detail_detail_detail.pdf", "archive/folder_04/folder_02/folder_03/render_notes_00053438_detail_detail_detail.pdf"),
    ("folder_05/folder_06/test_notes_00017014", "Downloads/folder_02/folder_05/folder_06/test_notes_00017014_detail_detail_detail_detail.js"),
    ("Documents/folder_05/folder_02/folder_03/holiday", "Documents/folder_05/folder_02/folder_03/holiday_window_76_detail.md"),
    ("meeting_window_00048281_detail.flac", "archive/folder_03/folder_07/folder_04/meeting_window_00048281_detail.flac"),
    ("moedl_notes_71_detail.md", "Music/folder_04/folder_01/folder_04/model_notes_71_detail.md"),
    ("folder_06/folder_00/notes_test_00011497", "projects/folder_06/folder_00/notes_test_00011497_detail_detail.txt"),
    ("archive/folder_05/folder_03/folder_03/draft", "archive/folder_05/folder_03/folder_03/draft_meeting_00001511_detail.flac"),
    ("meeting_test_00015389_detail_detail_detail_detail.zip", "Music/folder_01/folder_02/folder_00/meeting_test_00015389_detail_detail_detail_detail.zip"),
    ("drfat_test_00086243_detail_detail_detail.pdf", "archive/folder_05/folder_05/draft_test_00086243_detail_detail_detail.pdf"),
    ("folder_04/folder_05/meeting_invoice_00028133", "Documents/folder_04/folder_05/meeting_invoice_00028133_detail_detail_detail.pdf"),
    ("Videos/folder_03/folder_03/folder_02/model", "Videos/folder_03/folder_03/folder_02/model_holiday_09_detail_detail_detail_detail.jpg"),
    ("render_model_00040142_detail_detail.h", "shared/folder_01/folder_05/folder_05/render_model_00040142_detail_detail.h"),
    ("hoilday_invoice_12_detail.md", "shared/folder_01/folder_05/folder_06/holiday_invoice_12_detail.md"),
    ("folder_04/folder_04/test_model_00084790", "shared/folder_05/folder_04/folder_04/test_model_00084790.mp4"),
    ("Music/folder_07/folder_04/window", "Music/folder_07/folder_04/window_model_52_detail_detail_detail_detail.jpg"),
    ("test_report_00082090.mp4", "Music/folder_05/folder_06/folder_01/test_report_00082090.mp4"),
    ("meteing_test_00024749_detail_detail_detail_detail.zip", "Documents/folder_01/folder_03/meeting_test_00024749_detail_detail_detail_detail.zip"),
    ("folder_01/folder_04/test_holiday_00035362", "Documents/folder_04/folder_01/folder_04/test_holiday_00035362_detail_detail.txt"),
    ("Videos/folder_03/folder_07/folder_00/meeting", "Videos/folder_03/folder_07/folder_00/meeting_report_00010949_detail_detail_detail_detail.zip"),
    ("invoice_render_00039916_detail.c", "Videos/folder_03/folder_05/folder_04/invoice_render_00039916_detail.c"),
    ("widnow_report_90_detail_detail.svg", "Videos/folder_05/folder_04/folder_03/window_report_90_detail_detail.svg"),
    ("folder_01/folder_00/report_model_73", "Documents/folder_05/folder_01/folder_00/report_model_73_detail_detail.svg"),
    ("archive/folder_07/folder_06/invoice", "archive/folder_07/folder_06/invoice_holiday_00051196_detail.c"),
    ("notes_parser_00042289_detail_detail_detail_detail.js", "Downloads/folder_04/folder_02/folder_02/notes_parser_00042289_detail_detail_detail_detail.js"),
    ("meteing_budget_00076697_detail_detail.h", "Downloads/folder_04/folder_04/meeting_budget_00076697_detail_detail.h"),
    ("folder_07/folder_00/holiday_notes_52", "shared/folder_05/folder_07/folder_00/holiday_notes_52.rs"),
    ("Pictures/folder_01/folder_04/folder_04/report", "Pictures/folder_01/folder_04/folder_04/report_holiday_73_detail.md"),
    ("render_draft_00076742_detail_detail.h", "Pictures/folder_03/folder_06/folder_04/render_draft_00076742_detail_detail.h"),
    ("moedl_model_35.rs", "Documents/folder_04/folder_03/folder_00/model_model_35.rs"),
    ("folder_07/folder_01/report_model_21", "Documents/folder_01/folder_07/folder_01/report_model_21_detail_detail_detail_detail.jpg"),
    ("Music/folder_05/folder_01/folder_01/parser", "Music/folder_05/folder_01/folder_01/parser_render_00074480.qml"),
    ("budget_budget_00078139_detail_detail_detail_detail.js", "archive/folder_01/folder_00/folder_00/budget_budget_00078139_detail_detail_detail_detail.js"),
    ("bugdet_render_00092479_detail_detail_detail_detail.js", "projects/folder_05/folder_06/folder_03/budget_render_00092479_detail_detail_detail_detail.js"),
    ("folder_00/folder_01/invoice_invoice_00094228", "projects/folder_04/folder_00/folder_01/invoice_invoice_00094228_detail_detail_detail.json"),
    ("Pictures/folder_01/folder_07/folder_05/budget", "Pictures/folder_01/folder_07/folder_05/budget_window_00084571_detail.c"),
];

#[test]
#[ignore]
fn l_search_tail_heldout_probe() {
    use crate::backend::searchprior::alloc_tests::fixture_priors;
    use crate::backend::searchsession::Session;
    use crate::backend::testdir::TestDir;
    use std::hash::{Hash, Hasher};
    let dir = TestDir::new("l-search-heldout");
    let scope = dir.path().to_str().unwrap();
    let absolute = |path: &[u8]| {
        let mut full = dir.path().as_os_str().as_bytes().to_vec();
        full.push(b'/');
        full.extend_from_slice(path);
        full
    };
    for profile in ["native", "fresh", "rich", "equal-history"] {
        let (index, mut inventory) = super::fixture_tests::synthetic(100_000, true);
        inventory.sort();
        let targets: std::collections::BTreeSet<_> = L_SEARCH_HELDOUT.iter().map(|(_, target)| target.as_bytes()).collect();
        assert!(targets.iter().all(|target| inventory.iter().any(|(path, kind)| path == target && !kind)));
        let (files, folders) = match profile {
            "rich" => (targets.iter().map(|path| absolute(path)).collect(), Vec::new()),
            "equal-history" => (
                inventory.iter().filter(|(path, dir)| !dir && !targets.contains(path.as_slice())).take(32).map(|(path, _)| absolute(path)).collect(),
                inventory.iter().filter(|(_, dir)| *dir).take(16).map(|(path, _)| absolute(path)).collect()),
            _ => (Vec::new(), Vec::new()),
        };
        let priors = fixture_priors(files, folders);
        let mut session = Session::open_sync(scope, scope, false);
        session.priors = if priors.is_empty() { None } else { Some(Box::leak(Box::new(priors))) };
        session.score_scratch.pinned_cores = Some(2);
        *session.index.lock().unwrap() = index;
        let (mut gen, mut first, mut censored) = (0, 0, 0);
        for (chain, &(query, target)) in L_SEARCH_HELDOUT.iter().enumerate() {
            session.last_query.clear();
            for end in 1..=query.len() {
                gen += 1;
                let answer = session.query(gen, &query[..end], 350).unwrap();
                let rank = answer.rows.iter().position(|(path, _)| path == target).map_or(-1, |at| at as i32 + 1);
                let file_rank = answer.rows.iter().filter(|(_, dir)| !dir).position(|(path, _)| path == target).map_or(-1, |at| at as i32 + 1);
                first += usize::from(file_rank == 1);
                censored += usize::from(rank == -1);
                let mut digest = std::collections::hash_map::DefaultHasher::new();
                answer.rows.hash(&mut digest);
                session.last_matches.hash(&mut digest);
                println!("L_SEARCH_HELDOUT profile={profile} chain={chain} end={end} total={} rank={rank} file_rank={file_rank} digest={:016x}", answer.total, digest.finish());
            }
        }
        assert_eq!(gen, 2506, "every certified prefix must run");
        println!("L_SEARCH_QUALITY profile={profile} prefixes={gen} files_first={first} censored={censored}");
    }
}

// Six A/A pilots, then 30 alternating A/B pairs in one process, so machine drift cannot masquerade as a source win.
#[test]
#[ignore]
fn l_search_tail_paired_probe() {
    use crate::backend::searchmatch::parent_score;
    use crate::backend::searchsession::Session;
    use crate::backend::testdir::TestDir;
    use std::hash::{Hash, Hasher};
    let dir = TestDir::new("l-search-paired");
    let scope = dir.path().to_str().unwrap();
    for count in [4902, 100_000, 1_000_000] {
        let started = std::time::Instant::now();
        let index = if count == 4902 { l_search_source_index() } else { super::fixture_tests::synthetic(count, false).0 };
        println!("L_SEARCH_PAIR_INDEX entries={count} index_ms={:.4}", started.elapsed().as_secs_f64() * 1000.0);
        let mut session = Session::open_sync(scope, scope, false);
        session.priors = None;
        session.score_scratch.pinned_cores = Some(2);
        *session.index.lock().unwrap() = index;
        let queries = if count == 4902 { ["fuzzy.rs", "fzuzy.rs", "backend/searchindex", ".qml", "src/backend/searchw"] }
            else { ["render_frame.rs", "redner_frame.rs", "editor/src/components/render", ".svg", "archive/2024/client/reports/quarterly"] };
        let mut gen = 0;
        for pair in 0..36 {
            let phase = if pair < 6 { "noise" } else { "measured" };
            let mut digests = [0; 2];
            let order = if pair % 2 == 0 { [0, 1] } else { [1, 0] };
            for arm in order {
                parent_score(pair >= 6 && arm == 1);
                let mut times = Vec::new();
                let mut digest = std::collections::hash_map::DefaultHasher::new();
                let before = l_search_resources();
                for query in queries {
                    session.last_query.clear();
                    for end in 1..=query.len() {
                        gen += 1;
                        let text = &query[..end];
                        let started = std::time::Instant::now();
                        let answer = session.query(gen, text, 350).unwrap();
                        times.push((text, started.elapsed().as_secs_f64() * 1000.0));
                        answer.total.hash(&mut digest);
                        answer.rows.hash(&mut digest);
                    }
                }
                let after = l_search_resources();
                digests[arm] = digest.finish();
                for &(query, ms) in &times {
                    println!("L_SEARCH_PAIR_KEY entries={count} phase={phase} pair={pair} arm={arm} query={query:?} ms={ms:.5}");
                }
                let mut values: Vec<_> = times.iter().map(|(_, ms)| *ms).collect();
                values.sort_by(f64::total_cmp);
                println!("L_SEARCH_PAIR entries={count} phase={phase} pair={pair} arm={arm} p50={:.5} p95={:.5} cpu_ticks={} pss_kib={} digest={:016x}",
                    values[values.len() / 2], values[(values.len() * 95).div_ceil(100) - 1],
                    after.0 - before.0, after.1, digests[arm]);
            }
            assert_eq!(digests[0], digests[1], "{count} pair={pair}: every prefix's count and ranked head must match");
        }
    }
    parent_score(true);
}
