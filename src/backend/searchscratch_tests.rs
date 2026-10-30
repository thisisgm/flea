use super::*;
use crate::backend::searchindex::{self, Index, score_cached};
use crate::backend::searchmatch::{Matcher, tests::reference};
use crate::backend::searchprior::alloc_tests::count_allocations;
use std::collections::HashSet;

fn bounds(index: &Index, rows: &[usize]) -> (usize, usize) {
    let mut names = HashSet::new();
    let mut parents = HashSet::new();
    for &row in rows {
        names.insert(index.rows[row].name);
        let mut parent = index.rows[row].parent;
        while parent != searchindex::EMPTY && parents.insert(parent) {
            parent = index.blocks.record(parent).1;
        }
    }
    (names.len(), parents.len())
}

fn target(index: &Index) -> usize {
    index.rows.iter().position(|row| index.names.get(row.name) == b"render_frame.rs").unwrap()
}

#[test]
fn search_q2_scratch_work_follows_candidates_not_index_size() {
    for count in [100_000, 1_000_000] {
        let (index, _) = searchindex::fixture_tests::synthetic(count, false);
        let rows: Vec<_> = (0..index.len()).collect();
        let mut scratch = QueryScratch::default();
        score_cached(&Matcher::new("r"), &rows, &index, &mut scratch);
        let target = target(&index);
        for candidates in [vec![target], vec![index.len() - 1], vec![0, target, index.len() - 1], vec![]] {
            let (names, parents) = bounds(&index, &candidates);
            for query in ["render_frame.rs", "qqqq", "editor/src/components/render", "redner_frame.rs"] {
                score_cached(&Matcher::new(query), &candidates, &index, &mut scratch);
                let counters = &scratch.counters;
                assert!(counters.names_scored <= names, "{count} {query}: {counters:?}, names={names}");
                assert!(counters.directory_reach <= parents, "{count} {query}: {counters:?}, parents={parents}");
                assert!(counters.scratch_touched <= names + parents, "{count} {query}: {counters:?}, slots={}", names + parents);
            }
        }
        println!("Q2_COUNTERS entries={count}: candidate name and ancestor bounds passed");
    }
}

#[test]
fn search_q2_warm_sparse_scoring_allocates_only_its_output() {
    for count in [10_000, 100_000, 1_000_000] {
        let (index, _) = searchindex::fixture_tests::synthetic(count, false);
        let mut scratch = QueryScratch::default();
        let rows: Vec<_> = (0..index.len()).collect();
        score_cached(&Matcher::new("r"), &rows, &index, &mut scratch);
        let candidates = [target(&index)];
        for (query, expected) in [("qqqq", 0), ("render_frame.rs", 1), ("editor/src/components/render", 1)] {
            let matcher = Matcher::new(query);
            score_cached(&matcher, &candidates, &index, &mut scratch);
            let (hits, allocations) = count_allocations(|| score_cached(&matcher, &candidates, &index, &mut scratch));
            println!("Q2_ALLOC entries={count} query={query:?} allocations={allocations} hits={}", hits.len());
            assert_eq!(hits.len(), expected);
            assert!(allocations <= expected, "{count} {query}: {allocations} allocations, only the output may allocate");
        }
    }
}

#[test]
fn search_q2_reused_scratch_preserves_full_first_query_and_changed_queries() {
    let mut scratch = QueryScratch::default();
    for round in 0..3 {
        let mut index = Index::new();
        for parent in ["a", "downloads", "projects/editor/src/components", "café/CAFÉ", "archive/2024"] {
            for name in ["render_frame.rs", "helper.txt", "abcd1.svg", "cafè.txt", "quarterly_review.pdf"] {
                assert!(index.push(format!("{parent}/{name}"), false));
            }
        }
        if round > 0 {
            index.epoch = round;
            assert!(index.push("new-parent/renamed.txt", false));
        }
        let rows: Vec<_> = (0..index.len()).rev().collect();
        for query in ["r", "redner_frame.rs", "editor/src/components/render", "dwnhelp", "café", "archive/2024", ".svg", "", "zzzz"] {
            let expected: Vec<_> = rows.iter().filter_map(|&row| reference(query, &index.raw(row)).map(|score| (score, row))).collect();
            assert_eq!(score_cached(&Matcher::new(query), &rows, &index, &mut scratch), expected, "round={round} {query}");
            let subset: Vec<_> = rows.iter().copied().step_by(3).collect();
            let expected: Vec<_> = subset.iter().filter_map(|&row| reference(query, &index.raw(row)).map(|score| (score, row))).collect();
            assert_eq!(score_cached(&Matcher::new(query), &subset, &index, &mut scratch), expected, "subset round={round} {query}");
        }
    }
}

#[test]
fn search_q2_replaced_arenas_and_generation_wrap_drop_query_results() {
    let mut scratch = QueryScratch::default();
    for parent in ["aaaa", "zzzz"] {
        let mut index = Index::new();
        assert!(index.push(format!("{parent}/leaf.txt"), false));
        assert!(index.push(format!("{parent}/other.txt"), false));
        for query in ["aaaa/leaf", "zzzz/leaf", "leaf", "qqqq"] {
            let expected: Vec<_> = (0..index.len()).filter_map(|row| reference(query, &index.raw(row)).map(|score| (score, row))).collect();
            assert_eq!(score_cached(&Matcher::new(query), &[0, 1], &index, &mut scratch), expected);
            scratch.generation = u32::MAX;
            assert_eq!(score_cached(&Matcher::new(query), &[0, 1], &index, &mut scratch), expected);
        }
    }
}

// The typed queries whose stale path text and states once held 35.7 MB of a 1M session after scoring.
const TYPED: [&str; 6] = ["render_frame.rs", "redner_frame.rs", "editor/src/components/render", ".svg",
    "archive/2024/client/reports/quarterly", "editor/src/components/redner"];
const TYPED_ROWS: usize = 100_000;

#[test]
fn search_g1_path_text_and_states_keep_only_the_last_generation() {
    let (index, _) = searchindex::fixture_tests::synthetic(TYPED_ROWS, false);
    let rows: Vec<_> = (0..index.len()).collect();
    // One core keeps every row on the session's shared caches, the ones this gate bounds.
    let mut scratch = QueryScratch { pinned_cores: Some(1), ..QueryScratch::default() };
    let (mut peak_text, mut peak_states) = (0, 0);
    for query in TYPED {
        for end in 1..=query.len() {
            score_cached(&Matcher::new(&query[..end]), &rows, &index, &mut scratch);
            peak_text = peak_text.max(scratch.texts.len());
            peak_states = peak_states.max(scratch.states_used);
            assert!(scratch.texts.capacity() <= SHRINK_ABOVE * scratch.texts.len(), "{:?}: text {} of {}",
                &query[..end], scratch.texts.len(), scratch.texts.capacity());
            assert!(scratch.states.len() <= SHRINK_ABOVE * scratch.states_used, "{:?}: states {} of {}",
                &query[..end], scratch.states_used, scratch.states.len());
        }
    }
    assert!(peak_text > 0 && peak_states > 0, "the typed queries must build path text and states");
    assert_eq!((scratch.names.len(), scratch.dirs.len()), (index.names.len(), index.blocks.len()));
    let state_bytes: usize = scratch.states.iter().map(|state| std::mem::size_of::<State>() + state.heap_bytes()).sum();
    println!("G1_GATE rows={TYPED_ROWS} names={} dirs={} jobs={} text={} states={state_bytes} peak_text={} peak_states={peak_states}",
        scratch.names.capacity() * std::mem::size_of::<NameSlot>(), scratch.dirs.capacity() * std::mem::size_of::<Directory>(),
        scratch.jobs.capacity() * std::mem::size_of::<(u32, i32)>(), scratch.texts.capacity() * std::mem::size_of::<Letter>(),
        peak_text * std::mem::size_of::<Letter>());
    score_cached(&Matcher::new("qqqq"), &rows, &index, &mut scratch);
    assert_eq!((scratch.texts.len(), scratch.states_used), (0, 0), "qqqq builds no path text and no states");
    assert_eq!((scratch.texts.capacity(), scratch.states.capacity()), (0, 0), "a query without path work frees the last one's");
}

// Rows split across threads, each with its own one-directory cache, score exactly as the one-thread pass over the shared caches.
#[test]
fn search_fs4_split_row_pass_matches_the_one_thread_pass() {
    let (index, _) = searchindex::fixture_tests::synthetic(TYPED_ROWS, false);
    let all: Vec<_> = (0..index.len()).collect();
    let half: Vec<_> = all.iter().copied().filter(|row| row % 3 != 1).collect();
    assert!(half.len() >= super::super::PARALLEL_ROWS_AT, "the subset must take the split arm too");
    let mut one = QueryScratch { pinned_cores: Some(1), ..QueryScratch::default() };
    let mut split = QueryScratch { pinned_cores: Some(4), ..QueryScratch::default() };
    let queries = ["r", "e", ".", "render_frame.rs", "redner_frame.rs", "editor/src/components/render", "editor/src/components/redner",
        "edtor/src/comp", "archive/2024/client/reports/quarterly", "archve/2024/clent", ".svg", "café", "qqqq", ""];
    for query in queries {
        for matcher in [Matcher::new(query), Matcher::deferred(query)] {
            for rows in [&all, &half] {
                let expected = score_cached(&matcher, rows, &index, &mut one);
                assert_eq!(score_cached(&matcher, rows, &index, &mut split), expected, "{query:?} defer={} rows={}", matcher.defer, rows.len());
            }
        }
    }
    assert_eq!(split.row_workers.len(), 4, "the four-core scratch took the split arms");
}

// The strict-first pass is the full pass where typo rows could show, and elsewhere the full pass less the typo rows hide_typos drops.
#[test]
fn search_fs4_strict_pass_matches_the_full_pass_wherever_typos_could_show() {
    use crate::backend::searchindex::{class, score_shown, CLASSES};
    use crate::backend::searchmatch::WORD;
    let (synthetic, _) = searchindex::fixture_tests::synthetic(TYPED_ROWS, false);
    let mut hand = Index::new();
    for path in ["abxd/leaf", "abcd/leaf", "redner/frame.rs", "render_frame.rs", "caf\u{e9}/caf\u{e8}.txt", "alphaBeta/abcd1.svg",
        "editor/src/components/render.rs", "edtor/src/comp/x.rs", "archive/2024/client/reports/quarterly.pdf", "quartrely.txt",
        "\u{4e2d}\u{6587}/redne.txt", "r\u{e9}dner.md", "renderer/frame_redner.rs"] {
        assert!(hand.push(path, false));
    }
    // Digit rows take the hand index to the whole-table arm, whose name table holds parent-only names such as abxd and redner.
    for pad in 0..searchindex::PARALLEL_SCORE_AT { assert!(hand.push(format!("{pad:04}"), false)); }
    hand.finish();
    let queries = ["abxd", "redner", "render_frame.rs", "redner_frame.rs", "editor/src/components/render", "editor/src/components/redner", "edtor/src/comp",
        "archve/2024/clent", "redn", "redne", "rendr", "edito", "caf\u{e8}", "cafe", "abxd/leaf", "abdc/leaf", "quarterly", "quartrely",
        ".svg", "zzzz", "r\u{e9}dne", "frame"];
    let (mut strict, mut full_answers) = (0, 0);
    for index in [&synthetic, &hand] {
        let all: Vec<_> = (0..index.len()).collect();
        let few: Vec<_> = all.iter().copied().step_by(7).collect();
        for rows in [&all, &few] {
            for query in queries {
                for matcher in [Matcher::new(query), Matcher::deferred(query)] {
                    for cores in [Some(1), None] {
                        let full = score_cached(&matcher, rows, index, &mut QueryScratch { pinned_cores: cores, ..QueryScratch::default() });
                        let mut shown_scratch = QueryScratch { pinned_cores: cores, ..QueryScratch::default() };
                        let (shown, typos) = score_shown(&matcher, rows, index, &mut shown_scratch);
                        assert_eq!(shown_scratch.whole, rows.len() == index.len(), "{query:?} whole-table arm, rows={}", rows.len());
                        let real = full.iter().any(|hit| class(hit.0) <= class(WORD));
                        assert_eq!(typos, !real || matcher.budget == 0, "{query:?} rows={}", rows.len());
                        let expected: Vec<_> = if typos { full.clone() } else { full.iter().copied().filter(|hit| class(hit.0) < CLASSES - 1).collect() };
                        assert_eq!(shown, expected, "{query:?} defer={} rows={}", matcher.defer, rows.len());
                        if typos { full_answers += 1 } else { strict += 1 }
                    }
                }
            }
        }
    }
    println!("FS4_STRICT strict={strict} full={full_answers}");
    assert!(strict > 20 && full_answers > 20, "both arms ran: strict={strict} full={full_answers}");
}

fn rand(seed: &mut u64, below: usize) -> usize {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    (*seed % below as u64) as usize
}

// One read of an ASCII name gives the score name_bytes gives and, deferred, the basename score the DP gives.
#[test]
fn search_t10_name_own_is_name_bytes_and_the_dp() {
    use crate::backend::searchmatch::{ascii::Bits, PENDING_NAME};
    let mut seed = 0x9e37_79b9_7f4a_7c15;
    let letters_only = b"aAbBdDeEiIrRtT";
    let mixed = b"aAbBdDeEiIrRtT._- 09";
    let mut queries: Vec<String> = ["e", "ed", "edit", "red", "a.b", "_d", "rr", "eee", "x", ".", "e e", "deb", "Ab", "a/b", "caf\u{e9}"]
        .map(String::from).to_vec();
    for _ in 0..120 {
        let pool: &[u8] = if rand(&mut seed, 3) == 0 { mixed } else { letters_only };
        let len = 1 + rand(&mut seed, 5);
        queries.push((0..len).map(|_| pool[rand(&mut seed, pool.len())] as char).collect());
    }
    let mut names: Vec<Vec<u8>> = [&b""[..], &[b'e'; 64], &[b'e'; 65], b"detail_detail.txt", b"RedNote", b"rEDitor", "caf\u{e9}_edit".as_bytes()]
        .map(<[u8]>::to_vec).to_vec();
    for _ in 0..2500 {
        let len = rand(&mut seed, 70);
        names.push((0..len).map(|_| mixed[rand(&mut seed, mixed.len())]).collect());
    }
    let (mut letters, mut state, mut edits, mut bits) = (Vec::new(), State::new(0), Default::default(), Bits::new());
    let (mut owned, mut lifts) = (0, 0);
    for query in &queries {
        let exact = Matcher::new(query);
        for matcher in [Matcher::deferred(query), Matcher::deferred(query).strict(), Matcher::new(query), exact.strict()] {
            for name in &names {
                state.resize(matcher.needle.len());
                let (score, own) = matcher.name_own(name, &mut letters, &mut state, &mut edits, &mut bits);
                state.resize(matcher.needle.len());
                assert_eq!(score, matcher.name_bytes(name, &mut letters, &mut state, &mut edits), "{query:?} {:?}", String::from_utf8_lossy(name));
                let Some(own) = own else { continue };
                owned += 1;
                assert_eq!(score, Some(PENDING_NAME));
                state.resize(exact.needle.len());
                assert_eq!(Some(own.score()), exact.name_bytes(name, &mut letters, &mut state, &mut edits), "{query:?} {:?}", String::from_utf8_lossy(name));
                if name.len() <= 12 {
                    let needle: Vec<u8> = query.bytes().map(|byte| byte.to_ascii_lowercase()).collect();
                    assert_eq!((needle.len() > 1).then_some(own.lift()), brute_lift(name, &needle), "{query:?} {:?}", String::from_utf8_lossy(name));
                    lifts += 1;
                }
            }
        }
    }
    assert!(owned > 5_000 && lifts > 500, "deferred passes read own scores: {owned}, checked lifts: {lifts}");
}

// The lift by enumeration: each chain of a later needle suffix through the name, scored as the path DP scores it after a parent column of 0.
fn brute_lift(name: &[u8], needle: &[u8]) -> Option<i32> {
    use crate::backend::searchmatch::{separator, BASENAME_BONUS, BOUNDARY_BONUS, RUN_BONUS};
    fn walk(name: &[u8], rest: &[u8], last: Option<usize>, score: i32, best: &mut Option<i32>) {
        let Some((&letter, rest)) = rest.split_first() else { *best = (*best).max(Some(score)); return };
        for at in last.map_or(0, |last| last + 1)..name.len() {
            if name[at].to_ascii_lowercase() != letter { continue; }
            let start = at == 0 || separator(name[at - 1] as char) || (name[at].is_ascii_uppercase() && !name[at - 1].is_ascii_uppercase());
            let link = match last { None => -(at as i32), Some(last) if at == last + 1 => RUN_BONUS, Some(last) => -((at - last - 1) as i32) };
            walk(name, rest, Some(at), score + BASENAME_BONUS + if start { BOUNDARY_BONUS } else { 0 } + link, best);
        }
    }
    let mut best = None;
    for split in 1..needle.len() { walk(name, &needle[split..], None, 0, &mut best); }
    best
}

// Own scores from the name pass and the parent bound give every pending row the score the whole-path DP gives, with fewer DPs.
#[test]
fn search_t10_resolve_with_own_scores_is_the_path_dp() {
    use crate::backend::searchindex::resolve;
    use crate::backend::searchmatch::PENDING_NAME;
    use std::sync::atomic::Ordering;
    let (synthetic, _) = searchindex::fixture_tests::synthetic(TYPED_ROWS, false);
    let mut hand = Index::new();
    // Parents that hold the needle, or its head at word starts, lift a basename that holds it mid-word; others never can.
    for path in ["red/xrxexdx.txt", "re/xdx.txt", "r/xexdx", "x/xrxexdx.txt", "red/sub/xrxexdx.txt", "Red/xRxExDx", "edit/xexdxixt",
        "e/xe", "xe", "caf\u{e9}/xcxaxfx\u{e9}x", "a/b/c/d/redx.txt", "e/d/i/t/xeditx", &format!("red/{}x", "e".repeat(70))] {
        assert!(hand.push(path, false));
    }
    hand.finish();
    let queries = ["e", "ed", "edit", "red", "r", "a", ".", "x", "caf\u{e9}", "render", "frame", "rxd", "xe", "xedx", "edito", "redn", "d/x"];
    let (mut saved, mut lifted, mut shared) = (0, false, false);
    for index in [&synthetic, &hand] {
        let all: Vec<_> = (0..index.len()).collect();
        let few: Vec<_> = all.iter().copied().step_by(7).collect();
        for rows in [&all, &few] {
            for query in queries {
                for cores in [Some(1), Some(4)] {
                    let mut scratch = QueryScratch { pinned_cores: cores, ..QueryScratch::default() };
                    let mut exact = score_cached(&Matcher::new(query), rows, index, &mut scratch);
                    let deferred = Matcher::deferred(query);
                    let mut hits = score_cached(&deferred, rows, index, &mut scratch);
                    let pending = hits.iter().filter(|hit| hit.0 == PENDING_NAME && index.rows[hit.1].parent != searchindex::EMPTY).count();
                    index.path_dp.store(0, Ordering::Relaxed);
                    resolve(&deferred, index, &mut scratch, &mut hits);
                    let path_dps = index.path_dp.swap(0, Ordering::Relaxed);
                    shared |= cores == Some(4) && scratch.counters.resolve_largest_share > 0;
                    hits.sort_unstable_by_key(|hit| hit.1);
                    exact.sort_unstable_by_key(|hit| hit.1);
                    assert_eq!(hits, exact, "{query:?} cores={cores:?} rows={}", rows.len());
                    saved += pending.saturating_sub(path_dps);
                    let score = |path: &str| exact.iter().find(|hit| index.path(hit.1) == path).map(|hit| hit.0);
                    lifted |= query == "red" && score("red/xrxexdx.txt") > score("x/xrxexdx.txt");
                }
            }
        }
    }
    assert!(saved > 10_000, "own scores replaced path DPs: {saved}");
    assert!(lifted, "a parent that holds the needle lifted a basename above its own score");
    assert!(shared, "the four-core scratch resolved rows across threads");
}

// An ASCII basename read as bytes holds the same needle suffix its folded letters hold.
#[test]
fn search_t10_held_suffix_reads_ascii_as_its_letters() {
    use crate::backend::searchmatch::fold;
    let mut seed = 0x2545_f491_4f6c_dd1d;
    let pool = b"aAbBdDeEnNrR._- 09";
    let (mut letters, mut folded) = (Vec::new(), Vec::new());
    let mut checked = 0;
    for query in ["redn", "n", "e.d", "rDn", "ab_", "nnn"] {
        let matcher = Matcher::new(query);
        for _ in 0..2000 {
            let len = rand(&mut seed, 20);
            let name: Vec<u8> = (0..len).map(|_| pool[rand(&mut seed, pool.len())]).collect();
            fold(&name, &mut folded);
            let mut from = matcher.needle.len();
            for letter in folded.iter().rev() {
                if from > 0 && letter.lower == matcher.needle[from - 1] { from -= 1; }
            }
            assert_eq!(super::super::held_suffix(&matcher, &name, &mut letters), from, "{query:?} {:?}", String::from_utf8_lossy(&name));
            checked += usize::from(from < matcher.needle.len());
        }
    }
    assert!(checked > 1000, "names holding a suffix: {checked}");
}

#[test]
fn search_t10_own_scores_ride_in_twelve_bytes() {
    // Every name slot and every job carries one, so it costs two i16 halves, not two i32s.
    assert_eq!(std::mem::size_of::<NameSlot>(), 12);
    assert_eq!(std::mem::size_of::<(u32, i32, Own)>(), 12);
}
