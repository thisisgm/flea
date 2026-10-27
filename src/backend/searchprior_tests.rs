use super::*;
use crate::backend::searchindex::{rank_cmp, score_all};
use crate::backend::searchmatch::Matcher;
use crate::backend::testdir::TestDir;

fn hits(index: &Index, query: &str) -> Vec<(i32, usize)> {
    score_all(&Matcher::new(query), &(0..index.len()).collect::<Vec<_>>(), index)
}

fn ordered(index: &Index, mut hits: Vec<(i32, usize)>) -> Vec<String> {
    hits.sort_by(|a, b| rank_cmp(index, a, b));
    hits.into_iter().map(|(_, row)| index.path(row)).collect()
}

#[test]
fn searchprior_xml_reads_real_bookmarks_only_and_decodes_raw_local_identities() {
    let text = concat!("<?xml version='1.0'?><xbel version='1.0' xmlns:bookmark='urn:bookmark'>",
        "<!-- <bookmark href='file:///fake'/> --><title>&lt;bookmark href='file:///text'/&gt;</title>",
        "<bookmark href='FILE://localhost/work/a%20%26%FF.rs' visited='2026-10-18T01:02:03Z'>",
        "<info><metadata><bookmark:application name='a &quot;b&quot;' count='3'/></metadata></info></bookmark>",
        "<bookmark href='file:///work/a&#x26;b.rs' modified='2026-10-17T01:02:03Z'/>",
        "<bookmark href='https://example.org/a'/><bookmark href='file://remote/work/a'/>",
        "<info><bookmark href='file:///nested'/></info></xbel>");
    assert_eq!(xbel(text).unwrap(), [b"/work/a &\xff.rs".to_vec(), b"/work/a&b.rs".to_vec()]);
    for bad in ["file:///work/a%", "file:///work/a%0", "file:///work/a%gg", "file:///work/a%00", "file:///work/a%0a", "file:///work/../a", "file:///work//a", "file:///work/a?query", "file:///work/a#fragment"] {
        assert!(uri(bad).is_none(), "{bad}");
    }
    assert_eq!(uri("file:///work/a%23b%3Fc"), Some(b"/work/a#b?c".to_vec()));
}

#[test]
fn searchprior_corrupt_or_truncated_xml_drops_every_record_without_resolving_entities() {
    for text in ["", "<xbel>", "<xbel><bookmark href='file:///a'/>", "<xbel><bookmark href='file:///a'/></wrong>",
        "<xbel/><xbel/>", "<!DOCTYPE xbel SYSTEM 'file:///private'><xbel/>",
        "<xbel><bookmark href='file:///a' href='file:///b'/></xbel>",
        "<xbel><bookmark href='file:///a'visited='now'/></xbel>", "<xbel><bookmark href='file:///a&unknown;'/></xbel>",
        "<xbel><bookmark href='file:///a'/><info invalid></info></xbel>"] {
        assert!(xbel(text).is_none(), "{text}");
    }
    assert!(xbel(&format!("<xbel>{}{}</xbel>", "<info>".repeat(40), "</info>".repeat(40))).is_none());
}

#[test]
fn searchprior_fix1_invalid_element_names_drop_the_whole_provider() {
    for name in ["1bad", "-bad", ".bad", "a!bad"] {
        let xml = format!("<xbel><bookmark href='file:///work/z/report.rs'/><{name}/></xbel>");
        assert!(xbel(&xml).is_none(), "malformed element name {name} must drop the valid bookmark too");
    }
    assert!(xbel("<xbel xmlns:ns='urn:test'><_valid/><ns:valid-name.v1/></xbel>").is_some());
}

#[test]
fn searchprior_recency_orders_timezones_fractions_and_duplicate_paths_consistently() {
    assert_eq!(stamp("2026-10-18T01:30:00+01:30"), stamp("2026-10-18T00:00:00Z"));
    assert!(stamp("2026-10-18T00:00:00.1Z") > stamp("2026-10-18T00:00:00Z"));
    assert_eq!(stamp("2024-02-29T23:59:59Z").unwrap().0 + 1, stamp("2024-03-01T00:00:00Z").unwrap().0);
    for text in ["2025-02-29T00:00:00Z", "2026-10-18T99:00:00Z", "2026-10-18T00:00:00+99:00", "invalid"] { assert!(stamp(text).is_none()); }
    let paths = xbel("<xbel><bookmark href='file:///work/b' visited='2026-10-18T02:00:00+03:00'/><bookmark href='file:///work/a' visited='2026-10-18T00:00:00.1Z'/><bookmark href='file:///work/b' visited='2026-10-18T00:00:00Z'/></xbel>").unwrap();
    assert_eq!(paths[0], b"/work/a");
    let prior = Priors::new(paths, Vec::new());
    assert_eq!(prior.files.len(), 2);
    assert!(prior.files[b"/work/a".as_slice()] > prior.files[b"/work/b".as_slice()]);
}

#[test]
fn searchprior_file_reads_refuse_missing_symlink_fifo_directory_and_oversize() {
    let d = TestDir::new("searchprior-read");
    let good = d.file("recent.xbel", "<xbel><bookmark href='file:///work/file'/></xbel>");
    let before = std::fs::read(&good).unwrap();
    assert_eq!(read_xbel(&good), Some(vec![b"/work/file".to_vec()]));
    assert_eq!(std::fs::read(&good).unwrap(), before, "reading must never rewrite history");
    assert!(read_xbel(&d.join("missing")).is_none());
    assert!(read_xbel(d.path()).is_none());
    let link = d.join("link");
    std::os::unix::fs::symlink(&good, &link).unwrap();
    assert!(read_xbel(&link).is_none());
    let fifo = d.join("fifo");
    crate::backend::fifotest::mkfifo(&fifo);
    assert!(crate::backend::fifotest::within("prior fifo", move || read_xbel(&fifo)).is_none());
    let large = d.file("large", &" ".repeat(BYTES + 1));
    assert!(read_xbel(&large).is_none());
    assert!(read_xbel(&d.file("corrupt", "<xbel><bookmark href='file:///a'/>")).is_none());
}

#[test]
fn searchprior_zoxide_uses_read_only_argv_and_discards_failed_or_slow_output() {
    let d = TestDir::new("searchprior-zoxide");
    let good = d.script("good", "#!/bin/sh\n[ \"$*\" = 'query --list --all --score' ] || exit 9\nprintf '  2.0 /work/low\\n  8.0 /work/high with space\\n'\n");
    assert_eq!(zoxide(&good, Instant::now() + LIMIT).unwrap(), [b"/work/high with space".to_vec(), b"/work/low".to_vec()]);
    let bad = d.script("bad", "#!/bin/sh\nprintf '  80 /work/high\\n'\nexit 1\n");
    assert!(zoxide(&bad, Instant::now() + LIMIT).is_none());
    let slow = d.script("slow", "#!/bin/sh\nprintf '  80 /work/high\\n'\nexec sleep 3\n");
    let started = Instant::now();
    assert!(zoxide(&slow, started + Duration::from_millis(25)).is_none());
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(zoxide(&d.join("unavailable"), Instant::now() + LIMIT).is_none());
    let oversized = d.script("oversized", "#!/bin/sh\nhead -c 1048577 /dev/zero\n");
    assert!(zoxide(&oversized, Instant::now() + LIMIT).is_none());
    for text in [b"NaN /work/folder\n".as_slice(), b"inf /work/folder\n", b"-1 /work/folder\n", b"1 relative\n", b"1 /work/folder\ngarbage\n"] {
        assert!(zoxide_paths(text).is_none());
    }
    assert_eq!(zoxide_paths(b"1 /work/raw\xff\n").unwrap(), [b"/work/raw\xff".to_vec()]);
}

#[test]
fn searchprior_one_unavailable_provider_cannot_hide_the_other_or_extend_the_deadline() {
    let d = TestDir::new("searchprior-deadline");
    let history = d.file("recent.xbel", "<xbel><bookmark href='file:///work/file'/></xbel>");
    let slow = d.script("slow", "#!/bin/sh\nexec sleep 3\n");
    let started = Instant::now();
    let prior = load_from(Some(history), slow, Duration::from_millis(100));
    assert!(prior.files.contains_key(b"/work/file".as_slice()));
    assert!(prior.folders.values().all(|(direct, _)| *direct == 0));
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn searchprior_fresh_scores_are_identical_and_bonuses_preserve_all_lexical_advantages() {
    let mut index = Index::new();
    for path in ["render", "render_more", "a/rander", "z/render", "z/readme"] { index.push(path, false); }
    let fresh = hits(&index, "render");
    let mut unchanged = fresh.clone();
    Priors::default().apply(Path::new("/work"), Path::new("/work/z"), &index, &mut unchanged);
    assert_eq!(unchanged, fresh);
    let unrelated = Priors::new(vec![b"/work-other/z/render".to_vec()], Vec::new());
    unrelated.apply(Path::new("/work"), Path::new("/work/z"), &index, &mut unchanged);
    assert_eq!(unchanged, fresh, "external priors must not alter a fresh scope or match a partial path component");
    let priors = Priors::new(vec![b"/work/a/rander".to_vec(), b"/work/z/render".to_vec()], Vec::new());
    let mut assisted = fresh.clone();
    priors.apply(Path::new("/work"), Path::new("/work/a"), &index, &mut assisted);
    for (a, b) in fresh.iter().zip(&assisted) { assert_eq!(a.1, b.1); assert!((0..SCALE).contains(&(b.0 - a.0 * SCALE))); }
    for a in 0..fresh.len() { for b in 0..fresh.len() {
        if fresh[a].0 > fresh[b].0 { assert!(assisted[a].0 > assisted[b].0); }
    } }
    assert_eq!(ordered(&index, assisted)[0], "z/render");
}

#[test]
fn searchprior_closer_equal_matches_and_file_folder_context_rank_without_target_memorization() {
    let mut index = Index::new();
    for path in ["far/report.rs", "near/report.rs", "near/child/report.rs", "far/project", "near/project"] { index.push(path, path.ends_with("project")); }
    let prior = Priors::new(vec![b"/work/near/previously-opened.txt".to_vec()], Vec::new());
    let mut ranked = hits(&index, "report.rs");
    prior.apply(Path::new("/work"), Path::new("/work/near"), &index, &mut ranked);
    assert_eq!(ordered(&index, ranked), ["near/report.rs", "near/child/report.rs", "far/report.rs"]);
    let prior = Priors::new(Vec::new(), vec![b"/work/near/project".to_vec()]);
    let mut ranked = hits(&index, "project");
    prior.apply(Path::new("/work"), Path::new("/work/far"), &index, &mut ranked);
    assert_eq!(ordered(&index, ranked)[0], "near/project");
}

#[test]
fn searchprior_apply_builds_one_path_per_folder_that_holds_no_prior_file() {
    // 20 folders of 50 files and no prior file among them: a file's context and proximity are its folder's, so each folder builds one path.
    let mut index = Index::new();
    for row in 0..1000 { index.push(format!("d{:02}/report{:03}.rs", row / 50, row % 50), false); }
    let prior = Priors::new(vec![b"/work/other/recent.rs".to_vec()], vec![b"/work/d03".to_vec()]);
    let mut ranked = hits(&index, ".rs");
    assert_eq!(ranked.len(), 1000);
    index.reconstructed.store(0, std::sync::atomic::Ordering::Relaxed);
    prior.apply(Path::new("/work"), Path::new("/work/d07"), &index, &mut ranked);
    assert_eq!(index.reconstructed.load(std::sync::atomic::Ordering::Relaxed), 20, "one path per folder");
}

#[test]
fn searchprior_apply_scores_every_row_as_an_apply_of_that_row_alone() {
    // An apply of one row computes its whole bonus from its own path; the batch must reuse folder parts without changing any row.
    let mut index = Index::new();
    let folders = ["near", "near/deep", "near/deep/inner", "far", "far/x/y/z/w/v/u/t/s/r"];
    for path in ["top.rs", "near", "near/deep", "far", "far/x/y/z/w/v/u/t/s/r"] { index.push(path, !path.ends_with(".rs")); }
    for row in 0..400 { index.push(format!("{}/f{row:03}.rs", folders[row % folders.len()]), false); }
    let files = vec![b"/work/near/f005.rs".to_vec(), b"/work/far/x/y/old.rs".to_vec(), b"/work/top.rs".to_vec(), b"/work/near/deep/f011.rs".to_vec()];
    let prior = Priors::new(files, vec![b"/work/near/deep".to_vec(), b"/work/far".to_vec()]);
    for folder in ["/work/near/deep", "/work/far/x/y/z", "/work", "/elsewhere"] {
        let mut batch: Vec<(i32, usize)> = (0..index.len()).map(|row| (3, row)).collect();
        prior.apply(Path::new("/work"), Path::new(folder), &index, &mut batch);
        let alone: Vec<(i32, usize)> = (0..index.len()).map(|row| {
            let mut one = [(3, row)];
            prior.apply(Path::new("/work"), Path::new(folder), &index, &mut one);
            one[0]
        }).collect();
        assert_eq!(batch, alone, "folder {folder}");
        let bonuses: std::collections::BTreeSet<i32> = batch.iter().map(|hit| hit.0 - 3 * SCALE).collect();
        assert!(bonuses.len() >= 4, "the fixture gives distinct bonuses at {folder}: {bonuses:?}");
    }
}

#[test]
fn searchprior_caps_input_rows_retained_keys_and_total_path_bytes() {
    let xml = format!("<xbel>{}</xbel>", (0..5000).map(|i| format!("<bookmark href='file:///work/{i}'/>")).collect::<String>());
    assert_eq!(xbel(&xml).unwrap().len(), RECORDS);
    let dirs = (0..2000).map(|i| format!("{i} /work/{i}\n")).collect::<String>();
    assert_eq!(zoxide_paths(dirs.as_bytes()).unwrap().len(), 1000);
    let paths = (0..6000).map(|i| format!("/work/{i:04}{}", "x".repeat(250)).into_bytes()).collect();
    let prior = Priors::new(paths, Vec::new());
    assert!(prior.files.len() + prior.folders.len() <= RECORDS);
    assert!(prior.bytes <= BYTES);
    assert_eq!(prior.bytes, prior.files.keys().chain(prior.folders.keys()).map(Vec::len).sum::<usize>());
}

#[test]
#[ignore]
fn searchprior_scale_probe() {
    let d = TestDir::new("searchprior-probe");
    for count in [100_000, 1_000_000] {
        let mut index = Index::new();
        for row in 0..count { index.push(format!("d{:04}/report{:03}.rs", row / 100, row % 100), false); }
        let file = d.file("recent.xbel", &format!("<xbel>{}</xbel>", (0..1000).map(|i| format!("<bookmark href='file:///work/d{i:04}/report099.rs' visited='2026-10-{}T00:00:00Z'/>", if i == 999 { 18 } else { 17 })).collect::<String>()));
        let provider = d.script("zoxide", &format!("#!/bin/sh\nprintf '{}'\n", (0..1000).map(|i| format!("{} /work/d{i:04}\\n", 1000 - i)).collect::<String>()));
        let started = Instant::now();
        let prior = load_from(Some(file), provider, LIMIT);
        let load_us = started.elapsed().as_secs_f64() * 1e6;
        let mut ranked = hits(&index, ".rs");
        assert_eq!(ranked.len(), count);
        let started = Instant::now();
        prior.apply(Path::new("/work"), Path::new("/work/d0999"), &index, &mut ranked);
        let apply_us = started.elapsed().as_secs_f64() * 1e6;
        let fresh = hits(&index, ".rs");
        let target = 99_999;
        assert_eq!(index.raw(target), b"d0999/report099.rs");
        let base_target = fresh.iter().find(|(_, row)| *row == target).unwrap();
        let prior_target = ranked.iter().find(|(_, row)| *row == target).unwrap();
        let before = fresh.iter().filter(|hit| rank_cmp(&index, hit, base_target).is_lt()).count() + 1;
        let after = ranked.iter().filter(|hit| rank_cmp(&index, hit, prior_target).is_lt()).count() + 1;
        assert_eq!(after, 1);
        println!("SEARCHPRIOR_PROBE entries={count} load_us={load_us:.3} apply_us={apply_us:.3} retained_keys={} path_bytes={} target_rank_before={before} target_rank_after={after}", prior.files.len() + prior.folders.len(), prior.bytes);
    }
}
