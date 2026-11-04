//! A cache generation may change while the document and legacy captured input remain immutable.
use super::*;

#[test]
fn sol_gap64_format_one_cache_misses_then_accepts_a_new_render_without_touching_input() {
    let fixture = include_bytes!("../tests/compat/state-v0312.json");
    for (kind, source) in [("math", "x^2"), ("mermaid", "flowchart TD\n A --> B")] {
        let dir = TestDir::new("gap64-figure-compat");
        let input = dir.file("document-source", source);
        let captured = dir.file("state-v0312.json", std::str::from_utf8(fixture).unwrap());
        let source_bytes = fs::read(&input).unwrap();
        let store = dir.join("svg");
        fs::create_dir_all(&store).unwrap();
        let key = key(kind, THEME_A, false, source);
        let entry = store.join(entry_name(&key).unwrap());
        let legacy = String::from_utf8(encode(&key, SVG))
            .unwrap()
            .replacen(FORMAT, "flea-figure-svg 1", 1)
            .into_bytes();
        fs::write(&entry, &legacy).unwrap();
        assert_eq!(get(&store, &key), None);
        assert_eq!(
            fs::read(&entry).unwrap(),
            legacy,
            "a read does not rewrite the legacy cache"
        );
        let rendered = "<svg xmlns=\"http://www.w3.org/2000/svg\"><text>new render</text></svg>";
        put(&store, &key, rendered, MAX_ENTRIES, MAX_BYTES).unwrap();
        assert!(fs::read(&entry)
            .unwrap()
            .starts_with(b"flea-figure-svg 4\n"));
        assert_eq!(get(&store, &key).as_deref(), Some(rendered));
        assert_eq!(fs::read(&input).unwrap(), source_bytes);
        assert_eq!(fs::read(&captured).unwrap(), fixture.as_slice());
    }
}
