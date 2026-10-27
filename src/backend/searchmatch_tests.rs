use super::*;
use crate::backend::searchtypo;

fn chars(bytes: &[u8]) -> Vec<char> {
    String::from_utf8_lossy(bytes).chars().collect()
}

fn boundary(hay: &[char], i: usize) -> bool {
    i == 0 || separator(hay[i - 1]) || (hay[i].is_uppercase() && !hay[i - 1].is_uppercase())
}

// Exhaustive predecessor alignment is independent of the streaming maximum used in production.
fn alignment(query: &[char], hay: &[char], base: usize) -> Option<i32> {
    if query.is_empty() { return Some(0); }
    let mut table = vec![vec![None; hay.len()]; query.len()];
    for k in 0..query.len() {
        for at in 0..hay.len() {
            if query[k] != hay[at].to_lowercase().next().unwrap() { continue; }
            let bonus = i32::from(boundary(hay, at)) * 6 + i32::from(at >= base) * 4;
            table[k][at] = if k == 0 { Some(bonus) } else {
                (0..at).filter_map(|previous| table[k - 1][previous].map(|score| {
                    score + bonus + if previous + 1 == at { 8 } else { -((at - previous - 1) as i32) }
                })).max()
            };
        }
    }
    table.last().unwrap().iter().flatten().copied().max()
}

fn distance(query: &[char], hay: &[char], budget: usize) -> Option<usize> {
    let mut best = budget + 1;
    for start in 0..hay.len() {
        if !boundary(hay, start) { continue; }
        let tail: Vec<_> = hay[start..].iter().map(|c| c.to_lowercase().next().unwrap()).collect();
        let mut dp = vec![vec![1000; tail.len() + 1]; query.len() + 1];
        dp[0][0] = 0;
        for i in 0..=query.len() {
            for j in 0..=tail.len() {
                if i > 0 && query[i - 1].is_alphabetic() { dp[i][j] = dp[i][j].min(dp[i - 1][j] + 1); }
                if j > 0 && tail[j - 1].is_alphabetic() { dp[i][j] = dp[i][j].min(dp[i][j - 1] + 1); }
                if i > 0 && j > 0 {
                    let (a, b) = (query[i - 1], tail[j - 1]);
                    if a == b { dp[i][j] = dp[i][j].min(dp[i - 1][j - 1]); }
                    else if a.is_alphabetic() && b.is_alphabetic() { dp[i][j] = dp[i][j].min(dp[i - 1][j - 1] + 1); }
                    if i > 1 && j > 1 && a == tail[j - 2] && query[i - 2] == b && a.is_alphabetic() && b.is_alphabetic() {
                        dp[i][j] = dp[i][j].min(dp[i - 2][j - 2] + 1);
                    }
                }
            }
        }
        best = best.min(*dp.last().unwrap().iter().min().unwrap());
    }
    (budget > 0 && best <= budget).then_some(best)
}

pub(crate) fn reference(query: &str, path: &[u8]) -> Option<i32> {
    let query_chars: Vec<_> = query.chars().map(|c| c.to_lowercase().next().unwrap()).collect();
    if query_chars.is_empty() { return Some(0); }
    let hay = chars(path);
    let base = hay.iter().rposition(|&c| c == '/').map_or(0, |at| at + 1);
    let leaf = &hay[base..];
    let prefix = |at| leaf.get(at..at + query_chars.len()).is_some_and(|text| {
        text.iter().zip(&query_chars).all(|(a, b)| a.to_lowercase().next() == Some(*b))
    });
    if prefix(0) { return Some(if leaf.len() == query_chars.len() { EXACT } else { PREFIX }); }
    if (1..leaf.len()).any(|at| boundary(leaf, at) && prefix(at)) { return Some(WORD); }
    if let Some(score) = alignment(&query_chars, leaf, 0) { return Some(NAME + score.clamp(-100_000, 100_000)); }
    let matched = alignment(&query_chars, &hay, base).map(|score| {
        (if query_chars.contains(&'/') { ALIGNED } else { PATH }) + score.clamp(-100_000, 100_000)
    });
    let budget = Matcher::new(query).budget;
    let mut score = matched.max(distance(&query_chars, leaf, budget).map(|edits| TYPO - edits as i32 * 1000));
    if query_chars.contains(&'/') {
        score = score.max(distance(&query_chars, &hay, budget).map(|edits| TYPO - edits as i32 * 1000));
    }
    score
}

fn words(alphabet: &[u8], max: usize) -> Vec<Vec<u8>> {
    let mut out = vec![Vec::new()];
    let mut head = 0;
    for _ in 0..max {
        let end = out.len();
        for i in head..end {
            for &letter in alphabet {
                let mut word = out[i].clone();
                word.push(letter);
                out.push(word);
            }
        }
        head = end;
    }
    out
}

#[test]
fn search_matcher_matches_exhaustive_alignment_and_full_edit_matrix() {
    let names = words(b"ab1_", 4);
    for query in ["", "a", "ab", "ba", "aba", "aaaa", "abba", "a1ba", ".ab", "a_ba"] {
        let matcher = Matcher::new(query);
        let mut scratch = searchtypo::Scratch::default();
        for name in &names {
            assert_eq!(matcher.full(name), reference(query, name), "{query:?} {name:?}");
            let letters = chars(name);
            let mut folded = Vec::new();
            fold(name, &mut folded);
            let expected = distance(&matcher.needle, &letters, matcher.budget);
            assert_eq!(scratch.distance(&matcher.needle, &folded, matcher.budget), expected);
            assert_eq!(matcher.distance(&folded, &mut scratch), expected);
        }
    }
}

#[test]
fn search_matches_long_names_unicode_and_literal_punctuation() {
    for (query, name) in [
        ("ab", format!("{}ab", "ax".repeat(100))), ("café", "CAFÉ.txt".to_string()),
        ("i", "İstanbul".to_string()), (".svg", "icon.sxg".to_string()),
        ("render12", "render13".to_string()), ("render12", "render1".to_string()),
    ] {
        assert_eq!(Matcher::new(query).full(name.as_bytes()), reference(query, name.as_bytes()), "{query} {name}");
    }
    assert!(Matcher::new(".svg").full(b"icon.sxg").is_none());
    assert!(Matcher::new("render12").full(b"render13").is_none());
}

#[test]
fn search_edit_budgets_and_narrowing_are_conservative() {
    for (query, edits) in [("abc", 0), ("abcd", 1), ("abcdefg", 1), ("abcdefgh", 2), (".abcdefgh", 0)] {
        assert_eq!(Matcher::new(query).budget, edits);
    }
    assert!(Matcher::can_narrow("ab", "abc"));
    assert!(Matcher::can_narrow("abcd", "abcde"));
    for (before, now) in [("abc", "abcd"), ("abcdefg", "abcdefgh"), ("abcd", "abcd/"), ("ab", "a")] {
        assert!(!Matcher::can_narrow(before, now));
    }
}

#[test]
fn search_bit_edits_match_the_reference_at_both_budgets_and_word_boundaries() {
    let mut seed = 2u64;
    let mut next = |alphabet: &[u8]| {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        alphabet[(seed >> 32) as usize % alphabet.len()] as char
    };
    let mut folded = Vec::new();
    let mut scratch = searchtypo::Scratch::default();
    for step in 0..20_000 {
        let query: String = (0..4 + step % 7).map(|_| next(b"aab1")).collect();
        let hay: String = (0..1 + step * 5 % 14).map(|_| next(b"ab1_")).collect();
        let matcher = Matcher::new(&query);
        fold(hay.as_bytes(), &mut folded);
        assert_eq!(matcher.full(hay.as_bytes()), reference(&query, hay.as_bytes()), "{query} {hay}");
        assert_eq!(matcher.distance(&folded, &mut scratch), distance(&matcher.needle, &chars(hay.as_bytes()), matcher.budget), "{query} {hay}");
    }
    for len in [8, 63, 64, 128] {
        let query = format!("{}bc", "a".repeat(len - 2));
        for hay in [query.clone(), format!("{}cb", "a".repeat(len - 2)), format!("x{}bc", "a".repeat(len - 3))] {
            let matcher = Matcher::new(&query);
            fold(hay.as_bytes(), &mut folded);
            assert_eq!(matcher.distance(&folded, &mut scratch), distance(&matcher.needle, &chars(hay.as_bytes()), matcher.budget));
        }
    }
    let matcher = Matcher::new("café");
    fold("cafè".as_bytes(), &mut folded);
    assert_eq!(matcher.distance(&folded, &mut scratch), Some(1));
}
