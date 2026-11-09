// tests/js/match.js's split and run cases, plus the multi-byte and printable rules.
use super::*;

#[test]
fn a_result_path_splits_into_its_name_and_folder() {
    assert_eq!(location("BENCHMARK.md"), "");
    assert_eq!(base("BENCHMARK.md"), "BENCHMARK.md");
    assert_eq!(location("tools/benches/bench-run.sh"), "tools/benches");
    assert_eq!(base("tools/benches/bench-run.sh"), "bench-run.sh");
    assert_eq!(base("tools/benches"), "benches");
}

#[test]
fn the_typed_run_folds_case_and_is_exactly_the_query() {
    assert_eq!(run("bench.txt", ""), None);
    assert_eq!(run("other.txt", "bench"), None);
    assert_eq!(run("bench.txt", "bench"), Some((0, 5)));
    assert_eq!(run("BENCHMARK.md", "bench"), Some((0, 5)));
    assert_eq!(run("field-bench-notes.md", "bench"), Some((6, 11)));
    // Byte ranges stay on character boundaries for a multi-byte name.
    let name = "Ärger-NOTES.txt";
    let (start, end) = run(name, "notes").unwrap();
    assert_eq!(&name[start..end], "NOTES");
    assert_eq!(run("ÄRGER", "är"), Some((0, 3)));
    assert_eq!(run("ab", "abc"), None, "a query longer than the name never runs off its end");
}

#[test]
fn only_one_printable_character_types() {
    assert!(is_printable("e"));
    assert!(is_printable("é"));
    assert!(is_printable(" "));
    assert!(!is_printable("\u{7f}"));
    assert!(!is_printable("\t"));
    assert!(!is_printable(""));
    assert!(!is_printable("ab"));
}
