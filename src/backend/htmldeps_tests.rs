use super::*;
use crate::backend::testdir::TestDir;
use std::fs::{File, FileTimes};

fn saved(d: &TestDir, resources: &str) -> PathBuf {
    let png = d.join("cached.png");
    let bytes = record(resources, d.path(), SystemTime::now() + Duration::from_secs(1)).unwrap();
    std::fs::write(sidecar(&png), bytes).unwrap();
    png
}

#[test]
fn engine_resources_record_nested_styles_images_fonts_and_encoded_names() {
    let d = TestDir::new("html-dependencies");
    d.dir("sub");
    for name in ["sub/style.css", "image #?.png", "font.woff"] { d.file(name, "data"); }
    let png = saved(&d, r#"{"resources":[{"url":"file:///page/sub/style.css","failed":false},{"url":"file:///page/image%20%23%3F.png","failed":false},{"url":"file:///page/font.woff","failed":false},{"url":"data:image/png;base64,abc","failed":false}],"overflow":false}"#);
    assert!(unchanged(&png, d.path()));
    let document = jsondoc::parse(&std::fs::read_to_string(sidecar(&png)).unwrap()).unwrap();
    assert_eq!(document.get("dependencies").unwrap().as_array().unwrap().len(), 3);
    d.replace_file(&d.join("sub/style.css"), "new!");
    assert!(!unchanged(&png, d.path()));
}

#[test]
fn each_size_nanosecond_mtime_and_inode_change_invalidates_the_record() {
    let d = TestDir::new("html-dependency-stamps");
    let css = d.file("style.css", "old!");
    let stamp = SystemTime::UNIX_EPOCH + Duration::new(9, 20);
    let resources = r#"{"resources":[{"url":"file:///page/style.css","failed":false}],"overflow":false}"#;
    for change in ["size", "seconds", "nanoseconds", "inode"] {
        std::fs::write(&css, "old!").unwrap();
        File::open(&css).unwrap().set_times(FileTimes::new().set_modified(stamp)).unwrap();
        let png = saved(&d, resources);
        assert!(unchanged(&png, d.path()));
        match change {
            "size" => { std::fs::write(&css, "longer").unwrap(); File::open(&css).unwrap().set_times(FileTimes::new().set_modified(stamp)).unwrap(); }
            "seconds" => File::open(&css).unwrap().set_times(FileTimes::new().set_modified(stamp + Duration::from_secs(1))).unwrap(),
            "nanoseconds" => File::open(&css).unwrap().set_times(FileTimes::new().set_modified(stamp + Duration::from_nanos(1))).unwrap(),
            _ => { d.replace_file(&css, "new!"); File::open(&css).unwrap().set_times(FileTimes::new().set_modified(stamp)).unwrap(); }
        }
        assert!(!unchanged(&png, d.path()), "{change} must invalidate");
    }
}

#[test]
fn dependency_records_reject_malformed_input_and_track_denied_paths() {
    let d = TestDir::new("html-dependency-records");
    let png = d.join("cached.png");
    assert!(!unchanged(&png, d.path()));
    for bad in ["null".to_string(), "{}".into(), "x".repeat(MAX_RECORD_BYTES + 1),
        r#"{"version":1,"dependencies":[["../outside",1,2,3,4,5]]}"#.into(),
        r#"{"version":1,"dependencies":[["missing",1,2,3,4,5]]}"#.into(),
        r#"{"version":1,"dependencies":[[]]}"#.into(),
        r#"{"version":1,"dependencies":[["../outside","absent"]]}"#.into()] {
        std::fs::write(sidecar(&png), bad).unwrap();
        assert!(!unchanged(&png, d.path()));
    }
    let outside = TestDir::new("html-dependency-outside");
    let target = outside.file("other.css", "data");
    std::os::unix::fs::symlink(target, d.join("escape.css")).unwrap();
    let png = saved(&d, r#"{"resources":[{"url":"file:///page/escape.css","failed":false}],"overflow":false}"#);
    let denied = jsondoc::parse(&std::fs::read_to_string(sidecar(&png)).unwrap()).unwrap();
    assert_eq!(denied.get("dependencies").and_then(Json::as_array).unwrap()[0],
        Json::Arr(vec![Json::Str("escape.css".into()), Json::Str("blocked".into())]));
    assert!(unchanged(&png, d.path()), "an outside target stays inaccessible to the jail");
    d.replace_file(&d.join("escape.css"), "now local");
    assert!(!unchanged(&png, d.path()), "replacing the escape with local content must invalidate");
}

#[test]
fn a_missing_sheet_is_recorded_absent_and_its_arrival_invalidates() {
    let d = TestDir::new("html-dependency-absent");
    let png = d.join("cached.png");
    let resources = r#"{"resources":[{"url":"file:///page/late.css","failed":true}],"overflow":false}"#;
    let bytes = record(resources, d.path(), SystemTime::now() + Duration::from_secs(1)).unwrap();
    std::fs::write(sidecar(&png), &bytes).unwrap();
    assert!(unchanged(&png, d.path()));
    d.file("late.css", "body{margin:0}");
    assert!(!unchanged(&png, d.path()));
    let png = saved(&d, r#"{"resources":[{"url":"file:///page/folder.css","failed":true}],"overflow":false}"#);
    d.dir("folder.css");
    assert!(!unchanged(&png, d.path()), "any arrival invalidates an absent resource");
}

#[test]
fn overflow_and_edits_during_render_disable_reuse() {
    let d = TestDir::new("html-dependency-bounds");
    let started = SystemTime::UNIX_EPOCH;
    d.file("style.css", "data");
    let resources = r#"{"resources":[{"url":"file:///page/style.css","failed":false}],"overflow":false}"#;
    assert!(record(resources, d.path(), started).is_none());
    assert!(record(r#"{"resources":[],"overflow":true}"#, d.path(), SystemTime::now()).is_none());
    let urls = vec![jsondoc::parse(r#"{"url":"file:///page/style.css","failed":false}"#).unwrap(); MAX_RESOURCES + 1];
    let document = Json::Obj(vec![("resources".into(), Json::Arr(urls)), ("overflow".into(), Json::Bool(false))]);
    assert!(record(&jsondoc::render(&document), d.path(), SystemTime::now()).is_none());
    let rows = vec![Json::Arr(vec![Json::Str("style.css".into()), number(4), number(1), number(0), number(1), number(1)]); MAX_RESOURCES + 1];
    let document = Json::Obj(vec![("version".into(), number(1)), ("dependencies".into(), Json::Arr(rows))]);
    let png = d.join("cached.png");
    std::fs::write(sidecar(&png), jsondoc::render(&document)).unwrap();
    assert!(!unchanged(&png, d.path()));
    assert!(record(&" ".repeat(MAX_RECORD_BYTES + 1), d.path(), SystemTime::now()).is_none());
}

#[test]
fn a_loaded_sheet_deleted_before_record_cannot_be_cached_as_absent() {
    let d = TestDir::new("html-dependency-deleted");
    let css = d.file("loaded.css", "body{color:red}");
    let started = SystemTime::now();
    std::fs::remove_file(css).unwrap();
    let resources = r#"{"resources":[{"url":"file:///page/loaded.css","failed":false}],"overflow":false}"#;
    assert!(record(resources, d.path(), started).is_none(), "loaded pixels outlive the deleted sheet");
    let mixed = r#"{"resources":[{"url":"file:///page/loaded.css","failed":true},{"url":"file:///page/loaded.css","failed":false}],"overflow":false}"#;
    assert!(record(mixed, d.path(), started).is_none(), "a failed duplicate must not hide a loaded resource");
}

#[test]
fn foreign_and_newer_dependencies_survive_read_and_first_publish() {
    let d = crate::backend::testdir::TestDir::new("html-deps-preserve");
    let dest = d.join("existing.png");
    let deps = sidecar(&dest);
    let png = d.join("new.png");
    let record = d.join("new.deps");
    for original in ["{\"foreign\":true}\n", "{\"version\":999,\"dependencies\":[]}"] {
        std::fs::write(&deps, original).unwrap();
        std::fs::write(&png, b"\x89PNG\r\n\x1a\n").unwrap();
        std::fs::write(&record, "{\"version\":1,\"dependencies\":[]}").unwrap();
        assert!(!unchanged(&dest, d.path()));
        assert_eq!(std::fs::read_to_string(&deps).unwrap(), original);
        assert!(crate::backend::htmlcache::publish(&png, &record, &dest).unwrap_err().to_string().contains("left unchanged"));
        assert_eq!(std::fs::read_to_string(&deps).unwrap(), original);
    }
}

#[test]
fn a_damaged_record_that_is_no_json_is_rendered_again() {
    let d = TestDir::new("html-deps-damaged");
    let dest = d.join("existing.png");
    let png = d.join("new.png");
    let record = d.join("new.deps");
    for damaged in [&b"unparsable"[..], b"{\"truncated\":", "\u{fffd}\u{0}".as_bytes(), b"\xff\xfe"] {
        std::fs::write(sidecar(&dest), damaged).unwrap();
        std::fs::write(&png, b"\x89PNG\r\n\x1a\n").unwrap();
        std::fs::write(&record, "{\"version\":1,\"dependencies\":[]}").unwrap();
        assert!(!unchanged(&dest, d.path()));
        assert!(replaceable(&dest), "{:?} is damage, not another writer's record", String::from_utf8_lossy(damaged));
        crate::backend::htmlcache::publish(&png, &record, &dest).unwrap();
        assert_eq!(std::fs::read_to_string(sidecar(&dest)).unwrap(), "{\"version\":1,\"dependencies\":[]}");
    }
}

#[test]
fn a_record_this_writer_produced_is_replaced_by_the_next_publish() {
    let d = TestDir::new("html-deps-own-record");
    let site = d.dir("site");
    std::fs::write(site.join("style.css"), "body{}").unwrap();
    let outside = TestDir::new("html-deps-own-outside");
    std::os::unix::fs::symlink(outside.file("other.css", "data"), site.join("escape.css")).unwrap();
    let resources = r#"{"resources":[{"url":"file:///page/late.css","failed":true},{"url":"file:///page/escape.css","failed":false},{"url":"file:///page/style.css","failed":false}],"overflow":false}"#;
    let dest = d.join("cached.png");
    for round in ["first", "second"] {
        let bytes = record(resources, &site, SystemTime::now() + Duration::from_secs(1)).unwrap();
        let png = d.join("new.png");
        let deps = d.join("new.deps");
        std::fs::write(&png, b"\x89PNG\r\n\x1a\n").unwrap();
        std::fs::write(&deps, bytes).unwrap();
        crate::backend::htmlcache::publish(&png, &deps, &dest).unwrap_or_else(|e| panic!("{round} publish refused its own record: {e}"));
    }
    let kinds: Vec<Json> = jsondoc::parse(&std::fs::read_to_string(sidecar(&dest)).unwrap()).unwrap()
        .get("dependencies").and_then(Json::as_array).unwrap().iter().map(|row| row.as_array().unwrap()[1].clone()).collect();
    assert_eq!(&kinds[..2], [Json::Str("absent".into()), Json::Str("blocked".into())]);
    assert!(kinds[2].as_f64().is_some(), "the present sheet records its stamp");
}
