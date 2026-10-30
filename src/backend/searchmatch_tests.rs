use super::*;
use crate::backend::searchtypo;

// Multiplier and increment of the tests' linear congruential generator (Knuth's MMIX constants).
const LCG_MULTIPLIER: u64 = 6364136223846793005;
const LCG_INCREMENT: u64 = 1442695040888963407;

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

// Layer 1 holds alignments that dropped a query letter; those may only end where the matched word ends.
fn distance(query: &[char], hay: &[char], budget: usize) -> Option<usize> {
    let (mut best, mut plain) = (budget + 1, budget + 1);
    for start in 0..hay.len() {
        if !boundary(hay, start) { continue; }
        let tail: Vec<_> = hay[start..].iter().map(|c| c.to_lowercase().next().unwrap()).collect();
        let mut dp = vec![vec![vec![1000; tail.len() + 1]; query.len() + 1]; 2];
        dp[0][0][0] = 0;
        for i in 0..=query.len() {
            for j in 0..=tail.len() {
                for layer in 0..2 {
                    if i > 0 && query[i - 1].is_alphabetic() { dp[1][i][j] = dp[1][i][j].min(dp[layer][i - 1][j] + 1); }
                    if j > 0 && tail[j - 1].is_alphabetic() { dp[layer][i][j] = dp[layer][i][j].min(dp[layer][i][j - 1] + 1); }
                    if i > 0 && j > 0 {
                        let (a, b) = (query[i - 1], tail[j - 1]);
                        if a == b { dp[layer][i][j] = dp[layer][i][j].min(dp[layer][i - 1][j - 1]); }
                        else if a.is_alphabetic() && b.is_alphabetic() { dp[layer][i][j] = dp[layer][i][j].min(dp[layer][i - 1][j - 1] + 1); }
                        if i > 1 && j > 1 && a == tail[j - 2] && query[i - 2] == b && a.is_alphabetic() && b.is_alphabetic() {
                            dp[layer][i][j] = dp[layer][i][j].min(dp[layer][i - 2][j - 2] + 1);
                        }
                    }
                }
            }
        }
        let word_end = |j: usize| start + j == hay.len() || !hay[start + j].is_alphabetic() || boundary(hay, start + j);
        for (j, (&kept, &dropped)) in dp[0][query.len()].iter().zip(&dp[1][query.len()]).enumerate() {
            best = best.min(kept);
            plain = plain.min(kept).min(dropped);
            if word_end(j) { best = best.min(dropped); }
        }
    }
    // A match only a mid-word dropped letter gives is a shadow: narrowing keeps it, answers hide it.
    if budget == 0 { None } else if best <= budget { Some(best) } else { (plain <= budget).then_some(searchtypo::SHADOW + plain) }
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
    // docs/protocol.md: an ordinary subsequence is one class, scored by its best alignment across the whole relative path.
    let matched = alignment(&query_chars, &hay, base).map(|score| {
        (if query_chars.contains(&'/') { ALIGNED } else { NAME }) + score.clamp(-100_000, 100_000)
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
fn search_a_typed_folder_run_competes_with_name_matches_on_the_whole_path() {
    let folder = b"projects/editor/src/components/render_frame.rs".as_slice();
    let scattered = b"Music/folder_01/test_test_00074578_detail_detail_detail.json".as_slice();
    let contiguous = b"Music/folder_01/credit_notes.txt".as_slice();
    let word = b"Music/folder_01/notes_edit.txt".as_slice();
    let edit = Matcher::new("edit");
    for path in [folder, scattered, contiguous, word, b"projects/editor".as_slice(), b"editor/edit_x/tide/e_d_i_t".as_slice()] {
        assert_eq!(edit.full(path), reference("edit", path), "{}", String::from_utf8_lossy(path));
    }
    let score = |path: &[u8]| edit.full(path).expect("a subsequence of the path");
    assert!(score(folder) > score(scattered), "a run at a folder's start beats letters scattered through a name");
    assert!(score(contiguous) > score(folder), "a contiguous run inside the name still beats the folder run");
    assert!(score(word) > score(contiguous) && score(b"projects/editor") > score(folder), "word and prefix classes stay above");
    // A name that is itself a weak subsequence never ranks below a sibling whose name does not match at all.
    assert!(score(b"projects/editor/src/exdxixtx.rs") >= score(folder));
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
        seed = seed.wrapping_mul(LCG_MULTIPLIER).wrapping_add(1);
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

#[test]
fn search_fp1_a_typo_never_lists_a_word_two_edits_away() {
    // FP-1: "flac" is two edits from "flea"; dropping its "e" against the prefix "fla" only leaves a shadow narrowing keeps.
    for query in ["Flea", "flea"] {
        let matcher = Matcher::new(query);
        for path in [&b"audio-flac.flac"[..], b"music/song.flac", b"flags.toml", b"flame.png"] {
            assert_eq!(matcher.full(path), Some(SHADOW_TYPO - TYPO_EDIT), "{query} {}", String::from_utf8_lossy(path));
            assert_eq!(reference(query, path), Some(SHADOW_TYPO - TYPO_EDIT), "{query} {}", String::from_utf8_lossy(path));
        }
        assert!(matcher.full(b"notes/flea.txt").is_some_and(|score| score > TYPO));
    }
}

// The lane DP skips bytes that match no query position; every cell and the hit must equal fold then advance, from any carried state.
#[test]
fn search_lane_advance_matches_fold_advance_cell_for_cell() {
    let alphabet = b"abcdeEDA/_-. xyzQ0";
    let mut seed = 7u64;
    let mut next = |bound: usize| {
        seed = seed.wrapping_mul(LCG_MULTIPLIER).wrapping_add(1);
        (seed >> 33) as usize % bound
    };
    for _ in 0..20_000 {
        let query: String = (0..1 + next(6)).map(|_| alphabet[next(alphabet.len())] as char).collect();
        let matcher = Matcher::new(&query);
        let len = matcher.needle.len();
        let parent: Vec<u8> = (0..next(9)).map(|_| alphabet[next(alphabet.len())]).collect();
        let name: Vec<u8> = (0..next(14)).map(|_| alphabet[next(alphabet.len())]).collect();
        let mut letters = Vec::new();
        let mut start = State::new(len);
        fold(&parent, &mut letters);
        start.advance(&matcher.needle, &letters, next(2) == 0);
        for basename in [false, true] {
            let mut want = start.clone();
            fold(&name, &mut letters);
            want.advance(&matcher.needle, &letters, basename);
            let mut got = start.clone();
            matcher.advance(&mut got, &name, &mut Vec::new(), basename);
            let show = |s: &State| (s.best.clone(), s.end.clone(), s.hit);
            assert_eq!(show(&got), show(&want), "query {query:?} parent {:?} name {:?} basename {basename}",
                String::from_utf8_lossy(&parent), String::from_utf8_lossy(&name));
        }
    }
}

// The LCS bound in front of the typo automaton never drops a match: random names and near misses get the classic DP's distance.
#[test]
fn search_fs4_typo_bound_keeps_every_match() {
    let mut seed = 0x5146_5338_u64;
    let mut next = |limit: usize| {
        seed = seed.wrapping_mul(LCG_MULTIPLIER).wrapping_add(LCG_INCREMENT);
        (seed >> 33) as usize % limit
    };
    let alphabet = b"abcdenrAEN_.-1 /";
    let mut scratch = searchtypo::Scratch::default();
    let (mut matched, mut checked) = (0, 0);
    for query in ["redn", "rend", "render", "redner_f", "renderfr", "render_fr", "abcd.rs", "nerd-bad", "a1b2c3d4e", "edito"] {
        let matcher = Matcher::new(query);
        assert!(matcher.budget > 0, "{query:?} takes edits");
        for _ in 0..6000 {
            let mut name: Vec<u8> = query.as_bytes().to_vec();
            if next(3) == 0 {
                name = (0..1 + next(16)).map(|_| alphabet[next(alphabet.len())]).collect();
            } else {
                for _ in 0..1 + next(3) {
                    let at = next(name.len() + 1);
                    match next(4) {
                        0 if at < name.len() => name[at] = alphabet[next(alphabet.len())],
                        1 => name.insert(at, alphabet[next(alphabet.len())]),
                        2 if at < name.len() && name.len() > 1 => { name.remove(at); }
                        _ if at + 1 < name.len() => name.swap(at, at + 1),
                        _ => {}
                    }
                }
                if next(2) == 0 { name.splice(0..0, b"x_".iter().copied()); }
            }
            let mut folded = Vec::new();
            fold(&name, &mut folded);
            let expected = scratch.distance(&matcher.needle, &folded, matcher.budget);
            assert_eq!(matcher.distance(&folded, &mut scratch), expected, "{query:?} letters {:?}", String::from_utf8_lossy(&name));
            assert_eq!(matcher.typo.ascii_distance(ascii::Ascii(&name), matcher.budget), Some(expected),
                "{query:?} bytes {:?}", String::from_utf8_lossy(&name));
            matched += usize::from(expected.is_some());
            checked += 1;
        }
    }
    println!("FS4_TYPO_BOUND checked={checked} matched={matched}");
    assert!(matched > 5_000 && checked - matched > 5_000, "both outcomes ran: {matched} of {checked}");
}
