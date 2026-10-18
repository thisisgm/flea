#[path = "../src/perf/memory_probe.rs"]
pub mod memory_probe;

use memory_probe::{account, parse_rollup, parse_smaps, total, Memory};

fn record(header: &str, rss: u64, pss: u64, clean: u64, dirty: u64) -> String {
    format!("{header}\nRss: {rss} kB\nPss: {pss} kB\nPrivate_Clean: {clean} kB\nPrivate_Dirty: {dirty} kB\nPrivate_Hugetlb: 0 kB\nVmFlags: rd wr\n")
}

#[test]
fn private_text_data_heap_fonts_driver_and_shared_pages_reconcile() {
    let cases = [
        (
            "1000-2000 r-xp 0000 08:01 1 /opt/bin/flea-gui",
            16,
            12,
            4,
            0,
        ),
        ("2000-3000 rw-p 1000 08:01 1 /opt/bin/flea-gui", 8, 8, 0, 8),
        ("3000-4000 rw-p 0000 00:00 0 [heap]", 20, 20, 0, 20),
        ("4000-5000 rw-p 0000 00:00 0", 12, 12, 0, 12),
        (
            "5000-6000 r--p 0000 08:01 2 /opt/fonts/Body Regular.ttf",
            16,
            8,
            0,
            0,
        ),
        (
            "6000-7000 rw-s 0000 00:05 3 /dev/dri/renderD128",
            24,
            12,
            0,
            0,
        ),
        (
            "7000-8000 rw-s 0000 00:01 4 /memfd:glyph atlas (deleted)",
            4,
            4,
            0,
            4,
        ),
    ];
    let text: String = cases
        .iter()
        .map(|(h, r, p, c, d)| record(h, *r, *p, *c, *d))
        .collect();
    let mappings = parse_smaps(&text).unwrap();
    let classes = account(&mappings, "/opt/bin/flea-gui").unwrap();
    assert_eq!(classes.len(), cases.len());
    assert!(classes.contains_key("font:/opt/fonts/Body Regular.ttf"));
    assert!(classes.contains_key("memfd:/memfd:glyph atlas"));
    assert_eq!(classes["executable:r-xp"].private_clean_kib, 4);
    let sum = total(&classes).unwrap();
    assert_eq!((sum.rss_kib, sum.pss_kib, sum.uss_kib()), (100, 76, 48));
    let rollup = parse_rollup(&record(
        "1000-8000 ---p 0000 00:00 0 [rollup]",
        100,
        76,
        4,
        44,
    ))
    .unwrap();
    assert_eq!(sum, rollup);
}

#[test]
fn missing_duplicate_malformed_and_overflowed_counters_fail_closed() {
    let valid = record("1000-2000 rw-p 0000 00:00 0 [heap]", 8, 8, 0, 8);
    for invalid in [
        String::new(),
        valid.replace("Pss: 8 kB\n", ""),
        format!("{valid}Pss: 8 kB\n"),
        valid.replace("Rss: 8 kB", "Rss: 8 MB"),
        valid.replace("Pss: 8 kB", "Pss: -1 kB"),
        valid.replace("1000-2000", "2000-1000"),
        valid.replace("1000-2000", "1000-INVALID"),
        valid.replace("Private_Clean: 0", &format!("Private_Clean: {}", u64::MAX)),
    ] {
        assert!(parse_smaps(&invalid).is_err(), "accepted {invalid:?}");
    }
    assert!(parse_rollup(&valid).is_err());
    let memory = Memory {
        pss_kib: u64::MAX,
        ..Memory::default()
    };
    assert!(memory
        .checked_add(Memory {
            pss_kib: 1,
            ..Memory::default()
        })
        .is_err());
}

#[test]
fn deleted_executable_and_named_anonymous_memory_are_not_lost() {
    let text = record(
        "1000-2000 r--p 0000 08:01 1 /opt/bin/app (deleted)",
        8,
        8,
        8,
        0,
    ) + &record("2000-3000 rw-p 0000 00:00 0 [anon:arena 1]", 4, 4, 0, 4)
        + &record("3000-4000 rw-p 0000 00:00 0 [stack]", 4, 4, 0, 4);
    let classes = account(&parse_smaps(&text).unwrap(), "/opt/bin/app (deleted)").unwrap();
    assert_eq!(classes["executable:r--p"].uss_kib(), 8);
    assert_eq!(classes["anonymous:[anon:arena 1]"].uss_kib(), 4);
    assert_eq!(classes["stack"].uss_kib(), 4);
    assert_eq!(total(&classes).unwrap().uss_kib(), 16);
}

#[test]
fn private_hugetlb_stays_separate_from_conventional_uss_and_pss() {
    let text = record("1000-2000 rw-p 0000 00:00 0 [heap]", 8, 8, 0, 8)
        .replace("Private_Hugetlb: 0", "Private_Hugetlb: 2048");
    let classes = account(&parse_smaps(&text).unwrap(), "").unwrap();
    let sum = total(&classes).unwrap();
    assert_eq!(
        (sum.uss_kib(), sum.pss_kib, sum.private_hugetlb_kib),
        (8, 8, 2048)
    );
}

#[test]
#[cfg(target_os = "linux")]
fn real_proc_maps_are_accounted_without_claiming_a_gui_measurement() {
    let smaps = std::fs::read_to_string("/proc/self/smaps").unwrap();
    let rollup = std::fs::read_to_string("/proc/self/smaps_rollup").unwrap();
    let exe = std::fs::read_link("/proc/self/exe").unwrap();
    let mappings = parse_smaps(&smaps).unwrap();
    let classes = account(&mappings, exe.to_str().unwrap()).unwrap();
    let sum = total(&classes).unwrap();
    assert!(sum.rss_kib > 0 && sum.pss_kib > 0 && sum.uss_kib() > 0);
    assert!(parse_rollup(&rollup).unwrap().rss_kib > 0);
    assert!(classes.keys().any(|key| key.starts_with("executable:")));
}
