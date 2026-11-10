use super::{Update, Worker};
use crate::g2::device_rail::bfx_net_tests::Root;
use std::time::{Duration, Instant};

fn next(worker: &Worker) -> Update {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(update) = worker.updates.try_recv() {
            if !matches!(update, Update::Corner(_)) {
                return update;
            }
        }
        assert!(
            Instant::now() < until,
            "the live Settings store never answered"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn fxuish2b_live_store_removal_reports_once_and_distinct_bad_bytes_report_then_recover() {
    let root = Root::new();
    let path = root.0.join("ui.json");
    let original = r#"{"places":{"favourites":[{"label":"Keep","path":"/keep"}]}}"#;
    std::fs::write(&path, original).unwrap();
    let worker = Worker::at(path.clone(), root.0.join("no-flea")).unwrap();
    assert!(matches!(next(&worker), Update::Loaded { saved: false, .. }));
    std::fs::rename(&path, root.0.join("retained.json")).unwrap();
    assert!(matches!(
        next(&worker),
        Update::Notice("settings.error.favourites_unread")
    ));
    let until = Instant::now() + Duration::from_millis(450);
    while Instant::now() < until {
        if let Ok(update) = worker.updates.try_recv() {
            assert!(
                matches!(update, Update::Corner(_)),
                "one missing store must report only once"
            );
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    for bad in ["", "{malformed"] {
        std::fs::write(&path, bad).unwrap();
        assert!(matches!(
            next(&worker),
            Update::Notice("settings.error.favourites_unread")
        ));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), bad);
    }
    // A rename lands the whole document at once, so no poll reads a truncated store.
    std::fs::write(root.0.join("recovered.json"), original).unwrap();
    std::fs::rename(root.0.join("recovered.json"), &path).unwrap();
    let Update::Loaded { document, .. } = next(&worker) else {
        panic!("valid state must recover")
    };
    assert_eq!(
        document
            .get("places")
            .and_then(|v| v.get("favourites"))
            .and_then(|v| v.as_array())
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        std::fs::read_to_string(root.0.join("retained.json")).unwrap(),
        original
    );
}
