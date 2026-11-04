//! The ranked rows narrow answers over the 100,000-name fixture and the every-row-close folders, held to FIX2's answers.
use super::tests_fixture::{realistic_keys, realistic_names};
use super::*;

// FNV-1a over the exact rows, a separator, then the close rows in rank order.
fn digest(found: &Narrowed) -> u64 {
    let rows = found.exact.iter().chain([&usize::MAX]).chain(&found.close);
    rows.fold(0xcbf2_9ce4_8422_2325, |hash, &row| (hash ^ row as u64).wrapping_mul(0x0100_0000_01b3))
}

fn typing(word: &str) -> Vec<String> {
    (1..=word.len()).map(|end| word[..end].to_string()).collect()
}

fn answers(label: &str, names: &[String], keys: &[String], out: &mut Vec<String>) {
    for key in keys {
        let found = narrow(key, names.iter().enumerate().map(|(row, name)| (row, name.as_str()))).unwrap();
        out.push(format!("{label}\t{key:?}\t{}\t{}\t{:016x}", found.exact.len(), found.close.len(), digest(&found)));
    }
}

#[test]
fn narrow_answers_the_fixture_as_fix2_did() {
    let mut got = Vec::new();
    answers("realistic", &realistic_names(), &realistic_keys(), &mut got);
    let items: Vec<String> = (0..20_000).map(|row| format!("item-{row:06}.{}", if row % 2 == 0 { "png" } else { "txt" })).collect();
    answers("item", &items, &typing("itme"), &mut got);
    let fox: Vec<String> = (0..5_000).map(|row| format!("the quick brown fox jumps over the lazy dog {row:05}.txt")).collect();
    let fox_keys = ["teh quick", "teh quick brown fox", "teh quick brown fox jumps over the lazy dog"].map(String::from);
    answers("fox", &fox, &fox_keys, &mut got);
    let imgs: Vec<String> = (0..20_000).map(|row| format!("IMG_20261018_{:02}{:02}{:02}.jpg", (row / 3600) % 24, (row / 60) % 60, row % 60)).collect();
    answers("img", &imgs, &typing("ing_2026"), &mut got);
    let want: Vec<&str> = include_str!("tests_same.tsv").lines().collect();
    for line in &got {
        println!("SAME\t{line}");
    }
    assert_eq!(got.len(), want.len(), "one recorded answer per key");
    for (got, want) in got.iter().zip(want) {
        assert_eq!(got, want, "the ranked rows match FIX2's");
    }
}
