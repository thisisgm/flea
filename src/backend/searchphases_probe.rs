// Loaded-prior measurements use the real Session::query path and leave all measured work intact.
use super::*;
use crate::backend::searchprior::alloc_tests::{fixture_counts, fixture_priors};
use crate::backend::searchsession::{Session, timing::Phases};
use crate::backend::testdir::TestDir;
use std::time::Instant;

const PROBE_ROWS: usize = 1_000_000;
const RECENT_FILES: usize = 64;
const FREQUENT_FOLDERS: usize = 16;
const TARGETS: [&str; 5] = ["r", "re", "render", "render_fra", "archive/2"];
// Seven samples a batch and two batches, so drift between batches shows beside each median.
const SAMPLES: usize = 7;
const BATCHES: usize = 2;
const MS_PER_SECOND: f64 = 1e3;
const PERCENT: f64 = 100.0;

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

#[test]
#[ignore]
fn search_q3_loaded_phase_probe() {
    let dir = TestDir::new("searchq3phases");
    let scope = dir.path().to_str().unwrap();
    let mut session = Session::open_sync(scope, scope, false);
    let (index, _) = fixture_tests::synthetic(PROBE_ROWS, false);
    let files = (0..index.len()).filter(|&row| !index.is_dir(row)).count();
    let folders = index.len() - files;
    let absolute = |row| {
        let mut path = dir.path().as_os_str().as_bytes().to_vec();
        path.push(b'/');
        path.extend_from_slice(&index.raw(row));
        path
    };
    let recent = (0..index.len()).filter(|&row| !index.is_dir(row)).step_by((files / RECENT_FILES).max(1)).take(RECENT_FILES).map(absolute).collect();
    let frequent = (0..index.len()).filter(|&row| index.is_dir(row)).step_by((folders / FREQUENT_FOLDERS).max(1)).take(FREQUENT_FOLDERS).map(absolute).collect();
    let priors = Box::leak(Box::new(fixture_priors(recent, frequent)));
    assert!(!priors.is_empty());
    println!("Q3_PRIORS files={} folders={} requested_direct_folders={FREQUENT_FOLDERS}", fixture_counts(priors).0, fixture_counts(priors).1);
    *session.index.lock().unwrap() = index;
    session.priors = Some(priors);
    let mut gen = 0;
    for batch in 0..BATCHES {
        let mut samples: [Vec<(Phases, f64, usize, usize, u64)>; TARGETS.len()] = std::array::from_fn(|_| Vec::new());
        for _ in 0..SAMPLES {
            for chain in ["render_fra", "archive/2"] {
                session.last_query.clear();
                session.last_matches.clear();
                session.pending_rest = None;
                for end in 1..=chain.len() {
                    let text = &chain[..end];
                    gen += 1;
                    let started = Instant::now();
                    let answer = session.query(gen, text, FIRST_WINDOW_DEFAULT).unwrap();
                    let elapsed = started.elapsed().as_secs_f64() * MS_PER_SECOND;
                    if let Some(at) = TARGETS.iter().position(|target| *target == text) {
                        let index = session.index.lock().unwrap();
                        let mut hash = FNV_OFFSET;
                        for (path, kind) in &answer.rows {
                            for byte in path.as_bytes().iter().copied().chain([u8::from(*kind)]) {
                                hash = (hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME);
                            }
                        }
                        assert_eq!(answer.rows.len(), answer.total.min(FIRST_WINDOW_DEFAULT));
                        assert_eq!(index.len(), PROBE_ROWS);
                        samples[at].push((session.counters.phases, elapsed, answer.total, session.counters.rows_scored, hash));
                    }
                }
            }
        }
        for (at, text) in TARGETS.iter().enumerate() {
            let values = &samples[at];
            assert_eq!(values.len(), SAMPLES);
            assert!(values.iter().all(|value| value.4 == values[0].4));
            let time = |field: fn(&Phases) -> std::time::Duration| median(values.iter().map(|value| field(&value.0).as_secs_f64() * MS_PER_SECOND).collect());
            println!("Q3_PHASE batch={batch} query={text:?} hits={} candidates={} score_ms={:.6} prior_ms={:.6} resolve_ms={:.6} all_ms={:.6} select_ms={:.6} cut_ms={:.6} rows_ms={:.6} last_matches_ms={:.6} query_ms={:.6} prior_share_pct={:.3} select_raw={} head={:016x}",
                values[0].2, values[0].3, time(|p| p.score), time(|p| p.prior), time(|p| p.resolve), time(|p| p.all), time(|p| p.select), time(|p| p.cut), time(|p| p.rows), time(|p| p.last_matches),
                median(values.iter().map(|value| value.1).collect()), median(values.iter().map(|value| value.0.prior.as_secs_f64() * MS_PER_SECOND / value.1 * PERCENT).collect()),
                values.iter().map(|value| value.0.select_raw).max().unwrap(), values[0].4);
        }
    }
}
