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
