//! crates/flea-gui/startup-order.txt feeds the release link script build.rs writes, one section pattern a line.
use std::collections::HashSet;

// Sample crate root: "Cs9k3SxhrAWiO_3std", which a new toolchain or dependency version renames.
fn names_a_crate_hash(name: &str) -> bool {
    let bytes = name.as_bytes();
    (0..bytes.len().saturating_sub(2)).any(|at| {
        let run = bytes[at + 2..]
            .iter()
            .take_while(|b| b.is_ascii_alphanumeric())
            .count();
        bytes[at..].starts_with(b"Cs")
            && run >= 9
            && bytes.get(at + 2 + run) == Some(&b'_')
            && bytes.get(at + 3 + run).is_some_and(u8::is_ascii_digit)
    })
}

#[test]
fn startup_order_lines_are_hash_free_section_patterns() {
    let text =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/startup-order.txt")).unwrap();
    let names: Vec<&str> = text
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    // Far fewer would be a trace that missed the launch, and the layout would cover little of it.
    assert!(names.len() >= 1000, "{} names", names.len());
    let mut seen = HashSet::new();
    for name in names {
        // A space, parenthesis or bracket would end or change the pattern in the linker script.
        assert!(
            name.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.*".contains(&b)),
            "{name}"
        );
        assert!(
            !names_a_crate_hash(name),
            "{name} keeps a crate hash, so the next toolchain would miss it"
        );
        assert!(seen.insert(name), "{name} is listed twice");
    }
}

#[test]
fn a_crate_hash_is_told_from_a_name_that_merely_starts_with_cs() {
    assert!(names_a_crate_hash("_RNvCs9k3SxhrAWiO_3std4main"));
    assert!(!names_a_crate_hash("_RNvCs*_3std4main"));
    assert!(!names_a_crate_hash("_RNvNtCs*_4flea9CsvReader_4read"));
}

#[test]
fn startup_order_recorder_cleans_only_its_own_trace_directory() {
    let output = std::process::Command::new("bash")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/startup-order-tool.sh"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
