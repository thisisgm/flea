// Real sandboxed processes exercise dispatch, fallback, cleanup and cache publication.
// Scripts stand in for decoder executables; these are not libvips decoding tests.
use super::*;
use crate::backend::testdir::TestDir;
use crate::backend::thumbcache::{png_text, Hit};

struct Fixture {
    dir: TestDir,
    tables: Tables,
    job: Job,
    png: String,
}

impl Fixture {
    fn new() -> Self {
        let dir = TestDir::new("jpeg-orchestration");
        let aliases = Arc::new(Aliases::from_str(""));
        let specs = Thumbnailers::from_entries_with_vips(
            &[("/usr/share/thumbnailers/glycin-image-rs.thumbnailer".into(),
                "[Thumbnailer Entry]\nExec=/usr/bin/glycin-thumbnailer --input %u --output %o --size %s\nMimeType=image/jpeg;\n".into())],
            &aliases, true, |p| Some(p.to_owned()),
        );
        assert!(specs.can_speed_up_jpeg("image/jpeg", &aliases));
        let path = dir.file(
            "photo.jpg",
            "source not decoded by the orchestration fixtures",
        );
        let job = Job {
            replies: None,
            path,
            mtime: 42,
            source_size: Some(1024 * 1024),
            trash_identity: None,
            mime: "image/jpeg".into(),
            trace: None,
        };
        let image = dir.join("fixture.png");
        write_marker(&image, "file:///fixture", 1, None).unwrap();
        let png = std::fs::read(image)
            .unwrap()
            .iter()
            .map(|b| format!("\\{b:03o}"))
            .collect();
        let tables = Tables {
            aliases,
            specs: Arc::new(specs),
            cache: Cache::at(dir.join("cache")),
            worker: None,
        };
        Self {
            dir,
            tables,
            job,
            png,
        }
    }

    fn run(&mut self, fast: &str, fallback: &str) -> Outcome {
        assert!(
            sandbox::available(),
            "these tests require bwrap and prlimit"
        );
        let fast = self.dir.script(
            "fast-child",
            &format!(
                "#!/bin/sh\n[ \"$1\" = --jpeg-thumbnail ] && [ \"$4\" = 256 ] || exit 21\n{fast}\n"
            ),
        );
        let fallback = self.dir.script("fallback-child", &format!("#!/bin/sh\n[ \"$1\" = --input ] && [ \"$3\" = --output ] && [ \"$5\" = --size ] && [ \"$6\" = 256 ] || exit 22\n{fallback}\n"));
        run_one_with_programs(&self.tables, &mut self.job, &self.dir.join("raw"), &|p| {
            if p == "/usr/bin/glycin-thumbnailer" {
                &fallback
            } else {
                &fast
            }
            .to_str()
            .unwrap()
            .to_owned()
        })
    }

    fn assert_ready(&self, outcome: Outcome) {
        let Outcome::Ready(file) = outcome else {
            panic!("expected a published thumbnail")
        };
        let uri = uri_for(&self.job.path);
        assert_eq!(file, self.tables.cache.large_path(&uri));
        let bytes = std::fs::read(file).unwrap();
        assert_eq!(png_text(&bytes, "Thumb::URI"), Some(uri.clone()));
        assert_eq!(png_text(&bytes, "Thumb::MTime").as_deref(), Some("42"));
        assert!(!self.tables.cache.fail_path(&uri).exists());
        assert!(matches!(
            self.tables
                .cache
                .lookup(&self.job.path, 42, self.tables.specs.fingerprint()),
            Hit::Ready(_)
        ));
        self.assert_no_temps();
    }

    fn assert_no_temps(&self) {
        let entries = std::fs::read_dir(self.tables.cache.large_dir()).unwrap();
        assert!(entries
            .flatten()
            .all(|e| !e.file_name().to_string_lossy().starts_with('.')));
    }
}

#[test]
fn jpeg_fast_success_publishes_without_fallback() {
    let mut f = Fixture::new();
    let outcome = f.run(&format!("printf '{}' > \"$3\"", f.png), "exit 23");
    f.assert_ready(outcome);
}

#[test]
fn jpeg_timeout_clears_partial_output_before_fallback() {
    let mut f = Fixture::new();
    let outcome = f.run(
        "printf partial > \"$3\"\nexec /usr/bin/sleep 60",
        &format!("[ ! -s \"$4\" ] || exit 24\nprintf '{}' > \"$4\"", f.png),
    );
    f.assert_ready(outcome);
}

#[test]
fn jpeg_partial_fast_output_is_not_published_after_empty_fallback() {
    let mut f = Fixture::new();
    let outcome = f.run(&format!("printf '{}' > \"$3\"\nexit 1", f.png), "exit 0");
    assert!(matches!(outcome, Outcome::Failed));
    let uri = uri_for(&f.job.path);
    assert!(!f.tables.cache.large_path(&uri).exists());
    assert!(matches!(
        f.tables
            .cache
            .lookup(&f.job.path, 42, f.tables.specs.fingerprint()),
        Hit::Failed
    ));
    let bytes = std::fs::read(f.tables.cache.fail_path(&uri)).unwrap();
    assert_eq!(png_text(&bytes, "Thumb::URI"), Some(uri));
    assert_eq!(png_text(&bytes, "Thumb::MTime").as_deref(), Some("42"));
    f.assert_no_temps();
}

#[test]
fn jpeg_small_source_skips_fast_child_and_publishes_fallback() {
    let mut f = Fixture::new();
    f.job.source_size = Some(100);
    let outcome = f.run(
        &format!("printf '{}' > \"$3\"", f.png),
        &format!("printf '{}' > \"$4\"", f.png),
    );
    f.assert_ready(outcome);
    // A failed fallback would be hidden by a wrongly selected successful fast child.
    assert!(matches!(
        f.run(&format!("printf '{}' > \"$3\"", f.png), "exit 1"),
        Outcome::Failed
    ));
}
