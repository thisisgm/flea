use super::*;
use crate::backend::testdir::TestDir;
use std::fs::File;
use std::os::unix::fs::PermissionsExt;

const THEME_A: &str = "#101315|#c0caf5|#7aa2f7|||||monospace|14|7|0|0";
const THEME_B: &str = "#ffffff|#101315|#7aa2f7|||||monospace|14|7|0|0";
const ADVANCES: &str = "#101315|#c0caf5|#7aa2f7|||||monospace|14|7|1f2e3d4c|5a6b7c8d";
const SVG: &str = "<svg width=\"10\"><text>caf\u{e9} \u{1d11e}</text></svg>";
const LARGE: usize = 1 << 20;

#[path = "figurestore_gap64_tests.rs"]
mod gap64;

fn key(kind: &str, theme: &str, display: bool, source: &str) -> String {
    format!("{kind}\n{theme}\n{display}\n{source}")
}

fn math(source: &str) -> String {
    key("math", THEME_A, true, source)
}

fn stamp(dir: &Path, key: &str, seconds: u64) {
    let file = File::options().write(true).open(dir.join(entry_name(key).expect("a name"))).expect("entry");
    file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)).expect("mtime");
}

fn entries_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir).expect("dir").flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names
}

#[test]
fn a_put_figure_comes_back_byte_for_byte() {
    let dir = TestDir::new("figure-store-roundtrip");
    let store = dir.join("svg");
    for (kind, source) in [("math", "\\frac{a}{b}"), ("mermaid", "flowchart TD\n    A[\"q\\\"uote\"] --> B\n"), ("math", "caf\u{e9} \u{1d11e} \\\\ \t")] {
        let k = key(kind, THEME_A, true, source);
        assert_eq!(get(&store, &k), None, "nothing before the put");
        put(&store, &k, SVG, MAX_ENTRIES, MAX_BYTES).expect("put");
        assert_eq!(get(&store, &k).as_deref(), Some(SVG));
    }
    assert!(entries_in(&store).iter().all(|n| is_entry(n)), "the store holds entries and nothing else");
    assert_eq!(fs::metadata(&store).expect("dir").permissions().mode() & 0o777, 0o700);
}

#[test]
fn a_different_theme_advance_or_display_never_serves_another_entry() {
    let dir = TestDir::new("figure-store-keys");
    let store = dir.join("svg");
    let source = "flowchart TD\n    A --> B";
    let stored = key("mermaid", ADVANCES, false, source);
    put(&store, &stored, SVG, MAX_ENTRIES, MAX_BYTES).expect("put");
    let others = [key("mermaid", THEME_B, false, source), key("mermaid", THEME_A, false, source), key("mermaid", ADVANCES, true, source),
        key("mermaid", &ADVANCES.replace("5a6b7c8d", "5a6b7c8e"), false, source), key("math", ADVANCES, false, source)];
    for other in &others {
        assert_eq!(get(&store, other), None, "{other:?}");
    }
    assert_eq!(get(&store, &stored).as_deref(), Some(SVG));
}

#[test]
fn an_entry_filed_under_another_keys_name_is_refused() {
    let dir = TestDir::new("figure-store-collision");
    let store = dir.join("svg");
    let (a, b) = (math("x^2"), key("math", THEME_B, true, "x^2"));
    put(&store, &b, "<svg>B</svg>", MAX_ENTRIES, MAX_BYTES).expect("put b");
    fs::copy(store.join(entry_name(&b).expect("b")), store.join(entry_name(&a).expect("a"))).expect("collide");
    assert_eq!(get(&store, &a), None, "a name that matches while the key inside does not is a miss");
    assert_eq!(get(&store, &b).as_deref(), Some("<svg>B</svg>"));
}

#[test]
fn an_unsupported_entry_stays_unchanged_on_the_next_put() {
    let dir = TestDir::new("figure-store-corrupt");
    let store = dir.join("svg");
    let k = math("x^2");
    let good = encode(&k, SVG);
    let name = entry_name(&k).expect("name");
    let target = store.join(&name);
    let mut flipped = good.clone();
    let at = flipped.len() - 3;
    flipped[at] ^= 0x40;
    let mut longer = good.clone();
    longer.push(b'x');
    // Only the format line's version digit changes, so the key and both lengths still match and only the magic can refuse it.
    let mut foreign_magic = good.clone();
    foreign_magic[FORMAT.len() - 1] = b'9';
    let wrong_length = String::from_utf8(good.clone()).expect("text").replacen(&format!("{} {}", k.len(), SVG.len()), &format!("{} {}", k.len(), SVG.len() + 1), 1).into_bytes();
    let cases: Vec<(&str, Vec<u8>)> = vec![("an empty file", Vec::new()), ("a truncated file", good[..good.len() - 1].to_vec()), ("a trailing byte", longer),
        ("a flipped byte", flipped), ("a foreign magic", foreign_magic),
        ("a foreign object", b"{\"foreign\":true}".to_vec()),
        ("a wrong svg length", wrong_length), ("binary noise", vec![0xff; 300])];
    for (label, bytes) in cases {
        fs::create_dir_all(&store).expect("store");
        fs::write(&target, &bytes).expect("damage");
        let refused = get(&store, &k);
        // A flipped byte inside the svg is the one damage the key and lengths cannot see, and it is only a different figure of the same key.
        if label != "a flipped byte" {
            assert_eq!(refused, None, "{label}");
        }
        // Empty bytes, or bytes under this writer's own format line, are its own write torn or damaged, which 0.3.12 replaced.
        let torn = bytes.is_empty() || bytes.starts_with(format!("{FORMAT}\n").as_bytes());
        if label == "a flipped byte" || torn {
            assert_eq!(torn, ["an empty file", "a truncated file", "a trailing byte", "a flipped byte", "a wrong svg length"].contains(&label), "{label}");
            put(&store, &k, SVG, MAX_ENTRIES, MAX_BYTES).expect("understood wrapper");
            assert_eq!(get(&store, &k).as_deref(), Some(SVG));
        } else {
            assert_eq!(put(&store, &k, SVG, MAX_ENTRIES, MAX_BYTES), None);
            evict(&store, 0, 0);
            assert_eq!(fs::read(&target).expect("preserved document"), bytes, "{label}");
        }
    }
    fs::remove_file(&target).expect("unlink");
    std::os::unix::fs::symlink(dir.file("elsewhere", &String::from_utf8(good).expect("text")), &target).expect("link");
    assert_eq!(get(&store, &k), None, "a linked entry is never followed");
    assert_eq!(put(&store, &k, SVG, MAX_ENTRIES, MAX_BYTES), None);
    assert!(fs::symlink_metadata(&target).expect("entry").file_type().is_symlink());
    assert_eq!(fs::read_to_string(dir.join("elsewhere")).expect("target").len(), encode(&k, SVG).len(), "the link's target is untouched");
}

#[test]
fn an_entry_the_previous_bundle_drew_is_a_miss() {
    let dir = TestDir::new("figure-store-previous");
    let store = dir.join("svg");
    let k = key("mermaid", THEME_A, false, "flowchart TD\n    A --> B");
    put(&store, &k, SVG, MAX_ENTRIES, MAX_BYTES).expect("put");
    let target = store.join(entry_name(&k).expect("name"));
    // The same key and lengths under the format the beautiful-mermaid figures were stored in.
    let previous = String::from_utf8(encode(&k, SVG)).expect("text").replacen(FORMAT, "flea-figure-svg 1", 1);
    assert!(previous.starts_with("flea-figure-svg 1\n"), "the entry reads as the previous format");
    fs::write(&target, previous).expect("an entry from the previous bundle");
    assert_eq!(get(&store, &k), None, "a figure the previous bundle drew is never served");
    put(&store, &k, SVG, MAX_ENTRIES, MAX_BYTES).expect("redraw");
    assert_eq!(get(&store, &k).as_deref(), Some(SVG), "the redrawn figure replaces it");
}

#[test]
fn the_oldest_access_goes_first_when_the_count_is_over() {
    let dir = TestDir::new("figure-store-lru-count");
    let store = dir.join("svg");
    let keys: Vec<String> = (0..4).map(|i| math(&format!("x^{i}"))).collect();
    for (i, k) in keys.iter().enumerate() {
        put(&store, k, SVG, MAX_ENTRIES, MAX_BYTES).expect("put");
        stamp(&store, k, 1000 + i as u64);
    }
    // A hit is an access: the oldest entry, read now, outlives the one written after it.
    assert!(get(&store, &keys[0]).is_some());
    let newest = math("x^9");
    put(&store, &newest, SVG, 4, MAX_BYTES).expect("put over the bound");
    assert!(get(&store, &keys[0]).is_some(), "the entry read last survives");
    assert_eq!(get(&store, &keys[1]), None, "the least recently used entry goes");
    assert!(get(&store, &keys[2]).is_some() && get(&store, &keys[3]).is_some() && get(&store, &newest).is_some());
    assert_eq!(entries_in(&store).len(), 4);
}

#[test]
fn the_byte_bound_holds_too() {
    let dir = TestDir::new("figure-store-lru-bytes");
    let store = dir.join("svg");
    let svg = "s".repeat(LARGE);
    let keys: Vec<String> = (0..3).map(|i| math(&format!("big{i}"))).collect();
    for (i, k) in keys.iter().enumerate() {
        put(&store, k, &svg, MAX_ENTRIES, MAX_BYTES).expect("put");
        stamp(&store, k, 2000 + i as u64);
    }
    evict(&store, MAX_ENTRIES, 2 * LARGE as u64 + 4096);
    assert_eq!((get(&store, &keys[0]), get(&store, &keys[1]).is_some(), get(&store, &keys[2]).is_some()), (None, true, true));
    let over = "s".repeat(MAX_ENTRY_BYTES as usize + 1);
    assert_eq!(put(&store, &math("huge"), &over, MAX_ENTRIES, MAX_BYTES), None, "a figure over the entry limit is not kept");
}

#[test]
fn stale_scratch_is_swept_and_strangers_are_left_alone() {
    let dir = TestDir::new("figure-store-scratch");
    let store = dir.join("svg");
    let k = math("x^2");
    put(&store, &k, SVG, MAX_ENTRIES, MAX_BYTES).expect("put");
    let name = entry_name(&k).expect("name");
    let stale = store.join(format!("{name}{SCRATCH_MARK}111"));
    let fresh = store.join(format!("{name}{SCRATCH_MARK}222"));
    for path in [&stale, &fresh] {
        fs::write(path, "half").expect("scratch");
    }
    File::options().write(true).open(&stale).expect("stale").set_modified(SystemTime::UNIX_EPOCH).expect("old");
    let stranger = store.join("notes.txt");
    fs::write(&stranger, "mine").expect("stranger");
    evict(&store, 0, 0);
    assert!(!stale.exists() && fresh.exists() && stranger.exists(), "only stale scratch and entries are touched");
    assert_eq!(get(&store, &k), None, "a zero bound empties the entries");
}

#[test]
fn known_asks_whether_this_exact_key_was_drawn() {
    let dir = TestDir::new("figure-store-known");
    let store = dir.join("svg");
    let flow = "flowchart TD\n    A --> B";
    put(&store, &key("math", THEME_A, true, "x^2"), SVG, MAX_ENTRIES, MAX_BYTES).expect("put");
    put(&store, &key("mermaid", THEME_B, false, flow), SVG, MAX_ENTRIES, MAX_BYTES).expect("put");
    assert!(known(&store, &[key("math", THEME_A, true, "x^2"), key("mermaid", THEME_B, false, flow)]));
    assert!(!known(&store, &[key("math", THEME_B, true, "x^2")]), "a figure drawn under another theme is not known");
    assert!(!known(&store, &[key("math", THEME_A, false, "x^2")]), "nor under another display mode");
    assert!(!known(&store, &[key("math", ADVANCES, true, "x^2")]), "nor under another advance table");
    assert!(!known(&store, &[key("math", THEME_A, true, "x^2"), key("math", THEME_A, true, "y^2")]), "one unknown figure is enough to warm");
    assert!(!known(&store, &[key("mermaid", THEME_A, true, "x^2")]), "the kind is part of the key");
    assert!(!known(&store, &[]) && !known(&dir.join("absent"), &[key("math", THEME_A, true, "x^2")]));
    let linked = key("math", THEME_A, true, "z^2");
    std::os::unix::fs::symlink(dir.file("elsewhere", "x"), store.join(entry_name(&linked).expect("name"))).expect("link");
    assert!(!known(&store, &[linked]), "a linked entry is not known");
    // An entry the previous bundle drew is a miss for get, so it is not known either, and the first open after an upgrade warms the helper.
    let previous = key("mermaid", THEME_A, true, flow);
    put(&store, &previous, SVG, MAX_ENTRIES, MAX_BYTES).expect("put");
    let target = store.join(entry_name(&previous).expect("name"));
    assert!(known(&store, std::slice::from_ref(&previous)), "an entry of this format is known");
    fs::write(&target, String::from_utf8(encode(&previous, SVG)).expect("text").replacen(FORMAT, "flea-figure-svg 1", 1)).expect("format 1");
    assert!(!known(&store, &[previous]), "an entry of the previous format is not known");
    let short = key("math", THEME_A, true, "w^2");
    fs::write(store.join(entry_name(&short).expect("name")), "flea-figure-svg").expect("short");
    assert!(!known(&store, &[short]), "a file shorter than the format line is not known");
}

#[test]
fn an_entry_drawn_before_the_v0312_geometry_is_a_miss() {
    // Format 2 holds Pele's solid lifelines and 64 px participants (SW-19); format 3 its round boxes at least 48 wide and late first message (PELE-GEOM).
    for earlier in ["flea-figure-svg 2", "flea-figure-svg 3"] {
        let dir = TestDir::new("figure-store-earlier-format");
        let store = dir.join("svg");
        let k = key("mermaid", THEME_A, false, "sequenceDiagram\n    A->>B: hi");
        put(&store, &k, SVG, MAX_ENTRIES, MAX_BYTES).expect("put");
        let target = store.join(entry_name(&k).expect("name"));
        fs::write(&target, String::from_utf8(encode(&k, SVG)).expect("text").replacen(FORMAT, earlier, 1)).expect("an earlier entry");
        assert_eq!(get(&store, &k), None, "a {earlier} figure is never served");
        assert!(!known(&store, std::slice::from_ref(&k)), "a {earlier} figure is not known");
        assert!(recognized(&target), "a {earlier} figure is still ours, so eviction may remove it");
        put(&store, &k, SVG, MAX_ENTRIES, MAX_BYTES).expect("redraw");
        assert_eq!(get(&store, &k).as_deref(), Some(SVG), "the redrawn figure replaces it");
    }
}

#[test]
fn a_linked_svg_directory_is_not_used() {
    let dir = TestDir::new("figure-store-linkdir");
    let real = dir.join("real");
    fs::create_dir(&real).expect("real dir");
    let link = dir.join("svg");
    std::os::unix::fs::symlink(&real, &link).expect("link at the store's path");
    assert_eq!(safe_dir(link), None, "a link at the store directory is never followed");
    assert_eq!(safe_dir(real.clone()), Some(real), "a real directory is used");
    assert_eq!(safe_dir(dir.join("absent")), Some(dir.join("absent")), "one not made yet is used and made by the first put");
}

#[test]
fn the_wire_answers_get_put_and_known_and_ignores_the_rest() {
    let dir = TestDir::new("figure-store-wire");
    let store = dir.join("svg");
    let k = key("mermaid", THEME_A, false, "flowchart TD\n    A[\"q\"] --> B");
    let line = |op: &str, extra: &str| format!("{{\"op\":\"{op}\",{extra}}}");
    let wire_key = json::escape(&k);
    let get_line = line("get", &format!("\"id\":7,\"key\":\"{wire_key}\""));
    assert_eq!(answer(Some(&store), &get_line).as_deref(), Some("{\"id\":7,\"miss\":true}"));
    assert_eq!(answer(Some(&store), &line("put", &format!("\"key\":\"{wire_key}\",\"svg\":\"{}\"", json::escape(SVG)))), None, "a put has no reply");
    assert_eq!(answer(Some(&store), &get_line), Some(format!("{{\"id\":7,\"svg\":\"{}\"}}", json::escape(SVG))));
    let figures = json::escape(&k);
    assert_eq!(answer(Some(&store), &line("known", &format!("\"id\":8,\"keys\":[\"{figures}\"]"))).as_deref(), Some("{\"id\":8,\"known\":true}"));
    assert_eq!(answer(Some(&store), &line("known", "\"id\":9,\"keys\":[\"math\\nnope\"]")).as_deref(), Some("{\"id\":9,\"known\":false}"));
    for ignored in ["", "not json", "{\"op\":\"drop\",\"id\":1}", "{\"id\":1}", "{\"op\":\"get\"}"] {
        assert_eq!(answer(Some(&store), ignored), None, "{ignored:?}");
    }
    // With the cache off a get is a miss, a put does nothing and nothing is known.
    assert_eq!(answer(None, &get_line).as_deref(), Some("{\"id\":7,\"miss\":true}"));
    assert_eq!(answer(None, &line("known", &format!("\"id\":8,\"keys\":[\"{figures}\"]"))).as_deref(), Some("{\"id\":8,\"known\":false}"));
}

#[test]
fn an_empty_or_cut_short_entry_is_replaced_and_evicted() {
    let dir = TestDir::new("figure-store-torn");
    let store = dir.join("svg");
    let k = math("x^3");
    put(&store, &k, SVG, MAX_ENTRIES, MAX_BYTES).expect("put");
    let entry = store.join(entry_name(&k).expect("name"));
    let whole = fs::read(&entry).expect("entry");
    for torn in [&b""[..], &whole[..5], &whole[..whole.len() - 4]] {
        fs::write(&entry, torn).expect("torn");
        assert_eq!(get(&store, &k), None, "a torn entry is a miss");
        put(&store, &k, SVG, MAX_ENTRIES, MAX_BYTES).expect("the next put replaces it");
        assert_eq!(get(&store, &k).as_deref(), Some(SVG));
        fs::write(&entry, torn).expect("torn again");
        evict(&store, 0, 0);
        assert!(!entry.exists(), "and the bound evicts it");
    }
}
