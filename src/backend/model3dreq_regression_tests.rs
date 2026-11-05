use super::*;
use crate::backend::{model3d::testdata, testdir::TestDir};

fn file(root: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = root.join(name);
    assert!(path.is_absolute() && path.starts_with(root) && !path.components().any(|p| p == std::path::Component::ParentDir));
    std::fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn cache_hit_is_available_while_thumbnail_parser_busy() {
    let dir = TestDir::new("model3d-cache-hit");
    let root = dir.path();
    let input = file(root, "part.obj", testdata::OBJ);
    let model = model3d::parse("obj", testdata::OBJ).unwrap();
    let mut bytes = Vec::new();
    model3d::wire::write(&model, &mut bytes).unwrap();
    let key = cache::key(&input, &std::fs::metadata(&input).unwrap());
    file(root, &format!("{key}.3d"), &bytes);
    let ticket = codereq::begin("model3d:look");
    let result = {
        let held_ticket = codereq::begin("model3d-regression-held");
        let _held = PARSING.enter(&held_ticket,true,Instant::now()+Duration::from_secs(1)).unwrap();
        let result = ensure_until(&input, root, None, &ticket, Instant::now() + Duration::from_millis(30),false);
        drop(_held);
        held_ticket.discard();
        result
    };
    ticket.discard();
    assert!(result.is_ok(), "cached foreground preview: {:?}", result.err());
}

#[test]
fn sweep_work_name_and_young_dead_control() {
    let dir = TestDir::new("model3d-sweep");
    let root = dir.path();
    let work = Work::new(root, "model3d").unwrap();
    let prefix = format!(".flea-work-model3d-{}-", std::process::id());
    assert!(work.dir.file_name().unwrap().to_string_lossy().starts_with(&prefix));
    let young = dir.join(".flea-work-model3d-4294967295-0");
    dir.assert_contains(&young);
    std::fs::create_dir(&young).unwrap();
    cache::evict(root);
    assert!(young.is_dir());
    dir.assert_contains(&work.dir);
    drop(work);
}

/// Verify that an embedded PNG passes through the image decoder before publication.
#[test]
fn embedded_png_requires_decoder_before_publication() {
    let dir = TestDir::new("model3d-embedded-png");
    let root = dir.path();
    let cache_root = dir.dir("flea-3d");
    let input = file(root, "kit.3mf", b"cached model input");
    let png = testdata::PNG.to_vec();
    assert!(cache::png(&png));
    let mut model = model3d::parse("obj", testdata::OBJ).unwrap();
    model.thumbnails.push(model3d::Thumbnail { plate: None, format: "png".into(), bytes: png });
    let mut bytes = Vec::new();
    model3d::wire::write(&model, &mut bytes).unwrap();
    let key = cache::key(&input, &std::fs::metadata(&input).unwrap());
    file(&cache_root, &format!("{key}.3d"), &bytes);
    let aliases = std::sync::Arc::new(crate::backend::aliases::Aliases::from_str(""));
    let specs = std::sync::Arc::new(crate::backend::thumbspec::Thumbnailers::from_entries(&[], &aliases));
    let tables = thumbs::Tables { aliases, specs, cache: thumbcache::Cache::at(root.to_path_buf()), worker: None };
    let job = thumbs::Job { replies: None, path: input, mtime: 7, source_size: None, mime: "model/x-flea-3d".into(), trash_identity: None, trace: None };
    assert!(matches!(thumbnail(&tables, &job), thumbs::Outcome::Failed), "an embedded PNG must require a sandboxed decoder");
    // The cache hit extracted the picture and recorded no failure, so the refusal is the missing decoder's.
    assert_eq!(std::fs::read(cache_root.join(format!("{key}-all.png"))).ok().as_deref(), Some(&testdata::PNG[..]));
    assert!(matches!(tables.cache.lookup(&job.path,7,THUMB_TABLE),thumbcache::Hit::Miss));
    assert!(!tables.cache.large_path_for(&thumbcache::uri_for(&job.path),None).exists());
}

/// Verify that missing embedded data is cached under the model provider version.
#[test]
fn a_model_without_an_embedded_image_records_a_versioned_failure() {
    let dir = TestDir::new("model3d-no-image");
    let root = dir.path();
    let cache_root = dir.dir("flea-3d");
    let input = file(root,"empty.3mf",b"cached model input");
    let model = model3d::parse("obj",testdata::OBJ).unwrap();
    let mut bytes = Vec::new();
    model3d::wire::write(&model,&mut bytes).unwrap();
    let key = cache::key(&input,&std::fs::metadata(&input).unwrap());
    file(&cache_root,&format!("{key}.3d"),&bytes);
    let aliases = std::sync::Arc::new(crate::backend::aliases::Aliases::from_str(""));
    let specs = std::sync::Arc::new(crate::backend::thumbspec::Thumbnailers::from_entries(&[],&aliases));
    let tables = thumbs::Tables { aliases, specs, cache: thumbcache::Cache::at(root.to_path_buf()), worker: None };
    let job = thumbs::Job { replies: None, path: input, mtime: 7, source_size: None, mime: "model/x-flea-3d".into(), trash_identity: None, trace: None };
    assert!(matches!(thumbnail(&tables,&job),thumbs::Outcome::Failed));
    assert!(matches!(tables.cache.lookup(&job.path,7,THUMB_TABLE),thumbcache::Hit::Failed));
    assert!(matches!(tables.cache.lookup(&job.path,7,"flea-3d-v1"),thumbcache::Hit::Miss));
    assert!(matches!(tables.cache.lookup(&job.path,8,THUMB_TABLE),thumbcache::Hit::Miss));
}
