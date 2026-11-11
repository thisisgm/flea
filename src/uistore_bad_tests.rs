// A ui.json another process locks, that is not text, that is not a file, or that is wide: none may stop a launch or a save.
use super::*;
use crate::backend::fifotest::{mkfifo, within};
use crate::backend::testdir::TestDir;
use crate::jsondoc::{self, Json};
use crate::uischema::defaults;
use std::os::unix::fs::FileTypeExt;
use std::time::{Duration, Instant};

// A linear load charges a few units per key byte; a quadratic one exceeds this many per file byte.
const KEY_WORK_PER_BYTE: u64 = 128;
// The former wall-clock limit, now on thread CPU so a loaded host's run queue cannot exceed it.
const LINEAR_LOAD_CPU: Duration = Duration::from_secs(60);

fn store(d: &TestDir) -> Store {
    Store::at(&d.dir("state"), &d.dir("config"))
}

fn patch(text: &str) -> Json {
    jsondoc::parse(text).expect("the patch parses")
}

fn seeded(d: &TestDir) -> Store {
    let s = store(d);
    d.assert_contains(s.file());
    fs::create_dir_all(s.file().parent().expect("state dir")).expect("state dir");
    s
}

fn defaults_text() -> String {
    jsondoc::render(&defaults())
}

#[test]
fn a_lock_held_elsewhere_bounds_the_launch_and_the_save() {
    let d = TestDir::new("uistore-held");
    let s = seeded(&d);
    // Not the rendered shape, so the launch has to take the lock to settle it.
    fs::write(s.file(), r#"{"view":"grid"}"#).expect("seed");
    let holder = take_lock(s.lock_file()).expect("hold the lock");
    let file = s.file().to_path_buf();
    let (state, config) = (d.join("state"), d.join("config"));
    let started = Instant::now();
    let settled = within("settle under a held lock", move || {
        Store::at(&state, &config).settle()
    });
    let why = settled.expect_err("a held lock is reported, not waited on for good");
    assert!(
        why.contains("ui.json.lock") && why.contains("another process"),
        "{}",
        why
    );
    assert!(
        started.elapsed() >= LOCK_WAIT,
        "the wait is LOCK_WAIT, so an ordinary hold still lands"
    );
    assert!(
        s.update(&patch(r#"{"hidden":true}"#)).is_err(),
        "a save under the held lock is refused"
    );
    assert_eq!(
        fs::read_to_string(&file).expect("kept"),
        r#"{"view":"grid"}"#
    );
    assert_eq!(
        s.read().get("view").and_then(Json::as_str),
        Some("grid"),
        "the window opens on what the file says"
    );
    assert!(s.held_elsewhere());
    unlock(&holder).expect("release");
    assert!(!lock_held_elsewhere(&file));
    s.update(&patch(r#"{"hidden":true}"#))
        .expect("a freed lock saves");
    assert_eq!(s.read().get("hidden").and_then(Json::as_bool), Some(true));
}

#[test]
fn bytes_that_are_not_text_are_set_aside_and_the_next_save_lands() {
    let d = TestDir::new("uistore-binary");
    let s = seeded(&d);
    let garbage: Vec<u8> = (0..4096u32)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8 | 0x80)
        .collect();
    fs::write(s.file(), &garbage).expect("garbage");
    assert_eq!(
        s.read_checked(),
        (defaults(), true),
        "read as the defaults, and said so"
    );
    s.settle().expect("a launch leaves it for the first save");
    assert_eq!(fs::read(s.file()).expect("kept"), garbage);
    s.update(&patch(r#"{"hidden":true}"#))
        .expect("the save is not refused");
    assert_eq!(
        fs::read(s.file().with_file_name("ui.json.broken")).expect("backup"),
        garbage
    );
    assert!(!s.read_checked().1);
    assert_eq!(s.read().get("hidden").and_then(Json::as_bool), Some(true));
    s.update(&patch(r#"{"hidden":false}"#))
        .expect("a later save");
    assert_eq!(
        fs::read(s.file().with_file_name("ui.json.broken")).expect("backup kept"),
        garbage
    );
}

#[test]
fn a_fifo_or_a_device_never_blocks_and_is_set_aside_like_a_bad_file() {
    assert!(
        matches!(write_target(Path::new("/dev/null")), Ok(Target::Bad)),
        "a device node is a bad file, set aside without being opened"
    );
    let d = TestDir::new("uistore-fifo");
    let s = seeded(&d);
    mkfifo(s.file());
    let (state, config) = (d.join("state"), d.join("config"));
    let (read, settled) = within("a fifo ui.json", move || {
        let s = Store::at(&state, &config);
        (s.read_checked(), s.settle())
    });
    assert_eq!(read, (defaults(), true));
    settled.expect("a launch leaves the fifo for the first save");
    assert!(fs::symlink_metadata(s.file())
        .expect("fifo")
        .file_type()
        .is_fifo());
    s.update(&patch(r#"{"hidden":true}"#))
        .expect("the save sets the fifo aside");
    let backup = s.file().with_file_name("ui.json.broken");
    assert!(
        fs::symlink_metadata(&backup)
            .expect("backup")
            .file_type()
            .is_fifo(),
        "the fifo itself is kept, never opened"
    );
    assert_eq!(s.read().get("hidden").and_then(Json::as_bool), Some(true));
    fs::write(s.file(), [0xff_u8, 0xfe]).expect("garbage after the fifo");
    s.update(&patch(r#"{"hidden":false}"#))
        .expect("a fifo kept aside never blocks the next set-aside");
    assert_eq!(fs::read(&backup).expect("backup"), [0xff_u8, 0xfe]);

    let z = TestDir::new("uistore-devzero");
    let s = seeded(&z);
    std::os::unix::fs::symlink("/dev/zero", s.file()).expect("link to /dev/zero");
    let (state, config) = (z.join("state"), z.join("config"));
    let (text, settled) = within("a ui.json linked to /dev/zero", move || {
        let s = Store::at(&state, &config);
        (jsondoc::render(&s.read()), s.settle())
    });
    assert_eq!(text, defaults_text());
    assert!(settled
        .expect_err("a link is still refused")
        .contains("symbolic link"));

    let l = TestDir::new("uistore-lockfifo");
    let s = seeded(&l);
    mkfifo(s.lock_file());
    let file = s.file().to_path_buf();
    assert!(
        !within("a fifo at the lock's path", move || lock_held_elsewhere(
            &file
        )),
        "the probe neither waits on the fifo nor calls it held"
    );
}

#[test]
fn a_backup_that_already_is_ui_json_leaves_no_link_behind() {
    let d = TestDir::new("uistore-samelink");
    let s = seeded(&d);
    fs::write(s.file(), [0xff_u8, 0xfe]).expect("garbage");
    let backup = s.file().with_file_name("ui.json.broken");
    // A crash between the set-aside and the swap leaves both names on the one bad file.
    fs::hard_link(s.file(), &backup).expect("both names");
    s.update(&patch(r#"{"hidden":true}"#))
        .expect("the save lands");
    let left: Vec<String> = fs::read_dir(s.file().parent().expect("state dir"))
        .expect("state dir")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name.ends_with(".tmp"))
        .collect();
    assert!(left.is_empty(), "{left:?}");
    assert_eq!(fs::read(&backup).expect("backup"), [0xff_u8, 0xfe]);
}

#[test]
fn a_view_json_fifo_beside_no_ui_json_reads_the_defaults() {
    let d = TestDir::new("uistore-legacy-fifo");
    let s = store(&d);
    fs::create_dir_all(s.legacy().parent().expect("config dir")).expect("config dir");
    mkfifo(s.legacy());
    let (state, config) = (d.join("state"), d.join("config"));
    let text = within("a fifo view.json", move || {
        jsondoc::render(&Store::at(&state, &config).read())
    });
    assert_eq!(text, defaults_text());
}

#[test]
fn a_file_past_the_cap_reads_the_defaults_and_is_set_aside_unread() {
    let d = TestDir::new("uistore-oversize");
    let s = seeded(&d);
    // Sparse, so the test writes nothing near the cap to disk.
    fs::File::create(s.file())
        .and_then(|f| f.set_len(MAX_BYTES + 1))
        .expect("oversize");
    let refused = read_state(s.file()).expect_err("past the cap");
    assert!(
        refused.kind() == std::io::ErrorKind::InvalidData
            && refused.to_string().contains("larger than"),
        "refused by the cap, not by the parse of its zero bytes: {refused}"
    );
    assert_eq!(s.read_checked(), (defaults(), true));
    s.update(&patch(r#"{"hidden":true}"#))
        .expect("the save sets it aside");
    assert_eq!(
        fs::metadata(s.file().with_file_name("ui.json.broken"))
            .expect("backup")
            .len(),
        MAX_BYTES + 1
    );
}

#[test]
fn a_wide_folder_sorts_map_loads_and_settles_in_linear_time() {
    let d = TestDir::new("uistore-wide");
    let s = seeded(&d);
    let count = 150_000;
    let mut text = String::from("{\"folderSorts\":{");
    for i in 0..count {
        text.push_str(&format!(
            "{}\"/w/{i}\":{{\"key\":\"size\",\"reverse\":true}}",
            if i == 0 { "" } else { "," }
        ));
    }
    text.push_str("}}");
    fs::write(s.file(), &text).expect("wide");
    // Quadratic duplicate removal took 10.8 s at 30000 entries in a release build.
    let budget = text.len() as u64 * KEY_WORK_PER_BYTE;
    let cpu = jsondoc::cost::thread_cpu();
    let ((read, settled), work) = jsondoc::cost::measured(budget, || (s.read(), s.settle()));
    let spent = jsondoc::cost::thread_cpu() - cpu;
    eprintln!("wide map: {work} key-work units, {spent:?} thread CPU");
    // The meter counts jsondoc's name hashes, clones and comparisons and every Json::get, so CPU bounds the rest.
    assert!(
        (count as u64..=budget).contains(&work),
        "wide map used {work} key-work units"
    );
    assert!(
        spent < LINEAR_LOAD_CPU,
        "a wide map must load and settle in linear time: {spent:?} of thread CPU"
    );
    settled.expect("settle trims it");
    let kept = |doc: &Json| {
        doc.get("folderSorts")
            .and_then(Json::as_object)
            .map(<[(String, Json)]>::len)
    };
    assert_eq!(kept(&read), Some(crate::uischema::MAX_FOLDER_SORTS));
    let written = jsondoc::parse(&fs::read_to_string(s.file()).expect("settled")).expect("JSON");
    assert_eq!(kept(&written), Some(crate::uischema::MAX_FOLDER_SORTS));
    assert!(
        written
            .get("folderSorts")
            .and_then(|m| m.get(&format!("/w/{}", count - 1)))
            .is_some(),
        "the newest entries stay"
    );
}

#[test]
fn a_repeated_name_is_the_later_value_in_the_later_place() {
    let v = jsondoc::parse(r#"{"a":1,"b":2,"a":3}"#).expect("parse");
    let keys: Vec<&str> = v
        .as_object()
        .expect("object")
        .iter()
        .map(|(k, _)| k.as_str())
        .collect();
    assert_eq!(keys, ["b", "a"]);
    assert_eq!(v.get("a").and_then(Json::as_f64), Some(3.0));
    let mut wide = String::from("{");
    for round in 0..2 {
        for i in 0..50_000 {
            wide.push_str(&format!(
                "{}\"k{i}\":{round}",
                if round == 0 && i == 0 { "" } else { "," }
            ));
        }
    }
    wide.push('}');
    let budget = wide.len() as u64 * KEY_WORK_PER_BYTE;
    let cpu = jsondoc::cost::thread_cpu();
    let (len, work) = jsondoc::cost::measured(budget, || {
        jsondoc::parse(&wide).map(|v| v.as_object().map(<[(String, Json)]>::len))
    });
    let spent = jsondoc::cost::thread_cpu() - cpu;
    eprintln!("duplicate map: {work} key-work units, {spent:?} thread CPU");
    assert!(
        (100_000..=budget).contains(&work),
        "duplicate map used {work} key-work units"
    );
    assert!(
        spent < LINEAR_LOAD_CPU,
        "every name repeated once must stay linear: {spent:?} of thread CPU"
    );
    assert_eq!(len, Ok(Some(50_000)));
}
