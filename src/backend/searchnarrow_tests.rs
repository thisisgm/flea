use super::super::*;
use crate::backend::searchmatch::Matcher;
use crate::backend::searchsession::Session;
use crate::backend::testdir::TestDir;

const QUERIES: [&str; 5] = [
    "render_frame.rs", "redner_frame.rs", "editor/src/components/render", ".svg",
    "archive/2024/client/reports/quarterly",
];

fn extensions(index: &Index, queries: &[String]) -> usize {
    let rows: Vec<_> = (0..index.len()).collect();
    let mut admitted = 0;
    for query in queries {
        let ends: Vec<_> = std::iter::once(0).chain(query.char_indices().map(|(at, c)| at + c.len_utf8())).collect();
        let hits: Vec<_> = ends.iter().map(|&end| score_all(&Matcher::new(&query[..end]), &rows, index)).collect();
        for (at, &end) in ends.iter().enumerate().skip(1) {
            for previous in [at - 1, at / 2, 0] {
                let before = &query[..ends[previous]];
                let now = &query[..end];
                if !Matcher::can_narrow(before, now) { continue; }
                admitted += 1;
                let candidates: Vec<_> = hits[previous].iter().map(|&(_, row)| row).collect();
                let members: HashSet<_> = candidates.iter().copied().collect();
                for &(_, row) in &hits[at] {
                    assert!(members.contains(&row), "{before:?} to {now:?} gained {:?}", index.path(row));
                }
                assert_eq!(score_all(&Matcher::new(now), &candidates, index), hits[at],
                    "narrowing preserves scores and every member: {before:?} to {now:?}");
            }
        }
    }
    admitted
}

// Knuth's MMIX linear congruential step; the seed only fixes one reproducible inventory.
const LCG_MULTIPLIER: u64 = 6364136223846793005;
const SEED: u64 = 0x5146_5332;
const ROWS: usize = 192;
// Rows below this also contribute their name, path and two typo queries.
const QUERY_ROWS: usize = 48;
const NAME_MIN: usize = 4;
const NAME_SPREAD: usize = 12;
const FOLDERS: [&str; 4] = ["redner", "café", "abxd", "archive/2024"];
// The packed typo plan holds one bit per query letter in a u64, so these lengths sit on its limit.
const PACKED_LIMIT: usize = u64::BITS as usize;

fn random_inventory() -> (Index, Vec<String>) {
    let mut seed = SEED;
    let mut next = |limit: usize| {
        seed = seed.wrapping_mul(LCG_MULTIPLIER).wrapping_add(1);
        (seed >> u32::BITS) as usize % limit
    };
    let alphabet = ['a', 'b', 'c', 'd', 'r', 'e', 'n', '1', '2', '.', '_', 'é', '中', 'İ'];
    let mut index = Index::new();
    let mut queries: Vec<String> = QUERIES.iter().map(|q| (*q).to_string()).collect();
    for i in 0..ROWS {
        let name: String = (0..NAME_MIN + next(NAME_SPREAD)).map(|_| alphabet[next(alphabet.len())]).collect();
        let path = format!("abcd/{}/{name}.{i}", FOLDERS[next(FOLDERS.len())]);
        assert!(index.push(&path, false));
        if i < QUERY_ROWS {
            queries.push(name.clone());
            queries.push(path.clone());
            let mut typo: Vec<_> = name.chars().collect();
            typo.swap(0, 1);
            queries.push(typo.iter().collect());
            typo[0] = 'x';
            queries.push(typo.iter().collect());
        }
    }
    for path in ["abxd/leaf", "abcd/leaf", "redner/frame.rs", "render_frame.rs", "café/cafè.txt",
        "alphaBeta/abcd1.svg", "abcde/fg/leaf", "abcd/efgh/ijkl.txt"] {
        assert!(index.push(path, false));
        queries.push(path.to_string());
    }
    // Exercise the packed-plan limit and fallback with a transposition near the cut.
    for length in [PACKED_LIMIT - 1, PACKED_LIMIT, PACKED_LIMIT + 1] {
        let query = format!("{}bc", "a".repeat(length - 2));
        assert!(index.push(format!("{}cb.txt", "a".repeat(length - 2)), false));
        queries.push(query);
    }
    index.finish();
    (index, queries)
}

#[test]
fn search_q1_admits_equal_budgets_and_keeps_expansion_guards() {
    for (before, now) in [("abcd", "abcde"), ("abcdefgh", "abcdefghi"), ("redner_", "redner_f"),
        ("docs/", "docs/re"), ("café", "cafés"), (".sv", ".svg"), ("abc", "abc/"), ("abcd", "abcd")] {
        assert!(Matcher::can_narrow(before, now), "safe extension: {before:?} to {now:?}");
    }
    for (before, now) in [("abc", "abcd"), ("redner_f", "redner_fr"), ("abcd", "abcd/"),
        ("abcd", "abcd/ef"), ("abcd", "abc"), ("abcd", "abce")] {
        assert!(!Matcher::can_narrow(before, now), "rescan: {before:?} to {now:?}");
    }
}

#[test]
fn search_q1_admitted_extensions_preserve_synthetic_and_random_hits() {
    let (index, _) = fixture_tests::synthetic(10_000, false);
    let queries: Vec<_> = QUERIES.iter().map(|q| (*q).to_string()).collect();
    assert!(extensions(&index, &queries) > 100);
    let (index, queries) = random_inventory();
    assert!(extensions(&index, &queries) > 1_000);
}

#[test]
fn search_q1_first_slash_and_budget_steps_really_gain_hits() {
    let (index, _) = fixture_tests::synthetic(10_000, false);
    let rows: Vec<_> = (0..index.len()).collect();
    let previous: HashSet<_> = score_all(&Matcher::new("redner_f"), &rows, &index).into_iter().map(|(_, row)| row).collect();
    assert!(score_all(&Matcher::new("redner_fr"), &rows, &index).iter().any(|(_, row)| !previous.contains(row)));
    assert!(!Matcher::can_narrow("redner_f", "redner_fr"));
    let mut index = Index::new();
    assert!(index.push("abxd/leaf", false));
    index.finish();
    assert!(score_all(&Matcher::new("abcd"), &[0], &index).is_empty());
    assert_eq!(score_all(&Matcher::new("abcd/"), &[0], &index).len(), 1);
    assert!(!Matcher::can_narrow("abcd", "abcd/"));
}

#[test]
fn search_q1_session_scores_only_previous_matches_on_admitted_extensions() {
    let dir = TestDir::new("searchq1");
    let root = dir.path().to_str().unwrap();
    let mut session = Session::open_sync(root, root, false);
    let (index, _) = fixture_tests::synthetic(10_000, false);
    *session.index.lock().unwrap() = index;
    session.priors = None;
    let mut gen = 0;
    let (mut narrowed, mut rescored) = (0, 0);
    // rendr extends the strict answer for rend into one that needs typos, which takes the rescore arm.
    for query in QUERIES.into_iter().chain(["rendr"]) {
        let mut before = "";
        let (mut previous, mut previous_typos) = (0, true);
        for (at, c) in query.char_indices() {
            let now = &query[..at + c.len_utf8()];
            gen += 1;
            let answer = session.query(gen, now, 16).unwrap();
            // Matches that left out hidden typo rows narrow the strict pass; an answer that shows typos then scores the index too.
            let expected = if !before.is_empty() && Matcher::can_narrow(before, now) {
                narrowed += 1;
                if !previous_typos && answer.typos { rescored += 1; previous + session.indexed() } else { previous }
            } else { session.indexed() };
            assert_eq!(session.counters.rows_scored, expected, "{before:?} to {now:?}");
            let index = session.index.lock().unwrap();
            let rows: Vec<_> = (0..index.len()).collect();
            let mut full = score_all(&Matcher::new(now), &rows, &index);
            let typo_rows = full.iter().filter(|hit| class(hit.0) == CLASSES - 1).count();
            let real = full.iter().any(|hit| class(hit.0) <= class(crate::backend::searchmatch::WORD));
            assert_eq!(answer.typos, !real || Matcher::new(now).budget == 0, "typo rows are skipped exactly when hidden, for {now:?}");
            let kept = if answer.typos { full.len() } else { full.len() - typo_rows };
            assert_eq!(session.last_matches.len(), kept, "narrowing keeps every match it could show, for {now:?}");
            let mut counts = class_counts(&full);
            hide_typos(&mut full, &mut counts, Matcher::new(now).budget);
            if now.contains('/') {
                let mut pairs: Vec<_> = full.iter().map(|&hit| (hit, hit.0)).collect();
                family_oracle::order(&index, &mut pairs);
                full = pairs.into_iter().map(|pair| pair.0).collect();
            } else { full.sort_unstable_by(|a, b| rank_cmp(&index, a, b)); }
            let head: Vec<_> = full.iter().take(16).map(|&(_, row)| (index.path(row), index.is_dir(row))).collect();
            assert_eq!(answer.rows, head, "identical ranked head for {now:?}");
            assert_eq!(answer.total, full.len(), "identical recall for {now:?}");
            previous = session.last_matches.len();
            previous_typos = answer.typos;
            before = now;
        }
    }
    println!("FS4_NARROW narrowed={narrowed} rescored={rescored}");
    assert!(rescored > 0, "a strict answer followed by a typo answer must take the rescore arm");
    // Nonzero-budget admission must be exercised even before can_narrow is changed.
    session.query(gen + 1, "redner_", 16).unwrap();
    let previous = session.last_matches.len();
    assert!(previous > 0 && previous < session.indexed());
    session.query(gen + 2, "redner_f", 16).unwrap();
    assert_eq!(session.counters.rows_scored, previous, "equal-budget extension scores only previous matches");
    assert!(narrowed > 50);
}
