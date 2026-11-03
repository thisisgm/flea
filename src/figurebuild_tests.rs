use super::*;
use crate::backend::testdir::TestDir;
use std::cell::{Cell, RefCell};

// A UI tree and engine of their own, and a cache root inside the same sandbox.
struct Rig {
    dir: TestDir,
    ui: PathBuf,
    qjs: PathBuf,
    root: PathBuf,
}

fn rig(name: &str) -> Rig {
    let dir = TestDir::new(name);
    let ui = dir.join("ui");
    fs::create_dir_all(ui.join("vendor")).expect("vendor dir");
    fs::create_dir_all(ui.join("js")).expect("js dir");
    for relative in crate::figurecache::KEYED_SOURCES {
        fs::write(ui.join(relative), format!("// {relative}\n")).expect("source");
    }
    let qjs = dir.file("qjs", "engine\n");
    let root = dir.join("cache");
    fs::create_dir(&root).expect("cache root");
    Rig { dir, ui, qjs, root }
}

// What a healthy helper prints for the two smoke requests with `--report`: each bundle's load line, then its answer.
fn smoke_answer(math_from: &str, mermaid_from: &str) -> String {
    format!(
        "{{\"id\":0,\"bundle\":\"math\",\"from\":\"{math_from}\"}}\n{{\"id\":1,\"svg\":\"<svg/>\"}}\n{{\"id\":0,\"bundle\":\"mermaid\",\"from\":\"{mermaid_from}\"}}\n{{\"id\":2,\"svg\":\"<svg/>\"}}\n"
    )
}

// A jail stand-in: the compile call writes the blobs into its scratch dir, a smoke call answers what `smoke` says.
fn fake<'a>(calls: &'a RefCell<Vec<Vec<String>>>, blobs: &'a [&'a str], smoke: &'a dyn Fn(bool) -> Result<String, String>) -> impl Fn(&[String], &str) -> Result<String, String> + 'a {
    move |argv: &[String], _input: &str| {
        calls.borrow_mut().push(argv.to_vec());
        if argv.iter().any(|a| a.ends_with(COMPILE_NAME)) {
            let scratch = PathBuf::from(argv.last().expect("scratch arg"));
            for name in blobs {
                fs::write(scratch.join(name), format!("bytecode of {name}")).expect("fake blob");
            }
            return Ok(String::new());
        }
        smoke(argv.iter().any(|a| a.contains(".tmp-")))
    }
}

fn healthy(bytecode: bool) -> Result<String, String> {
    let from = if bytecode { "bytecode" } else { "source" };
    Ok(smoke_answer(from, from))
}

#[test]
fn the_compile_jail_binds_one_writable_directory_and_the_vendor_tree_read_only() {
    let argv = compile_argv(Path::new("/usr/bin/qjs"), Path::new("/ui/vendor"), Path::new("/cache/figures/k.tmp-1"));
    let joined = argv.join(" ");
    assert_eq!(argv[0], "prlimit");
    assert!(joined.contains("--unshare-all") && joined.contains("--clearenv"), "the same boundary as the render jail: {joined}");
    assert_eq!(argv.iter().filter(|a| *a == "--bind").count(), 1, "exactly one writable bind: {joined}");
    assert!(joined.contains("--bind /cache/figures/k.tmp-1 /cache/figures/k.tmp-1"), "and it is the scratch directory: {joined}");
    assert!(joined.contains("--ro-bind /ui/vendor /ui/vendor"), "the sources it compiles stay read-only: {joined}");
    assert!(!joined.contains("--ro-bind /usr/bin/qjs"), "an engine under /usr is already visible: {joined}");
    assert_eq!(argv[argv.len() - 4..], ["/usr/bin/qjs", "/ui/vendor/figure-compile.mjs", "/ui/vendor", "/cache/figures/k.tmp-1"]);
    let hooked = compile_argv(Path::new("/home/x/qjs"), Path::new("/ui/vendor"), Path::new("/c/k.tmp-1")).join(" ");
    assert!(hooked.contains("--ro-bind /home/x/qjs /home/x/qjs"), "a hook outside /usr is bound read-only: {hooked}");
}

#[test]
fn a_build_installs_a_verified_directory_and_sweeps_what_is_no_longer_current() {
    let rig = rig("figure-build-ok");
    let old = rig.root.join(figurecache::digest(b"an older key"));
    fs::create_dir(&old).expect("old key dir");
    let old_key = old.file_name().unwrap().to_str().unwrap();
    fs::write(old.join(figurecache::MANIFEST), figurecache::manifest_text(old_key, &figurecache::BLOBS.map(|n| (n.to_string(), 1, "digest".to_string())))).unwrap();
    fs::write(rig.root.join("notes.txt"), "not ours").expect("unrelated file");
    fs::create_dir(rig.root.join("keepme")).expect("unrelated dir");
    let calls = RefCell::new(Vec::new());
    let smoke: &dyn Fn(bool) -> Result<String, String> = &healthy;
    build(&rig.root, &rig.qjs, &rig.ui, &fake(&calls, &figurecache::BLOBS, smoke)).expect("a clean build");
    let key = figurecache::key(&rig.qjs, &rig.ui).expect("key");
    assert_eq!(figurecache::verified(&rig.root.join(&key), &key), Some(rig.root.join(&key)));
    assert_eq!(calls.borrow().len(), 3, "one compile, then the same two requests through bytecode and source");
    assert!(!old.exists(), "another key's directory is swept");
    assert!(rig.root.join("notes.txt").is_file() && rig.root.join("keepme").is_dir(), "nothing that is not ours is touched");
    assert!(!lock_path(&rig.root, &key).exists() && !failed_path(&rig.root, &key).exists(), "no lock or failure mark remains");
    let names: Vec<String> = fs::read_dir(&rig.root).expect("root").flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    assert!(names.iter().all(|n| !n.contains(".tmp-") && !n.contains(".old-")), "no scratch remains: {names:?}");
    let again = RefCell::new(Vec::new());
    build(&rig.root, &rig.qjs, &rig.ui, &fake(&again, &figurecache::BLOBS, smoke)).expect("a second build");
    assert!(again.borrow().is_empty(), "a verified cache is not built again");
}

#[test]
fn a_foreign_directory_is_preserved_and_a_known_generation_can_be_repaired() {
    let rig = rig("figure-build-replace");
    let key = figurecache::key(&rig.qjs, &rig.ui).expect("key");
    let outside = rig.dir.join("outside");
    fs::create_dir(&outside).expect("outside dir");
    fs::write(outside.join("precious"), "keep").expect("outside file");
    std::os::unix::fs::symlink(&outside, rig.root.join(&key)).expect("link at the key's path");
    let calls = RefCell::new(Vec::new());
    assert!(build(&rig.root, &rig.qjs, &rig.ui, &fake(&calls, &figurecache::BLOBS, &healthy)).unwrap_err().contains("left unchanged"));
    assert!(rig.root.join(&key).symlink_metadata().unwrap().file_type().is_symlink());
    assert_eq!(fs::read_to_string(outside.join("precious")).expect("outside file"), "keep", "the link's target is untouched");
    rig.dir.assert_contains(&rig.root.join(&key));
    fs::remove_file(rig.root.join(&key)).unwrap();
    fs::remove_file(failed_path(&rig.root, &key)).unwrap();
    build(&rig.root, &rig.qjs, &rig.ui, &fake(&calls, &figurecache::BLOBS, &healthy)).unwrap();
    fs::write(rig.root.join(&key).join("math.bc"), "corrupt").expect("corrupt blob");
    let again = RefCell::new(Vec::new());
    build(&rig.root, &rig.qjs, &rig.ui, &fake(&again, &figurecache::BLOBS, &healthy)).expect("a build over a corrupt directory");
    assert_eq!(figurecache::verified(&rig.root.join(&key), &key), Some(rig.root.join(&key)), "the corrupt directory is replaced whole");
}

#[test]
fn a_failed_build_leaves_nothing_live_and_is_not_retried_at_once() {
    for (label, blobs, smoke) in [
        ("a compile that writes one blob", &figurecache::BLOBS[..1], &healthy as &dyn Fn(bool) -> Result<String, String>),
        ("bytecode that answers differently", &figurecache::BLOBS[..], &|bytecode| Ok(if bytecode { smoke_answer("bytecode", "bytecode").replacen("<svg/>", "<svg>x</svg>", 1) } else { smoke_answer("source", "source") })),
        ("bytecode that answers nothing", &figurecache::BLOBS[..], &|bytecode| Ok(if bytecode { String::new() } else { smoke_answer("source", "source") })),
        ("a jail that fails", &figurecache::BLOBS[..], &|_| Err("jail refused".to_string())),
        ("bytecode that never loaded and answered from source", &figurecache::BLOBS[..], &|_| Ok(smoke_answer("source", "source"))),
        ("one bundle that fell back to source", &figurecache::BLOBS[..], &|bytecode| Ok(if bytecode { smoke_answer("bytecode", "source") } else { smoke_answer("source", "source") })),
        ("a source run that claims bytecode", &figurecache::BLOBS[..], &|_| Ok(smoke_answer("bytecode", "bytecode"))),
    ] {
        let rig = rig("figure-build-fail");
        let key = figurecache::key(&rig.qjs, &rig.ui).expect("key");
        let calls = RefCell::new(Vec::new());
        let result = build(&rig.root, &rig.qjs, &rig.ui, &fake(&calls, blobs, smoke));
        assert!(result.is_err(), "{label} fails the build");
        assert!(!rig.root.join(&key).exists(), "{label} installs nothing");
        assert!(failed_path(&rig.root, &key).is_file() && !lock_path(&rig.root, &key).exists(), "{label} leaves the failure mark and no lock");
        assert!(!wanted(&rig.root, &key), "{label} is not retried at once");
        let before = calls.borrow().len();
        build(&rig.root, &rig.qjs, &rig.ui, &fake(&calls, blobs, smoke)).expect("a repeat is quiet");
        assert_eq!(calls.borrow().len(), before, "{label} runs no jail again inside the retry window");
        let names: Vec<String> = fs::read_dir(&rig.root).expect("root").flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert!(names.iter().all(|n| !n.contains(".tmp-")), "{label} leaves no scratch: {names:?}");
    }
}

// Backdates a path's modification time.
fn age_by(path: &Path, by: Duration) {
    let file = fs::File::open(path).expect("open for time");
    file.set_modified(SystemTime::now() - by).expect("set mtime");
}

#[test]
fn a_live_lock_defers_a_build_and_a_stale_one_or_an_old_failure_does_not() {
    let rig = rig("figure-build-lock");
    let key = figurecache::key(&rig.qjs, &rig.ui).expect("key");
    create_new(&lock_path(&rig.root, &key)).expect("a rival's lock");
    let calls = RefCell::new(Vec::new());
    build(&rig.root, &rig.qjs, &rig.ui, &fake(&calls, &figurecache::BLOBS, &healthy)).expect("deferred");
    assert!(calls.borrow().is_empty() && !wanted(&rig.root, &key), "a fresh lock means another builder is on it");
    age_by(&lock_path(&rig.root, &key), LOCK_STALE + Duration::from_secs(1));
    assert!(wanted(&rig.root, &key), "a lock older than any build is stale");
    build(&rig.root, &rig.qjs, &rig.ui, &fake(&calls, &figurecache::BLOBS, &healthy)).expect("built over a stale lock");
    assert_eq!(figurecache::verified(&rig.root.join(&key), &key), Some(rig.root.join(&key)));
    let failed = rig.dir.join("failed-mark");
    fs::write(&failed, "").expect("mark");
    age_by(&failed, FAILED_RETRY + Duration::from_secs(1));
    let other = rig.dir.join("cache2");
    fs::create_dir_all(&other).expect("second root");
    fs::copy(&failed, failed_path(&other, &key)).expect("copy mark");
    age_by(&failed_path(&other, &key), FAILED_RETRY + Duration::from_secs(1));
    assert!(wanted(&other, &key), "a failure is retried after its window");
}

#[test]
fn the_final_unlock_removes_only_the_lock_that_is_still_this_builders() {
    let first = rig("figure-build-owner");
    let key = figurecache::key(&first.qjs, &first.ui).expect("key");
    let lock = lock_path(&first.root, &key);
    // A second builder takes the lock over while the first is still compiling.
    let calls = RefCell::new(Vec::new());
    let inner = fake(&calls, &figurecache::BLOBS, &healthy);
    let takeover = |argv: &[String], input: &str| {
        if argv.iter().any(|a| a.ends_with(COMPILE_NAME)) {
            fs::remove_file(&lock).expect("the first builder's lock");
            fs::write(&lock, "rival builder").expect("the rival's lock");
        }
        inner(argv, input)
    };
    build(&first.root, &first.qjs, &first.ui, &takeover).expect("a build");
    assert_eq!(fs::read_to_string(&lock).expect("the rival's lock is still there"), "rival builder");
    let alone = rig("figure-build-owner-alone");
    let alone_key = figurecache::key(&alone.qjs, &alone.ui).expect("key");
    let alone_lock = lock_path(&alone.root, &alone_key);
    let held = Cell::new(false);
    let alone_calls = RefCell::new(Vec::new());
    let alone_inner = fake(&alone_calls, &figurecache::BLOBS, &healthy);
    let watch = |argv: &[String], input: &str| {
        if argv.iter().any(|a| a.ends_with(COMPILE_NAME)) {
            held.set(alone_lock.exists());
        }
        alone_inner(argv, input)
    };
    build(&alone.root, &alone.qjs, &alone.ui, &watch).expect("a build");
    assert!(held.get(), "the builder held its lock while it compiled");
    assert!(!alone_lock.exists(), "a builder's own lock is removed");
}

#[test]
fn only_stale_scratch_and_other_keys_are_swept() {
    let rig = rig("figure-build-sweep");
    let current = figurecache::digest(b"current");
    let other = figurecache::digest(b"other");
    let stale_scratch = rig.root.join(format!("{other}.tmp-4242"));
    let fresh_scratch = rig.root.join(format!("{other}.tmp-4343"));
    let stale_lock = rig.root.join(format!("{other}.lock"));
    let live_lock = rig.root.join(format!("{current}.lock"));
    let failed = rig.root.join(format!("{other}.failed"));
    for dir in [&stale_scratch, &fresh_scratch] {
        fs::create_dir(dir).expect("scratch dir");
    }
    for file in [&stale_lock, &live_lock, &failed] {
        fs::write(file, "").expect("marker file");
    }
    for path in [&stale_scratch, &stale_lock] {
        age_by(path, SCRATCH_STALE + Duration::from_secs(1));
    }
    let strangers = [rig.root.join("notes.txt"), rig.root.join(format!("{other}.bak")), rig.root.join("0123.tmp-1")];
    for path in &strangers {
        fs::write(path, "").expect("stranger");
    }
    sweep(&rig.root, &current);
    assert!(!stale_scratch.exists() && !stale_lock.exists() && !failed.exists(), "stale scratch, a stale lock of another key and its failure mark go");
    assert!(fresh_scratch.exists(), "a builder that may still be running keeps its scratch");
    assert!(live_lock.exists(), "the current key's own lock is the builder's to remove");
    assert!(strangers.iter().all(|p| p.exists()), "names that are not ours are never touched");
}

#[test]
fn classify_reads_only_our_names() {
    let key = figurecache::digest(b"k");
    assert!(matches!(classify(&key), Some((_, Kind::Live))));
    assert!(matches!(classify(&format!("{key}.lock")), Some((_, Kind::Lock))));
    assert!(matches!(classify(&format!("{key}.failed")), Some((_, Kind::Failed))));
    assert!(matches!(classify(&format!("{key}.tmp-12")), Some((_, Kind::Scratch))));
    assert!(matches!(classify(&format!("{key}.old-12")), Some((_, Kind::Scratch))));
    for bad in ["", "manifest", &format!("{key}.tmp-"), &format!("{key}.tmp-1a"), &format!("{key}x"), &format!("{key}.lock2"), "x.lock"] {
        assert!(classify(bad).is_none(), "{bad:?}");
    }
}

// The escape probes: each writes one place a jail must not let it, every one inside the test's own root, so nothing is ever written outside it.
struct Probe {
    dir: TestDir,
    scratch: PathBuf,
    sibling: PathBuf,
    vendor: PathBuf,
    outside: PathBuf,
}

fn probe(name: &str) -> Probe {
    let dir = TestDir::new(name);
    let (scratch, sibling, vendor, outside) = (dir.join("scratch"), dir.join("sibling"), dir.join("vendor"), dir.join("outside"));
    for path in [&scratch, &sibling, &vendor, &outside] {
        fs::create_dir(path).expect("dir");
    }
    Probe { dir, scratch, sibling, vendor, outside }
}

impl Probe {
    // The command: the scratch takes a write; a sibling, the read-only vendor tree, an unbound directory and the root itself must not.
    fn inner(&self) -> Vec<String> {
        let script = "echo ok > \"$1/inside\"; echo no > \"$2/sibling\"; echo no > \"$3/vendor\"; echo no > \"$4/outside\"; echo no > \"$5/root\"; exit 0";
        let paths = [&self.scratch, &self.sibling, &self.vendor, &self.outside, &self.dir.path().to_path_buf()];
        ["/bin/sh", "-c", script, "sh"].iter().map(|s| s.to_string()).chain(paths.iter().map(|p| p.to_string_lossy().into_owned())).collect()
    }

    fn escapes(&self) -> Vec<&'static str> {
        let wrote = [
            ("a sibling directory", self.sibling.join("sibling")),
            ("the read-only vendor bind", self.vendor.join("vendor")),
            ("an unbound directory", self.outside.join("outside")),
            ("the root itself", self.dir.join("root")),
        ];
        wrote.into_iter().filter(|(_, path)| path.exists()).map(|(label, _)| label).collect()
    }
}

#[test]
fn every_escape_probe_fires_when_nothing_jails_it() {
    let rig = probe("figure-jail-teeth");
    let status = Command::new(&rig.inner()[0]).args(&rig.inner()[1..]).status().expect("sh runs");
    assert!(status.success());
    assert!(rig.scratch.join("inside").is_file(), "the scratch write lands");
    assert_eq!(rig.escapes(), ["a sibling directory", "the read-only vendor bind", "an unbound directory", "the root itself"], "each probe is red as the runner that runs it, with no jail");
}

// The jail itself, where bwrap can make a namespace: the scratch directory takes a write and nothing else does.
#[test]
fn the_real_compile_jail_writes_only_its_scratch_directory() {
    // Only a box whose layout the jail supports can run it: Arch's, which the CI image has and a Debian host does not.
    let runnable = |argv: Vec<String>| Command::new(&argv[0]).args(&argv[1..]).stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false);
    let probe_dir = TestDir::new("figure-jail-probe");
    if !sandbox::available() || !runnable(sandbox::wrap_compile(&["/bin/true".to_string()], &[], probe_dir.path())) {
        eprintln!("SKIP the compile jail cannot run here, so it was not run for real");
        return;
    }
    let rig = probe("figure-jail");
    let argv = sandbox::wrap_compile(&rig.inner(), &[rig.vendor.as_path()], &rig.scratch);
    let status = Command::new(&argv[0]).args(&argv[1..]).stdout(Stdio::null()).stderr(Stdio::null()).status().expect("jail starts");
    assert!(status.success(), "the jail ran the probe");
    assert!(rig.scratch.join("inside").is_file(), "the scratch directory takes the write");
    assert_eq!(rig.escapes(), Vec::<&str>::new(), "nothing else does");
}

#[test]
fn a_cache_root_that_is_gone_is_not_recreated_by_a_late_build() {
    let rig = rig("figure-build-gone");
    fs::remove_dir(&rig.root).expect("remove the root");
    let calls = RefCell::new(Vec::new());
    let result = build(&rig.root, &rig.qjs, &rig.ui, &fake(&calls, &figurecache::BLOBS, &healthy));
    assert!(result.is_err() && calls.borrow().is_empty(), "no jail runs without a root");
    assert!(!rig.root.exists(), "and the root stays gone");
    let made = rig.dir.join("deep/er/cache");
    assert!(make_root(&made).is_some() && made.is_dir(), "the launcher makes the root, parents included");
}

#[test]
fn foreign_and_newer_manifests_survive_read_build_and_sweep() {
    for original in ["{\"foreign\":true}\n", "flea-figures 999\nkey future\n", "unparsable"] {
        let rig = rig("figure-preserve");
        let key = figurecache::key(&rig.qjs, &rig.ui).unwrap();
        let dir = rig.root.join(&key);
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join(figurecache::MANIFEST), original).unwrap();
        fs::write(dir.join("precious"), "untouched").unwrap();
        assert!(figurecache::verified(&dir, &key).is_none());
        assert_eq!(fs::read_to_string(dir.join(figurecache::MANIFEST)).unwrap(), original);
        let calls = RefCell::new(Vec::new());
        assert!(build(&rig.root, &rig.qjs, &rig.ui, &fake(&calls, &figurecache::BLOBS, &healthy)).unwrap_err().contains("left unchanged"));
        sweep(&rig.root, "another-key");
        assert_eq!(fs::read_to_string(dir.join(figurecache::MANIFEST)).unwrap(), original);
        assert_eq!(fs::read_to_string(dir.join("precious")).unwrap(), "untouched");
        assert!(calls.borrow().is_empty());
    }
}
