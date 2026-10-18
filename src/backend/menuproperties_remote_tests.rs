use super::*;
use crate::backend::testdir::TestDir;
use crate::json::field_str;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};

thread_local! {
    pub(super) static PANIC_ON_PROGRESS: RefCell<Option<Arc<AtomicBool>>> = const { RefCell::new(None) };
}

#[test]
fn properties_worker_panicking_mid_walk_reports_an_error_instead_of_a_floor() {
    let sandbox = TestDir::new("properties-worker-panic");
    let folder = sandbox.dir("folder");
    sandbox.file("folder/child", "counted before the worker panics");
    let plain = sandbox.file("plain", "abc");
    let selected = [folder, plain]
        .iter()
        .map(|path| Selected::inspect(path.to_str().unwrap()).unwrap())
        .collect::<Vec<_>>();
    let body = format!(
        "1 0 8:1 / / rw - ext4 /dev/a rw\n2 1 0:45 / {} rw - nfs n:/s rw\n",
        sandbox.path().display()
    );
    let reached_walk = Arc::new(AtomicBool::new(false));
    PANIC_ON_PROGRESS.with(|slot| *slot.borrow_mut() = Some(Arc::clone(&reached_walk)));
    let (tx, rx) = channel();
    let cancel = Cancellation::default().next();
    assert!(serve_bounded(
        r#"{"op":"properties","id":7}"#,
        &selected,
        &tx,
        &cancel,
        &body,
        Duration::from_secs(5),
    ));
    assert!(
        reached_walk.load(Ordering::SeqCst),
        "the real walk must reach its progress callback"
    );
    let lines = rx
        .try_iter()
        .map(|reply| match reply {
            OpMsg::Meta { line } => line,
            _ => panic!("Properties emits only menu replies"),
        })
        .collect::<Vec<_>>();
    assert!(
        lines
            .iter()
            .any(|line| field_bool(line, "ok") && !field_bool(line, "done")),
        "{lines:?}"
    );
    let last = lines.last().unwrap();
    assert!(
        !field_bool(last, "ok"),
        "a dead worker must fail loudly: {last}"
    );
    assert_eq!(
        field_str(last, "error").as_deref(),
        Some("The selected items stopped answering.")
    );
    assert!(
        !last.contains("\"done\""),
        "a worker failure cannot look completed: {last}"
    );
    assert!(
        !iomount::is_stuck(sandbox.path(), iomount::STUCK_TTL),
        "a dead worker must not mark its mount stuck"
    );
}
