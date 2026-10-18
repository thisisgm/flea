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
            let _rest = select_head(&mut hits, window, |hit| (std::cmp::Reverse(hit.0), index.path_len(hit.1)), |a, b| rank_cmp(&index, a, b));
            println!("SEARCH_MATCH_PROBE entries={count} query={query:?} matches={matches} core_us={:.3}", start.elapsed().as_secs_f64() * 1e6);
        }
    }
}
