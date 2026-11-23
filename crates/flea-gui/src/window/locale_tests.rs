use super::locale::Service;
use flea::i18n::Paths;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

// A worker that dies before its first publish closes the first receiver, so main exits instead of blocking.
#[test]
fn a_worker_that_dies_before_its_first_publish_closes_the_first_receiver() {
    let paths = Paths {
        omarchy: PathBuf::new(),
        language_file: PathBuf::new(),
        catalogs: Vec::new(),
    };
    let (service, initial) =
        Service::start_with(paths, || panic!("the worker dies before its first publish")).unwrap();
    let (report, outcome) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = report.send(initial.recv_blocking().is_err());
    });
    let outcome = outcome.recv_timeout(Duration::from_secs(10));
    drop(service);
    assert_eq!(
        outcome,
        Ok(true),
        "the first receiver blocked on a dead worker"
    );
}

fn empty_paths() -> Paths {
    Paths {
        omarchy: PathBuf::new(),
        language_file: PathBuf::new(),
        catalogs: Vec::new(),
    }
}

// main.rs's launch path: a worker that dies before its first publish is a startup refusal, never a hung launch.
#[test]
fn m9_integ_g05_first_publish_worker_exit_never_hangs() {
    let (report, outcome) = mpsc::channel();
    std::thread::spawn(move || {
        let launched = Service::launch_with(empty_paths(), || panic!("the worker dies first"));
        let _ = report.send(launched.err().map(|error| error.kind()));
    });
    assert_eq!(
        outcome.recv_timeout(Duration::from_secs(10)),
        Ok(Some(std::io::ErrorKind::BrokenPipe)),
        "the launch hung or succeeded on a dead catalog worker"
    );
    let (report, outcome) = mpsc::channel();
    std::thread::spawn(move || {
        let launched = Service::launch(empty_paths()).map(|(service, first)| {
            let current = service.current().map(|snapshot| snapshot.revision);
            (first.revision, current)
        });
        let _ = report.send(launched.ok());
    });
    assert_eq!(
        outcome.recv_timeout(Duration::from_secs(10)),
        Ok(Some((1, Some(1)))),
        "a live worker's first snapshot did not reach the launch"
    );
}

// A switch whose new catalog is still being written is not stable yet, and the folders the old
// language's watch covers never report the new one's later edits, so the worker must retry on the poll.
#[test]
fn a_language_switch_lands_while_its_catalog_is_still_being_written() {
    let root = crate::dirwake::tests::scratch("locale-switch");
    let roster = root.join("omarchy/default/i18n");
    std::fs::create_dir_all(&roster).unwrap();
    std::fs::write(roster.join("locales.json"), r#"{"en":{},"de":{}}"#).unwrap();
    std::fs::create_dir_all(root.join("catalogs/en")).unwrap();
    std::fs::create_dir_all(root.join("catalogs/de")).unwrap();
    let german = root.join("catalogs/de/flea.json");
    std::fs::write(&german, "{}").unwrap();
    std::fs::write(root.join("language"), "en\n").unwrap();
    let paths = Paths {
        omarchy: root.join("omarchy"),
        language_file: root.join("language"),
        catalogs: vec![root.join("catalogs")],
    };
    let (service, first) = Service::launch(paths).unwrap();
    assert_eq!(first.catalog.language(), "en");
    let writing = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let writer = {
        let (writing, german) = (writing.clone(), german.clone());
        std::thread::spawn(move || {
            for edit in 0u64.. {
                if !writing.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                std::fs::write(&german, format!("{{\"edit\":\"{edit}\"}}")).unwrap();
            }
        })
    };
    std::fs::write(root.join("language"), "de\n").unwrap();
    std::thread::sleep(Duration::from_millis(400));
    writing.store(false, std::sync::atomic::Ordering::Relaxed);
    writer.join().unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let language = || service.current().map(|s| s.catalog.language().to_owned());
    while language().as_deref() != Some("de") && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        language().as_deref(),
        Some("de"),
        "the switch stalled on the old language"
    );
    drop(service);
    let _ = std::fs::remove_dir_all(&root);
}
