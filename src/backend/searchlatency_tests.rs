use super::*;

#[test]
#[ignore]
fn search_match_scale_probe() {
    for count in [100_000, 1_000_000] {
        let (index, _) = super::fixture_tests::synthetic(count, false);
        let rows: Vec<_> = (0..index.len()).collect();
        for query in ["render_frame.rs", "redner_frame.rs", "editor/src/components/render", ".svg", "archive/2024/client/reports/quarterly"] {
            let matcher = crate::backend::searchmatch::Matcher::new(query);
            let start = std::time::Instant::now();
            let mut hits = score_all(&matcher, &rows, &index);
            let matches = hits.len();
            let window = matches.min(350);
            let _rest = select_head(&mut hits, window, |hit| index.path_len(hit.1), |a, b| rank_cmp(&index, a, b));
            println!("SEARCH_MATCH_PROBE entries={count} query={query:?} matches={matches} core_us={:.3}", start.elapsed().as_secs_f64() * 1e6);
        }
    }
}

// FS-2 query root-cause probe: per typed prefix, full rescan against narrowing from the previous prefix's matches.
#[test]
#[ignore]
fn search_fs2_query_phase_probe() {
    let med = |mut v: Vec<f64>| { v.sort_by(|a, b| a.partial_cmp(b).unwrap()); v[v.len() / 2] };
    for count in [100_000, 1_000_000] {
        let (index, _) = super::fixture_tests::synthetic(count, false);
        let rows: Vec<_> = (0..index.len()).collect();
        let (mut full_scratch, mut narrow_scratch) = (QueryScratch::default(), QueryScratch::default());
        for query in ["render_frame.rs", "redner_frame.rs", "editor/src/components/render", ".svg", "archive/2024/client/reports/quarterly"] {
            let mut previous: Option<Vec<usize>> = None;
            for end in 1..=query.len() {
                let text = &query[..end];
                let matcher = crate::backend::searchmatch::Matcher::new(text);
                let (mut score, mut select, mut clone, mut narrow) = (vec![], vec![], vec![], vec![]);
                let mut full_ids = vec![];
                let mut narrow_ids = vec![];
                for _ in 0..7 {
                    let t = std::time::Instant::now();
                    let mut hits = score_cached(&matcher, &rows, &index, &mut full_scratch);
                    score.push(t.elapsed().as_secs_f64() * 1e3);
                    let t = std::time::Instant::now();
                    let all: Vec<usize> = hits.iter().map(|(_, row)| *row).collect();
                    clone.push(t.elapsed().as_secs_f64() * 1e3);
                    let window = hits.len().min(350);
                    let t = std::time::Instant::now();
                    let _rest = select_head(&mut hits, window, |hit| index.path_len(hit.1), |a, b| rank_cmp(&index, a, b));
                    select.push(t.elapsed().as_secs_f64() * 1e3);
                    full_ids = all;
                    if let Some(prev) = &previous {
                        let t = std::time::Instant::now();
                        let hits = score_cached(&matcher, prev, &index, &mut narrow_scratch);
                        narrow.push(t.elapsed().as_secs_f64() * 1e3);
                        narrow_ids = hits.iter().map(|(_, row)| *row).collect();
                    }
                }
                let (mut a, mut b) = (full_ids.clone(), narrow_ids.clone());
                a.sort_unstable();
                b.sort_unstable();
                let lost = if previous.is_some() { a.iter().filter(|r| b.binary_search(r).is_err()).count() } else { 0 };
                println!("FS2_QPROBE entries={count} query={text:?} can_narrow_now={} hits={} score_ms={:.3} clone_ms={:.3} select_ms={:.3} narrow_score_ms={} narrow_lost={lost}",
                    previous.is_some() && crate::backend::searchmatch::Matcher::can_narrow(&query[..end - 1], text),
                    full_ids.len(), med(score), med(clone), med(select),
                    if narrow.is_empty() { "-".to_string() } else { format!("{:.3}", med(narrow)) });
                previous = Some(full_ids);
            }
        }
    }
}

#[path = "searchnarrow_tests.rs"]
mod narrow_tests;
