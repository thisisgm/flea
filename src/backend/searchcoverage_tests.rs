use super::*;

#[test]
fn search_index_keeps_one_million_raw_identities() {
    let mut index = Index::new();
    for i in 0..1_000_000 {
        index.push(format!("d{:03}/f{:06}.txt", i / 1000, i), false);
    }
    assert_eq!(index.len(), 1_000_000);
    for i in 0..1_000_000 {
        assert_eq!(index.raw(i), format!("d{:03}/f{:06}.txt", i / 1000, i).as_bytes());
    }
}

#[test]
fn search_compact_storage_counts_lookup_and_stays_below_ninety_bytes_per_entry() {
    let mut index = Index::new();
    for i in 0..100_000 {
        index.push(format!("some_directory_name_{:03}/nested_subdir/report_final_draft_v{:05}_reviewed.txt", i % 200, i), false);
    }
    assert!(index_bytes(&index) <= 90 * index.len(), "{} bytes for {} entries", index_bytes(&index), index.len());
}

#[test]
fn search_raw_names_never_collide_with_lossy_or_unicode_names() {
    let paths: &[&[u8]] = &[b"d\xff/caf\xe8", b"d\xfe/caf\xe9", "d\u{fffd}/caf\u{fffd}".as_bytes(), "\u{0130}/CAF\u{00c9}".as_bytes()];
    let mut index = Index::new();
    for path in paths { assert!(index.push(OsStr::from_bytes(path), false)); }
    for (row, path) in paths.iter().enumerate() {
        assert_eq!(&index.raw(row), path);
        assert_eq!(index.path_len(row), String::from_utf8_lossy(path).len());
    }
    let fuzzy = crate::backend::searchmatch::Matcher::new("caf");
    let hits = score_all(&fuzzy, &(0..paths.len()).collect::<Vec<_>>(), &index);
    let direct = crate::backend::searchmatch::Matcher::new("caf");
    for (score, row) in hits {
        assert_eq!(Some(score), direct.full(paths[row]));
    }
}

#[test]
fn search_interns_repeated_basenames_and_survives_slab_boundaries() {
    let mut index = Index::new();
    for i in 0..1000 { index.push(format!("d{i}/same.txt"), false); }
    assert_eq!(index.names.len(), 1001);
    for i in 0..1000 { assert_eq!(index.raw(i), format!("d{i}/same.txt").as_bytes()); }
    for i in 0..1000 { index.push(format!("{i:04}{}", "x".repeat(240)), false); }
    index.finish();
    assert!(index.push("after-finish", false));
    assert_eq!(index.raw(2000), b"after-finish");
    assert_eq!(index.partial, None);
    assert!(index.bytes(true) >= index_bytes(&index));
    assert!(!index.push("x".repeat(super::super::searchnames::SLAB + 1), false));
    assert_eq!(index.len(), 2001);
    assert_eq!(index.partial, Some("capacity"));
}

#[test]
fn search_refused_walk_and_cancel_are_explicitly_partial() {
    use crate::backend::searchwalk::{walk, WalkProbes};
    use crate::backend::testdir::TestDir;
    let d = TestDir::new("searchpartial");
    let (index, _) = walk(&d.join("missing"), false, false, "", &WalkProbes::new());
    assert_eq!(index.partial, Some("unreadable"));
    let probes = WalkProbes::new();
    probes.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    let (index, _) = walk(d.path(), false, false, "", &probes);
    assert_eq!(index.partial, Some("cancelled"));
    let line = crate::backend::searchreq::searched_status_line(0, 0, 0.0, 4, false, index.partial);
    assert!(crate::json::field_bool(&line, "partial"));
    assert_eq!(crate::json::field_str(&line, "partial_reason").as_deref(), Some("cancelled"));
}

fn pss_bytes() -> usize {
    std::fs::read_to_string("/proc/self/smaps_rollup").unwrap().lines()
        .find_map(|line| line.strip_prefix("Pss:").map(|text| text.split_whitespace().next().unwrap().parse::<usize>().unwrap() * 1024)).unwrap()
}

#[test]
#[ignore]
fn search_compact_scale_probe() {
    let Ok(count) = std::env::var("FLEA_SCALE_COUNT") else {
        for count in [100_000, 1_000_000] {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["search_compact_scale_probe", "--ignored", "--nocapture", "--test-threads=1"])
                .env("FLEA_SCALE_COUNT", count.to_string()).status().unwrap();
            assert!(status.success());
        }
        return;
    };
    let count: usize = count.parse().unwrap();
    {
        let before = pss_bytes();
        let started = std::time::Instant::now();
        let (index, _) = super::fixture_tests::synthetic(count, false);
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        let pss = pss_bytes();
        assert_eq!(index.len(), count);
        assert_eq!(index.partial, None);
        assert!(index_bytes(&index) <= 90 * count);
        eprintln!("SEARCH_SCALE n={count} build_ms={ms:.3} logical={} allocated={} pss_before={before} pss_after={pss} pss_delta={} logical_bpe={:.3} pss_delta_bpe={:.3}",
            index_bytes(&index), index.bytes(true), pss.saturating_sub(before), index_bytes(&index) as f64 / count as f64, pss.saturating_sub(before) as f64 / count as f64);
        std::hint::black_box(&index);
    }
}

#[test]
fn search_walk_preserves_hidden_policy_and_never_descends_directory_links() {
    use crate::backend::searchwalk::{walk, WalkProbes};
    use crate::backend::testdir::TestDir;
    let d = TestDir::new("searchpolicy");
    d.dir("tree/sub");
    d.file("tree/sub/name", "");
    d.dir("tree/.hidden");
    d.file("tree/.hidden/inside", "");
    d.file("tree/.dotfile", "");
    d.dir("outside");
    d.file("outside/never-indexed", "");
    std::os::unix::fs::symlink(d.join("outside"), d.join("tree/to-outside")).unwrap();
    std::os::unix::fs::symlink(d.join("tree"), d.join("tree/sub/back")).unwrap();
    for (hidden, expected, dirs) in [(false, 4, 2), (true, 7, 3)] {
        let (index, walked) = walk(&d.join("tree"), hidden, false, "", &WalkProbes::new());
        assert_eq!(index.len(), expected);
        assert_eq!(walked, dirs);
        assert_eq!(index.partial, None);
        for row in 0..index.len() {
            let path = index.path(row);
            assert!(!path.contains("never-indexed"));
            if path.ends_with("back") || path == "to-outside" { assert!(!index.is_dir(row)); }
            if !hidden { assert!(!path.starts_with('.')); }
        }
    }
}
