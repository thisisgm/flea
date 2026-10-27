// The filter's backend half (issue 315). ui/js/Filter.js narrows the rows the pane holds on the
// keystroke itself; this answers for the whole folder from the names the listing already holds, so a
// match costs no stat and no row crosses the wire that a screen will not draw. ui/js/FilterWire.js
// asks, src/backend/filterreq.rs answers, and the wire is docs/protocol.md "filter".
use crate::backend::listing::Listing;
use crate::backend::timing::since;
use std::time::Instant;

// The matches as [start, length] runs of consecutive listing rows, the shape the filtered line carries.
pub type Runs = Vec<(usize, usize)>;

// The standing query over one numbering: every name lowercased once per numbering, and the rows the
// newest query kept, in listing order, which is what a window and the next keystroke read.
pub struct Standing {
    pub generation: u64,
    lower: Vec<String>,
    query: String,
    pub gen: usize,
    pub matches: Vec<usize>,
}

impl Standing {
    fn over(l: &Listing, generation: u64, gen: usize) -> Standing {
        Standing { generation, lower: (0..l.len()).map(|i| l.name(i).to_lowercase()).collect(), query: String::new(), gen, matches: Vec::new() }
    }
}

// One query's answer: the match count, the matches as runs and the time it took. None when gen is
// not newer than the one answered, the rule searchquery follows, so a burst answers only its newest.
// The test is the one ui/js/Match.js paints with: the name lowercased holds the query lowercased.
pub fn answer(standing: &mut Option<Standing>, l: &Listing, generation: u64, query: &str, gen: usize) -> Option<(usize, Runs, f64)> {
    let t = Instant::now();
    let answered = standing.as_ref().map_or(0, |s| s.gen);
    if gen <= answered {
        return None;
    }
    // A list or a sort renumbered the rows since, so the names are read again; the gen count carries on.
    let stale = standing.as_ref().map_or(true, |s| s.generation != generation || s.lower.len() != l.len());
    if stale {
        *standing = Some(Standing::over(l, generation, answered));
    }
    let s = standing.as_mut().expect("a standing query was just set");
    let q = query.to_lowercase();
    // A query extending the previous one tests only the previous matches, as the search's scorer does.
    let kept: Vec<usize> = if !s.query.is_empty() && q.starts_with(&s.query) {
        s.matches.iter().copied().filter(|&i| s.lower[i].contains(&q)).collect()
    } else {
        (0..s.lower.len()).filter(|&i| s.lower[i].contains(&q)).collect()
    };
    s.matches = kept;
    s.query = q;
    s.gen = gen;
    Some((s.matches.len(), runs(&s.matches), since(t)))
}

// The matches as maximal runs of consecutive rows, each [start, length]: a letter every name shares
// is one pair on the wire and not a hundred thousand numbers, and the client expands them in order.
pub fn runs(matches: &[usize]) -> Runs {
    let mut out: Runs = Vec::new();
    for &i in matches {
        match out.last_mut() {
            Some(last) if last.0 + last.1 == i => last.1 += 1,
            _ => out.push((i, 1)),
        }
    }
    out
}

// The matched rows a view window names, clamped the way window clamps, with the gen they answer and
// the start as clamped, which is what the frows line says. None with no query standing over this
// numbering, so a window that raced a close or a sort is silent.
pub fn window(standing: &Option<Standing>, generation: u64, start: usize, count: usize) -> Option<(Vec<usize>, usize, usize)> {
    let s = standing.as_ref().filter(|s| s.generation == generation)?;
    let end = start.saturating_add(count).min(s.matches.len());
    let start = start.min(end);
    Some((s.matches[start..end].to_vec(), s.gen, start))
}

// The client's cursor as a filter request names it: the listing row it is on, and how many matches
// ahead of it the first window should hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub at: usize,
    pub before: usize,
}

// Where a query's first window starts: before matches ahead of the match at the cursor's row, worked
// out here because the client knows the position of its cursor among the rows it holds and not among
// the folder's matches. With no match at that row the client's cursor takes the first match, so the
// window starts there. 0 with nothing standing.
pub fn start_around(standing: &Option<Standing>, generation: u64, cursor: Cursor) -> usize {
    let Some(s) = standing.as_ref().filter(|s| s.generation == generation) else { return 0 };
    let pos = s.matches.partition_point(|&m| m < cursor.at);
    if s.matches.get(pos) == Some(&cursor.at) { pos.saturating_sub(cursor.before) } else { 0 }
}

// Sample output: {"t":"filtered","gen":3,"n":11,"runs":[[233,1],[2340,10]],"ms":0.412,"listing":1}
pub fn filtered_line(gen: usize, n: usize, runs: &[(usize, usize)], ms: f64, generation: u64) -> String {
    let runs_json = runs.iter().map(|(start, len)| format!("[{},{}]", start, len)).collect::<Vec<_>>().join(",");
    format!(r#"{{"t":"filtered","gen":{},"n":{},"runs":[{}],"ms":{:.3},"listing":{}}}"#, gen, n, runs_json, ms, generation)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Directories first, as the backend orders them: 0 Screens 1 scripts 2 tools 3 notes.md 4 readme.txt 5 screenrecording 6 screenshot
    fn listing() -> Listing {
        let mut l = Listing::new();
        for (name, dir) in [("Screens", true), ("scripts", true), ("tools", true), ("notes.md", false), ("readme.txt", false), ("screenrecording-2026-08-21.mp4", false), ("screenshot-2026-08-30.png", false)] {
            l.push(name, dir);
        }
        l
    }

    #[test]
    fn a_query_matches_case_insensitively_in_listing_order() {
        let l = listing();
        let mut standing = None;
        let (n, runs, _) = answer(&mut standing, &l, 1, "SCR", 1).expect("the first gen answers");
        assert_eq!((n, runs), (4, vec![(0, 2), (5, 2)]), "Screens, scripts, screenrecording and screenshot, directories first");
        let (n, runs, _) = answer(&mut standing, &l, 1, "e", 2).expect("a newer gen answers");
        assert_eq!((n, runs), (5, vec![(0, 1), (3, 4)]), "every name but scripts and tools holds an e");
    }

    #[test]
    fn a_gen_not_newer_than_the_answered_one_is_dropped() {
        let l = listing();
        let mut standing = None;
        assert!(answer(&mut standing, &l, 1, "s", 3).is_some());
        assert!(answer(&mut standing, &l, 1, "sc", 3).is_none(), "the same gen answers nothing");
        assert!(answer(&mut standing, &l, 1, "sc", 2).is_none(), "an older gen answers nothing");
        assert_eq!(standing.as_ref().map(|s| s.matches.len()), Some(6), "the standing matches are gen 3's: every name but readme.txt holds an s");
        assert!(answer(&mut standing, &l, 1, "sc", 4).is_some());
    }

    #[test]
    fn an_extended_query_narrows_the_previous_matches_to_the_same_answer_as_a_fresh_scan() {
        let l = listing();
        let mut narrowed = None;
        answer(&mut narrowed, &l, 1, "s", 1);
        let (n, runs, _) = answer(&mut narrowed, &l, 1, "scr", 2).expect("the extension answers");
        let mut fresh = None;
        let (fn_, fruns, _) = answer(&mut fresh, &l, 1, "scr", 1).expect("the fresh scan answers");
        assert_eq!((n, runs.clone()), (fn_, fruns));
        assert_eq!(runs, vec![(0, 2), (5, 2)]);
        // A query that is not an extension scans the whole listing again.
        let (n, runs, _) = answer(&mut narrowed, &l, 1, "t", 3).expect("a different query answers");
        assert_eq!((n, runs), (5, vec![(1, 4), (6, 1)]), "scripts tools notes.md readme.txt screenshot");
    }

    #[test]
    fn a_new_numbering_rereads_the_names_and_keeps_counting_gens() {
        let mut l = listing();
        let mut standing = None;
        answer(&mut standing, &l, 1, "zzz", 1);
        assert_eq!(standing.as_ref().map(|s| s.matches.len()), Some(0));
        l.push("zzz-top.txt", false);
        let (n, runs, _) = answer(&mut standing, &l, 2, "zzz", 2).expect("the renumbered listing answers");
        assert_eq!((n, runs), (1, vec![(7, 1)]), "the new name is seen under the new numbering");
        assert!(answer(&mut standing, &l, 2, "zzz", 2).is_none(), "the gen count carried across the numbering");
        assert_eq!(standing.as_ref().map(|s| s.generation), Some(2));
    }

    #[test]
    fn a_window_names_the_matched_rows_clamped_and_is_silent_over_another_numbering() {
        let l = listing();
        let mut standing = None;
        answer(&mut standing, &l, 1, "s", 5);
        assert_eq!(window(&standing, 1, 0, 2), Some((vec![0, 1], 5, 0)));
        assert_eq!(window(&standing, 1, 3, 10), Some((vec![3, 5, 6], 5, 3)), "the end clamps to the match count");
        assert_eq!(window(&standing, 1, 9, 10), Some((vec![], 5, 6)), "a start past the end answers no rows, and says where the matches end");
        assert_eq!(window(&standing, 2, 0, 2), None, "a sort since means these indices name other rows");
        assert_eq!(window(&None, 1, 0, 2), None);
    }

    #[test]
    fn the_first_window_starts_before_the_cursors_match_or_at_the_first_match() {
        let l = listing();
        let mut standing = None;
        // Matches of "s": 0 1 2 3 5 6, so row 5 is the fifth match, position 4.
        answer(&mut standing, &l, 1, "s", 1);
        let cursor = |at, before| Cursor { at, before };
        assert_eq!(start_around(&standing, 1, cursor(5, 1)), 3, "one match ahead of the cursor's");
        assert_eq!(start_around(&standing, 1, cursor(5, 10)), 0, "a buffer longer than the matches ahead clamps at 0");
        assert_eq!(start_around(&standing, 1, cursor(4, 1)), 0, "readme.txt holds no s, so the cursor will take the first match");
        assert_eq!(start_around(&standing, 1, cursor(6, 0)), 5, "the last match with no buffer is its own position");
        assert_eq!(start_around(&standing, 2, cursor(5, 1)), 0, "another numbering has nothing standing");
        assert_eq!(start_around(&None, 1, cursor(5, 1)), 0);
    }

    #[test]
    fn runs_join_consecutive_rows_and_the_line_carries_them() {
        assert_eq!(runs(&[]), vec![]);
        assert_eq!(runs(&[0, 1, 2, 4, 7, 8]), vec![(0, 3), (4, 1), (7, 2)]);
        assert_eq!(filtered_line(3, 6, &[(0, 3), (4, 1), (7, 2)], 0.4126, 9),
                   r#"{"t":"filtered","gen":3,"n":6,"runs":[[0,3],[4,1],[7,2]],"ms":0.413,"listing":9}"#);
    }
}
