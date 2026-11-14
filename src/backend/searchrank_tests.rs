// Head-bounded ranking must answer, head and rest alike, exactly what scoring every row and sorting every match answers.
use crate::backend::searchindex::{fixture_tests, rank_cmp, score_cached, Index, QueryScratch};
use crate::backend::searchindex::family_oracle;
use crate::backend::searchmatch::{Matcher, ALIGNED, EXACT, HIT_SPAN, NAME, PREFIX, SHADOW_TYPO, TYPO, WORD};
use crate::backend::searchprior::alloc_tests::fixture_priors;
use crate::backend::searchsession::{Session, FIRST_WINDOW_DEFAULT};
use crate::backend::testdir::TestDir;
use std::collections::BTreeSet;
use std::os::unix::ffi::OsStrExt;
use std::sync::atomic::Ordering;

#[path = "searcha42_tests.rs"]
mod a42;

const QUERIES: [&str; 5] = ["render_frame.rs", "redner_frame.rs", "editor/src/components/render", ".svg", "archive/2024/client/reports/quarterly"];
const CHUNK: usize = FIRST_WINDOW_DEFAULT;
// searchprior's SCALE: a prior bonus is added after the score is multiplied by this.
const SCALE: i32 = 64;
const TIERS: usize = 6;
const CORPUS_FILES: usize = 4000;
const SYNTHETIC_ROWS: usize = 100_000;
// Every nth file and nth folder carries a prior: sparse on the synthetic tree, dense on the corpus.
const SYNTHETIC_PRIOR_EVERY: (usize, usize) = (1500, 400);
const CORPUS_PRIOR_EVERY: (usize, usize) = (37, 11);
// Each typo edit costs TYPO_STEP, and two edits is the most the typo budget allows.
const TYPO_STEP: i32 = 1000;
const BUDGET_EDITS: i32 = 2;
// searchprior's smallest direct-use bonus, 24 plus twice a strength of 1.
const DIRECT_BONUS_MIN: i32 = 26;
// Knuth's MMIX LCG; the high bits are the well-mixed ones.
const LCG_MULTIPLIER: u64 = 6364136223846793005;
const LCG_INCREMENT: u64 = 1442695040888963407;
const LCG_SHIFT: u32 = 33;
// corpus_name's arms, one per name shape.
const NAME_SHAPES: usize = 7;
// Filler between letters, so the name matches only as a spread-out subsequence.
const SPREAD_GAP: usize = 20;
const NUMBER_SUFFIXES: usize = 100;
const FOLDER_DEPTHS: usize = 4;
// One folder in this many is a one-edit typo of its word.
const TYPO_FOLDER_ONE_IN: usize = 5;
const CORPUS_SEED: u64 = 0x5eed_f53b;
// A word match no name starts with, at windows that end inside its WORD class.
const WORD_ONLY: &str = "case";
const WORD_ONLY_WINDOWS: [usize; 2] = [1, 3];

// Score tiers from best to worst, kept apart from class() so a wrong floor there shows as a DP count mismatch here.
fn tier(score: i32) -> usize {
    [EXACT, PREFIX, WORD, ALIGNED - HIT_SPAN, NAME - HIT_SPAN].iter().position(|&floor| score >= floor).unwrap_or(TIERS - 1)
}

struct Reference {
    all: Vec<usize>,
    order: Vec<(String, bool)>,
    raw: Vec<i32>,
    bonus: Vec<i32>,
}

// Every candidate scored with the full DP and every match sorted by rank_cmp.
fn reference(session: &Session, text: &str) -> Reference {
    let index = session.index.lock().unwrap();
    let rows: Vec<usize> = (0..index.len()).collect();
    let terms = crate::backend::searchtags::Tags::terms(text);
    let text = terms.text;
    let mut raw = score_cached(&Matcher::new(&text), &rows, &index, &mut QueryScratch::default());
    // Shadow rows never show, and typo rows only when no basename holds the query exactly, as its prefix or at a word start.
    let real = raw.iter().any(|hit| tier(hit.0) <= tier(WORD));
    // An untagged answer skips the typo rows it would hide, so its match list holds none of them.
    let skipped = real && terms.tags.is_empty() && Matcher::new(&text).budget > 0;
    let all: Vec<usize> = raw.iter().filter(|hit| !skipped || tier(hit.0) < TIERS - 1).map(|&(_, row)| row).collect();
    if Matcher::new(&text).budget > 0 { raw.retain(|hit| tier(hit.0) < TIERS - 1 || (!real && hit.0 > SHADOW_TYPO)); }
    let mut ranked = raw.clone();
    crate::backend::searchprior::scale(session.priors, &session.scope, &session.folder, &index, &mut ranked, all.len());
    let applied = ranked.iter().zip(&raw).any(|(ranked, raw)| ranked.0 != raw.0);
    let mut pairs: Vec<_> = ranked.iter().copied().zip(raw.iter().map(|hit| hit.0)).collect();
    if text.contains('/') { family_oracle::order(&index, &mut pairs); }
    else { pairs.sort_by(|a, b| rank_cmp(&index, &a.0, &b.0)); }
    Reference {
        all,
        order: pairs.iter().map(|&((_, row), _)| (index.path(row), index.is_dir(row))).collect(),
        raw: pairs.iter().map(|&(_, raw)| raw).collect(),
        bonus: pairs.iter().map(|&((ranked, _), raw)| if applied { ranked - raw * SCALE } else { 0 }).collect(),
    }
}

#[derive(Default)]
struct Seen {
    boundaries: BTreeSet<usize>,
    // (boundary tier, next tier the answer holds below it) when that next tier needs a DP, so merging the two over-evaluates.
    deferred: BTreeSet<(usize, usize)>,
    boundary_bonus: i32,
    typo_edge: bool,
    negative_name: bool,
    window_past_total: bool,
}

// One typed prefix: the answer's head, total and match list, then every rest chunk until the rest is spent.
fn check(session: &mut Session, gen: &mut usize, text: &str, window: usize, seen: &mut Seen) -> (usize, usize) {
    let want = reference(session, text);
    {
        let index = session.index.lock().unwrap();
        index.name_dp.store(0, Ordering::Relaxed);
        index.path_dp.store(0, Ordering::Relaxed);
    }
    *gen += 1;
    let answer = session.query(*gen, text, window).unwrap();
    let dp = { let index = session.index.lock().unwrap(); index.name_dp.load(Ordering::Relaxed) + index.path_dp.load(Ordering::Relaxed) };
    let head = window.min(want.order.len());
    assert_eq!(answer.total, want.order.len(), "{text:?} total");
    assert_eq!(session.last_matches, want.all, "{text:?} all");
    assert_eq!(answer.rows, want.order[..head], "{text:?} head of {window}");
    let mut adopted = head;
    while let Some(rows) = session.take_rest_chunk(adopted + CHUNK, adopted) {
        assert!(!rows.is_empty(), "{text:?} empty chunk at {adopted}");
        assert_eq!(rows, want.order[adopted..adopted + rows.len()], "{text:?} rest chunk at {adopted}");
        adopted += rows.len();
    }
    assert_eq!(adopted, want.order.len(), "{text:?} rest spent");
    let boundary = if head == 0 { TIERS } else { tier(want.raw[head - 1]) };
    let next = want.raw.iter().map(|&raw| tier(raw)).filter(|&below| below > boundary).min();
    if let Some(next) = next.filter(|&next| head > 0 && (tier(ALIGNED)..=tier(NAME)).contains(&next)) { seen.deferred.insert((boundary, next)); }
    if head > 0 {
        seen.boundaries.insert(boundary);
        seen.boundary_bonus = seen.boundary_bonus.max(want.raw.iter().zip(&want.bonus)
            .filter(|(raw, _)| tier(**raw) == boundary).map(|(_, bonus)| *bonus).max().unwrap_or(0));
    }
    seen.typo_edge |= want.raw.contains(&(TYPO - BUDGET_EDITS * TYPO_STEP));
    seen.negative_name |= want.raw.iter().any(|&raw| (NAME - HIT_SPAN..NAME).contains(&raw));
    seen.window_past_total |= window > want.order.len() && !want.order.is_empty();
    // Rows whose class needs a DP and that sit in the boundary tier or above: the most a bounded answer may evaluate.
    let pending = want.raw.iter().filter(|&&raw| (tier(ALIGNED)..=tier(NAME)).contains(&tier(raw)) && tier(raw) <= boundary).count();
    (dp, pending)
}

fn priors_for(dir: &TestDir, index: &Index, files_every: usize, folders_every: usize) -> &'static crate::backend::searchprior::Priors {
    let absolute = |row| {
        let mut path = dir.path().as_os_str().as_bytes().to_vec();
        path.push(b'/');
        path.extend_from_slice(&index.raw(row));
        path
    };
    let files = (0..index.len()).filter(|&row| !index.is_dir(row)).step_by(files_every).map(absolute).collect();
    let folders = (0..index.len()).filter(|&row| index.is_dir(row)).step_by(folders_every).map(absolute).collect();
    let priors = Box::leak(Box::new(fixture_priors(files, folders)));
    assert!(!priors.is_empty());
    priors
}

fn synthetic_session(dir: &TestDir) -> Session {
    let scope = dir.path().to_str().unwrap();
    let mut session = Session::open_sync(scope, scope, false);
    let (index, _) = fixture_tests::synthetic(SYNTHETIC_ROWS, false);
    session.priors = Some(priors_for(dir, &index, SYNTHETIC_PRIOR_EVERY.0, SYNTHETIC_PRIOR_EVERY.1));
    *session.index.lock().unwrap() = index;
    session
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> usize {
        self.0 = self.0.wrapping_mul(LCG_MULTIPLIER).wrapping_add(LCG_INCREMENT);
        (self.0 >> LCG_SHIFT) as usize
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.next() % items.len()]
    }
}

const WORDS: [&str; 16] = ["render", "frame", "Report", "quarterly", "archive", "client", "editor", "components", "café", "naïve",
    "日本語", "data2024", "v10", "snake_case", "CamelCase", "x.y"];

fn edited(rng: &mut Rng, word: &str, edits: usize) -> String {
    let mut chars: Vec<char> = word.chars().collect();
    for _ in 0..edits {
        let at = rng.next() % chars.len();
        chars[at] = *rng.pick(&['q', 'z', 'x', '0']);
    }
    chars.into_iter().collect()
}

// Names that hold exact, prefix, word, spread-out, typo and non-ASCII matches for the corpus queries.
fn corpus_name(rng: &mut Rng) -> String {
    let word = *rng.pick(&WORDS);
    let mut name = match rng.next() % NAME_SHAPES {
        0 => word.to_string(),
        1 => format!("{}_{word}", rng.pick(&WORDS)),
        2 => edited(rng, word, 1),
        3 => edited(rng, word, 2),
        4 => word.chars().map(|c| format!("{c}{}", "w".repeat(SPREAD_GAP))).collect(),
        5 => word.to_uppercase(),
        _ => format!("{word}{}", rng.next() % NUMBER_SUFFIXES),
    };
    name.push_str(rng.pick(&["", ".rs", ".svg", ".md", ".tar.gz", "-1", " copy"]));
    name
}

fn corpus_session(dir: &TestDir) -> Session {
    let mut rng = Rng(CORPUS_SEED);
    let mut index = Index::new();
    let mut seen = BTreeSet::new();
    while seen.len() < CORPUS_FILES {
        let depth = rng.next() % FOLDER_DEPTHS;
        let mut path = String::new();
        for _ in 0..depth {
            let word = *rng.pick(&WORDS);
            let part = if rng.next().is_multiple_of(TYPO_FOLDER_ONE_IN) { edited(&mut rng, word, 1) } else { word.to_string() };
            path.push_str(&part);
            if seen.insert(path.clone()) { assert!(index.push(&path, true)); }
            path.push('/');
        }
        path.push_str(&corpus_name(&mut rng));
        if seen.insert(path.clone()) { assert!(index.push(&path, false)); }
    }
    let scope = dir.path().to_str().unwrap();
    let mut session = Session::open_sync(scope, scope, false);
    session.priors = Some(priors_for(dir, &index, CORPUS_PRIOR_EVERY.0, CORPUS_PRIOR_EVERY.1));
    *session.index.lock().unwrap() = index;
    session
}

const CORPUS_CHAINS: [&str; 20] = ["render_frame", "rendr", "qaurterly", "quartelry", "archive/client", "client/rep", "editor/comp",
    ".svg", "日本", "café", "data2", "v1", "x.y", "Report", "CamelCase", "snake", "frame", "arcrep", "edcomp", "r/f"];
const WINDOWS: [usize; 6] = [1, 2, 7, 50, 350, 1_000_000];

// Every typed prefix of the chains with a rotating window, then a word match that no name starts with.
fn corpus_queries() -> Vec<(String, usize)> {
    let mut queries = Vec::new();
    for (at, chain) in CORPUS_CHAINS.iter().enumerate() {
        for (step, (offset, c)) in chain.char_indices().enumerate() {
            queries.push((chain[..offset + c.len_utf8()].to_string(), WINDOWS[(at + step) % WINDOWS.len()]));
        }
    }
    queries.extend(WORD_ONLY_WINDOWS.map(|window| (WORD_ONLY.to_string(), window)));
    queries
}

#[test]
fn search_rb_bench_prefixes_match_full_scoring_at_100k() {
    let dir = TestDir::new("searchrbsynthetic");
    let mut session = synthetic_session(&dir);
    let (mut gen, mut seen) = (0, Seen::default());
    for query in QUERIES {
        for end in 1..=query.len() { check(&mut session, &mut gen, &query[..end], CHUNK, &mut seen); }
    }
    println!("RB_SYNTHETIC boundaries={:?} boundary_bonus={}", seen.boundaries, seen.boundary_bonus);
}

#[test]
fn search_rb_seeded_corpus_matches_full_scoring_in_every_boundary_class() {
    let dir = TestDir::new("searchrbcorpus");
    let mut session = corpus_session(&dir);
    let (mut gen, mut seen) = (0, Seen::default());
    for (text, window) in corpus_queries() { check(&mut session, &mut gen, &text, window, &mut seen); }
    println!("RB_CORPUS boundaries={:?} boundary_bonus={} typo_edge={} negative_name={}", seen.boundaries, seen.boundary_bonus, seen.typo_edge, seen.negative_name);
    assert_eq!(seen.boundaries, (0..TIERS).collect(), "every class is the boundary at least once");
    assert!(seen.boundary_bonus >= DIRECT_BONUS_MIN, "a prior-boosted row sits in a boundary class");
    assert!(seen.typo_edge && seen.negative_name && seen.window_past_total);
}

#[test]
fn search_rb_deferred_dp_stays_within_the_boundary_classes() {
    let dir = TestDir::new("searchrbcounts");
    let mut session = corpus_session(&dir);
    let (mut gen, mut seen) = (0, Seen::default());
    let (mut failures, mut evaluated) = (Vec::new(), 0);
    for (text, window) in corpus_queries() {
        let (dp, pending) = check(&mut session, &mut gen, &text, window, &mut seen);
        if dp > pending { failures.push(format!("{text:?}: {dp} DP for {pending} pending")); }
        evaluated += dp;
    }
    assert!(failures.is_empty(), "{} of the answers ran more DPs than their boundary classes hold: {}", failures.len(), failures.join("; "));
    assert!(evaluated > 0, "the DP counters fired on at least one answer");
    // ALIGNED needs a slash in the query and no name holds one, so a merged class floor can only over-evaluate at this pair.
    let pair = (tier(WORD), tier(NAME));
    assert!(seen.deferred.contains(&pair), "boundary tier {} with DP rows of tier {} waiting below it: seen {:?}", pair.0, pair.1, seen.deferred);
}

#[test]
fn search_rb_re_answers_without_any_dp_at_100k() {
    let dir = TestDir::new("searchrbre");
    let mut session = synthetic_session(&dir);
    let (mut gen, mut seen) = (0, Seen::default());
    check(&mut session, &mut gen, "r", CHUNK, &mut seen);
    let (dp, _) = check(&mut session, &mut gen, "re", CHUNK, &mut seen);
    assert_eq!(dp, 0, "PREFIX rows fill the window for re, so no row needs a DP");
}

// One index of `count` rows per stem, each stem followed by a four-digit number and `.txt`; returns the DP and pending counts and the reference order.
fn bound_case(dir: &str, groups: &[(&str, usize)], needle: &str) -> (usize, usize, Vec<String>) {
    let dir = TestDir::new(dir);
    let scope = dir.path().to_str().unwrap();
    let mut session = Session::open_sync(scope, scope, false);
    let mut index = Index::new();
    for &(stem, count) in groups { for i in 0..count { index.push(format!("{stem}{i:04}.txt"), false); } }
    *session.index.lock().unwrap() = index;
    let order = reference(&session, needle).order.into_iter().map(|(path, _)| path).collect();
    let (mut gen, mut seen) = (0, Seen::default());
    let (dp, pending) = check(&mut session, &mut gen, needle, CHUNK, &mut seen);
    (dp, pending, order)
}

#[test]
fn search_head_leaves_unscored_the_path_only_rows_whose_bound_cannot_reach_the_window() {
    // Each name spells r, e, d at word starts, so it scores above any row whose name holds none of them under `shared`.
    let (strong, weak) = (CHUNK + 50, 3 * CHUNK);
    let (dp, pending, _) = bound_case("searchpathbound", &[("a/r_e_d_", strong), ("shared/x_", weak)], "red");
    assert_eq!(pending, strong + weak, "every match sits in the NAME class");
    assert_eq!(dp, strong, "the head scored the strong names and no row under `shared`");
}

#[test]
fn search_head_scores_a_path_only_row_whose_basename_suffix_reaches_the_window_floor() {
    // r/ed_ and rx_e_d_ both score NAME + 27 and the shorter path wins the tie, so the floor is 27 and every r/ row ranks first.
    let (names, paths) = (CHUNK + 50, 20);
    let (dp, pending, order) = bound_case("searchpathadmit", &[("a/rx_e_d_", names), ("r/ed_", paths)], "red");
    assert_eq!(order[..CHUNK].iter().filter(|path| path.starts_with("r/")).count(), paths, "every r/ row belongs in the window");
    assert_eq!(dp, pending, "the bound admitted every r/ row to its DP");
}

#[test]
fn search_head_scores_a_path_only_row_whose_parent_ends_in_the_needle_prefix() {
    // re/d_ takes r and e in the parent and scores NAME + 23, its bound exactly, tied with every rxxx_exx_d_ name at the floor.
    let (names, paths) = (CHUNK + 50, 20);
    let (dp, pending, order) = bound_case("searchpathcross", &[("a/rxxx_exx_d_", names), ("re/d_", paths)], "red");
    assert_eq!(order[..CHUNK].iter().filter(|path| path.starts_with("re/")).count(), paths, "every re/ row belongs in the window");
    assert_eq!(dp, pending, "a bound equal to the floor admitted every re/ row to its DP");
}

#[test]
fn search_head_moves_below_every_row_scored_under_the_window_floor() {
    // r_e_d_ names score NAME + 28 and fill the window, and every rxxxx_exx_d_ name scores 22, so none of those can reach it.
    let (strong, weak) = (CHUNK + 50, 3 * CHUNK);
    let dir = TestDir::new("searchheadfloor");
    let scope = dir.path().to_str().unwrap();
    let mut session = Session::open_sync(scope, scope, false);
    let mut index = Index::new();
    for (stem, count) in [("a/r_e_d_", strong), ("a/rxxxx_exx_d_", weak)] { for i in 0..count { index.push(format!("{stem}{i:04}.txt"), false); } }
    *session.index.lock().unwrap() = index;
    assert!(reference(&session, "red").order[..strong].iter().all(|(path, _)| path.starts_with("a/r_e_d_")), "every r_e_d_ row ranks above every rxxxx_exx_d_ row");
    session.query(1, "red", CHUNK).unwrap();
    assert_eq!(session.pending_rest.as_ref().map_or(0, |rest| rest.ranked.len()), strong - CHUNK, "only the r_e_d_ rows past the window wait ranked");
    let (mut gen, mut seen) = (1, Seen::default());
    check(&mut session, &mut gen, "red", CHUNK, &mut seen);
}

#[test]
fn search_head_floor_counts_the_rows_of_earlier_classes() {
    // red_ PREFIX rows take all but 100 places, so the floor is the 100th name at NAME + 28 and the re/ rows bounded at 23 skip their DP.
    let (prefix, names, paths) = (CHUNK - 100, 100, 20);
    let groups = [("a/red_", prefix), ("a/r_e_d_", names), ("re/d_", paths), ("re/d_e_", paths)];
    let (dp, pending, order) = bound_case("searchheadclasses", &groups, "red");
    assert!(order[..prefix].iter().all(|path| path.starts_with("a/red_")) && order[prefix..CHUNK].iter().all(|path| path.starts_with("a/r_e_d_")),
        "the PREFIX rows and then the names fill the window");
    assert_eq!(pending, names + 2 * paths, "the names and the re/ rows sit in the NAME class");
    assert_eq!(dp, names, "the head scored every name and no re/ row");
}

#[test]
fn search_head_moves_below_every_resolved_row_a_pruned_path_row_could_tie() {
    // Names at NAME + 27 set the floor; re/d_ and re/d_e_ score 23 under it, and a chunk of rxxxx_exx_d_ names at 22 could fill the first rest chunk alone.
    let (strong, paths, low) = (CHUNK + 50, 20, CHUNK);
    let groups = [("a/rx_e_d_", strong), ("re/d_", paths), ("re/d_e_", paths), ("a/rxxxx_exx_d_", low)];
    let (dp, pending, order) = bound_case("searchpathceiling", &groups, "red");
    let first = |stem: &str| order.iter().position(|path| path.starts_with(stem)).unwrap();
    let last_path = order.iter().rposition(|path| path.starts_with("re/")).unwrap();
    assert!(first("re/") >= CHUNK && last_path < first("a/rxxxx_"), "every re/ row ranks after the window and above every 22-point name");
    assert_eq!(pending, strong + 2 * paths + low, "every match sits in the NAME class");
    assert_eq!(dp, strong + low, "the head scored every name and no re/ row");
}

#[test]
fn search_typo_rows_answer_only_a_query_no_basename_holds() {
    // FP-1: one-edit words never sit beside a real hit, and a query with no real hit still finds its word by typo.
    let dir = TestDir::new("searchtypohide");
    let scope = dir.path().to_str().unwrap();
    let mut session = Session::open_sync(scope, scope, false);
    let mut index = Index::new();
    for path in ["notes/flea.txt", "Flea-notes.md", "audio-flac.flac", "fleet.txt", "flex.rs", "flame.png", "src/render_frame.rs"] {
        assert!(index.push(path, false));
    }
    *session.index.lock().unwrap() = index;
    let (mut gen, mut seen) = (0, Seen::default());
    for (query, want) in [("flea", &["Flea-notes.md", "notes/flea.txt"][..]), ("frame", &["src/render_frame.rs"]), ("flae", &["notes/flea.txt", "fleet.txt"])] {
        check(&mut session, &mut gen, query, CHUNK, &mut seen);
        let files: Vec<_> = reference(&session, query).order.into_iter().filter(|(_, dir)| !dir).map(|(path, _)| path).collect();
        let typo = files.iter().filter(|path| !want.iter().any(|name| path.ends_with(name))).count();
        assert!(want.iter().all(|name| files.iter().any(|path| path.ends_with(name))), "{query:?} lists {want:?}: {files:?}");
        assert!(query == "flae" || typo == 0, "{query:?} lists no typo row beside its real hits: {files:?}");
    }
    // A mid-word dropped letter leaves "flea" a shadow of "flac": never listed, still kept for narrowing.
    let mut index = Index::new();
    for path in ["audio-flac.flac", "music/song.flac"] { assert!(index.push(path, false)); }
    *session.index.lock().unwrap() = index;
    check(&mut session, &mut gen, "flea", CHUNK, &mut seen);
    let answer = session.query(gen + 1, "flea", CHUNK).unwrap();
    assert_eq!((answer.total, session.last_matches.len()), (0, 2), "shadow rows are kept for narrowing and never listed");
}
