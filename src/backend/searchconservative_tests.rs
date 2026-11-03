use super::*;
use crate::backend::testdir::TestDir;

fn session(files: &[&str]) -> (TestDir, Session) {
    let root = TestDir::new("searchconservative");
    for name in files {
        if let Some(parent) = Path::new(name).parent() {
            if !parent.as_os_str().is_empty() { root.dir(parent.to_str().unwrap()); }
        }
        root.file(name, "");
    }
    let path = root.path().to_str().unwrap();
    let search = Session::open_sync(path, path, false);
    (root, search)
}

fn names(search: &mut Session, gen: usize, query: &str) -> Vec<String> {
    search.query(gen, query, 100).unwrap().rows.into_iter().map(|row| row.0).collect()
}

#[test]
fn search_repairs_all_four_letter_edits_and_wrong_first_letter() {
    let (_root, mut search) = session(&["render_frame.rs"]);
    for (gen, query) in ["redner_frame.rs", "rander_frame.rs", "rendxer_frame.rs", "rener_frame.rs", "xender_frame.rs"].iter().enumerate() {
        assert_eq!(names(&mut search, gen + 1, query), ["render_frame.rs"], "{query}");
    }
}

#[test]
fn search_edit_threshold_growth_and_backspace_restore_new_candidates() {
    let (_root, mut search) = session(&["abxd", "abxxefgh"]);
    assert!(!names(&mut search, 1, "abc").contains(&"abxd".to_string()));
    assert!(names(&mut search, 2, "abcd").contains(&"abxd".to_string()));
    assert!(!names(&mut search, 3, "abcdefg").contains(&"abxxefgh".to_string()));
    assert!(names(&mut search, 4, "abcdefgh").contains(&"abxxefgh".to_string()));
    assert_eq!(names(&mut search, 5, "ab"), ["abxd", "abxxefgh"]);
}

#[test]
fn search_exact_prefix_path_and_protected_numeric_controls() {
    let (_root, mut search) = session(&["render", "render_more", "rander", "render-parent/notes.txt", "downloads/helper.txt", "render12", "render13", "icon.svg", "icon.sxg"]);
    let ranked = names(&mut search, 1, "render");
    assert_eq!(ranked[0], "render");
    assert!(ranked.contains(&"render_more".to_string()) && !ranked.contains(&"rander".to_string()), "no typo row beside real hits");
    assert!(ranked.iter().position(|p| p == "render-parent/notes.txt").unwrap() > 0);
    assert!(names(&mut search, 2, "dwnhelp").contains(&"downloads/helper.txt".to_string()));
    let numbered = names(&mut search, 3, "render12");
    assert!(numbered.contains(&"render12".to_string()));
    assert!(!numbered.contains(&"render13".to_string()));
    assert_eq!(names(&mut search, 4, ".svg"), ["icon.svg"]);
    assert!(search.query(5, "downloads", 100).unwrap().rows.contains(&("downloads".to_string(), true)));
    assert!(names(&mut search, 6, "rnader").contains(&"rander".to_string()), "a typo row shows where no basename holds the query");
}
