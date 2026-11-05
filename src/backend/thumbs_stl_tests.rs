use super::*;
use crate::backend::testdir::TestDir;
use crate::backend::thumbspec::Spec;

/// A declined native render leaves the system thumbnailer free to publish its PNG.
#[test]
fn a_declined_native_stl_publishes_the_system_thumbnail() {
    if !sandbox::available() {
        return;
    }
    let dir = TestDir::new("stl-fallback");
    let png = dir.join("fixture.png");
    write_marker(&png, "file:///fixture", 1, None).unwrap();
    let octal: String = std::fs::read(png)
        .unwrap()
        .iter()
        .map(|byte| format!("\\{byte:03o}"))
        .collect();
    let native = dir.script("native-stl", "#!/bin/sh\nprintf partial > \"$2\"\nexit 1\n");
    let system = dir.script(
        "system-stl",
        &format!("#!/bin/sh\n[ ! -s \"$2\" ] || exit 1\nprintf '{octal}' > \"$2\"\n"),
    );
    let entry = format!(
        "[Thumbnailer Entry]\nExec={} %i %o\nMimeType=model/stl;\n",
        system.display()
    );
    let aliases = Arc::new(Aliases::from_str(""));
    let specs = Thumbnailers::from_layers(
        &[],
        &[("system.thumbnailer".into(), entry)],
        &aliases,
        Some(Spec {
            exec: vec![
                native.to_string_lossy().into_owned(),
                "%i".into(),
                "%o".into(),
            ],
        }),
    );
    let tables = Tables {
        aliases,
        specs: Arc::new(specs),
        cache: Cache::at(dir.join("cache")),
        worker: None,
    };
    let path = dir.file("model.stl", "a native decoder decline");
    let uri = uri_for(&path);
    let mut job = Job {
        replies: None,
        path,
        mtime: 2,
        trash_identity: None,
        mime: "model/stl".into(),
        trace: None,
    };
    let Outcome::Ready(file) = run_one_in(&tables, &mut job, &dir.join("raw")) else {
        panic!("the system thumbnailer did not publish after the native decline");
    };
    assert_eq!(file, tables.cache.large_path(&uri));
    let bytes = std::fs::read(file).unwrap();
    assert_eq!(
        crate::backend::thumbcache::png_text(&bytes, "Thumb::URI"),
        Some(uri)
    );
    assert_eq!(
        crate::backend::thumbcache::png_text(&bytes, "Thumb::MTime").as_deref(),
        Some("2")
    );
}

/// An expired fallback never starts a decoder or truncates its output.
#[test]
fn an_expired_stl_fallback_leaves_the_output_alone() {
    let dir = TestDir::new("stl-fallback-expired");
    let input = dir.file("model.stl", "");
    let output = dir.file("temp.png", "partial");
    let spec = Spec {
        exec: vec!["/bin/false".into()],
    };
    assert!(matches!(
        retry_stl(&spec, &input, &output, Instant::now()),
        Ran::TimedOut
    ));
    assert_eq!(std::fs::read_to_string(output).unwrap(), "partial");
}
