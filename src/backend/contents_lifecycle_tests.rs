use super::*;

const FIRST: usize = 10;
const NAMES: [&str; 3] = ["b.txt", "a.txt", "c.txt"];

fn held() -> Fixture {
    let mut f = Fixture::new();
    std::fs::write(f.root.join("match.txt"), "needle\n").unwrap();
    f.search(false, 1, FIRST);
    f.finish();
    assert_eq!(f.st.contents.held.as_ref().unwrap().matches.len(), 1);
    for name in NAMES {
        std::fs::write(f.root.join(name), name).unwrap();
    }
    f.out.clear();
    f
}

fn listed(f: &Fixture) -> super::super::super::iomount::ListOut {
    let mut listing = Listing::new();
    for name in NAMES {
        listing.push(name, false);
    }
    let (first_metas, first_ms) =
        super::super::super::meta::stat_range(&f.root, &listing, 0, FIRST);
    super::super::super::iomount::ListOut {
        listing,
        read_ms: 0.,
        sort_ms: 0.,
        sized: Vec::new(),
        dev: 0,
        writable: true,
        first_metas,
        first_start: 0,
        first_ms,
        watch_wd: -1,
        watch_mount: PathBuf::new(),
    }
}

fn directory(f: &mut Fixture) {
    let done = listed(f);
    adopt_listed(
        &mut f.out,
        &mut f.st,
        &f.pool,
        &f.tb,
        &f.root.to_string_lossy(),
        done,
        false,
    );
}

fn empty_query(f: &mut Fixture) {
    let done = listed(f);
    adopt_session(
        &mut f.out,
        &mut f.st,
        &f.pool,
        &f.tb,
        &f.root.to_string_lossy(),
        done,
        2,
    );
}

fn paths(f: &mut Fixture) {
    let paths: Vec<_> = NAMES
        .iter()
        .map(|name| f.root.join(name).to_string_lossy().into_owned())
        .collect();
    super::super::super::listpaths::answer(
        &mut f.out, &mut f.st, &f.pool, &f.tb, &paths, FIRST, "{}",
    );
}

fn names_query(f: &mut Fixture) {
    let answer = super::super::super::searchreq::Answer {
        gen: 2,
        all: vec![0, 1, 2],
        typos: true,
        rows: NAMES.iter().map(|name| ((*name).into(), false)).collect(),
        total: NAMES.len(),
        indexed: NAMES.len(),
        walking: false,
        partial: None,
        ms: 0.,
        count: FIRST,
        scope: f.root.clone(),
        dev: 0,
        writable: true,
    };
    super::super::super::searchreq::emit_session_answer(
        &mut f.out, &mut f.st, &f.tb, &f.pool, &answer,
    );
}

fn check(publish: fn(&mut Fixture), sort: bool) {
    let mut f = held();
    publish(&mut f);
    assert_eq!(f.st.listing.len(), NAMES.len());
    let mut expected: Vec<_> = (0..f.st.listing.len())
        .map(|row| f.st.listing.name(row).to_owned())
        .collect();
    f.out.clear();
    if sort {
        expected.sort();
        f.request(r#"{"c":"sort","by":"name","desc":false,"count":10}"#);
        assert!(
            f.text().contains(r#""t":"listed""#),
            "sort must publish the new listing: {}",
            f.text()
        );
        f.request(r#"{"c":"window","start":0,"count":10}"#);
    } else {
        f.request(r#"{"c":"window","start":0,"count":10}"#);
    }
    assert!(
        !f.text().contains("contentsmatches"),
        "foreign rows received contents metadata: {}",
        f.text()
    );
    assert!(
        !f.text().contains(r#""t":"error""#),
        "new listing request failed: {}",
        f.text()
    );
    assert!(f.text().contains(r#""t":"rows""#));
    assert_eq!(
        (0..f.st.listing.len())
            .map(|row| f.st.listing.name(row).to_owned())
            .collect::<Vec<_>>(),
        expected
    );
    assert!(f.st.contents.held.is_none());
}

macro_rules! publication {
    ($window:ident, $sort:ident, $publish:ident) => {
        #[test]
        fn $window() {
            if crate::backend::sandboxprobe::skipped() {
                return;
            }
            check($publish, false);
        }
        #[test]
        fn $sort() {
            if crate::backend::sandboxprobe::skipped() {
                return;
            }
            check($publish, true);
        }
    };
}

publication!(
    m9_g2rows_01_lifecycle_directory_window,
    m9_g2rows_01_lifecycle_directory_sort,
    directory
);
publication!(
    m9_g2rows_01_lifecycle_empty_query_window,
    m9_g2rows_01_lifecycle_empty_query_sort,
    empty_query
);
publication!(
    m9_g2rows_01_lifecycle_paths_window,
    m9_g2rows_01_lifecycle_paths_sort,
    paths
);
publication!(
    m9_g2rows_01_lifecycle_names_query_window,
    m9_g2rows_01_lifecycle_names_query_sort,
    names_query
);

#[test]
fn m9_g2rows_01_lifecycle_own_sort_preserves_pending_contents() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let mut f = held();
    std::fs::write(f.root.join("next.txt"), "needle next\n").unwrap();
    f.search(false, 2, FIRST);
    assert!(f.st.contents.busy());
    f.request(r#"{"c":"sort","by":"name","desc":true}"#);
    assert_eq!(f.st.contents.held.as_ref().unwrap().ask.gen, 1);
    assert!(f.st.contents.busy());
    f.out.clear();
    f.finish();
    assert_eq!(f.st.contents.held.as_ref().unwrap().ask.gen, 2);
    assert_eq!(f.st.listing.len(), 2);
    f.out.clear();
    f.request(r#"{"c":"searchclose"}"#);
    assert!(f.text().contains(r#""gen":2"#));
    assert!(f.text().contains("searchclosed"));
}

#[test]
fn m9_g2rows_01_lifecycle_foreign_publication_discards_pending_contents() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let mut f = held();
    f.search(false, 2, FIRST);
    assert!(f.st.contents.busy());
    names_query(&mut f);
    f.out.clear();
    let deadline = Instant::now() + Duration::from_secs(5);
    while f.st.contents.busy() {
        let event =
            f.rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap();
        f.event(event);
    }
    assert!(f.st.contents.held.is_none());
    assert_eq!(f.st.listing.len(), NAMES.len());
    assert!(!f.text().contains("contentsmatches"));
    f.request(r#"{"c":"window","start":0,"count":10}"#);
    assert!(f.text().contains(r#""t":"rows""#));
    assert!(!f.text().contains("contentsmatches"));
}
