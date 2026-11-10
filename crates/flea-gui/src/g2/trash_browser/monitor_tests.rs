use super::*;

// The pane starts its function-pointer seam on a background executor; return the pipe to its test.
static WATCH_INPUT: std::sync::Mutex<Option<std::process::ChildStdin>> =
    std::sync::Mutex::new(None);

pub(crate) fn pane_watch() -> Started {
    let query = vec![
        "sh".into(),
        "-c".into(),
        "echo 'trash::item-count: 3'".into(),
    ];
    let (monitor, receiver, input) = controlled(query);
    assert!(WATCH_INPUT.lock().unwrap().replace(input).is_none());
    Ok((monitor, receiver))
}

pub(crate) fn watch_input() -> std::process::ChildStdin {
    WATCH_INPUT.lock().unwrap().take().expect("watch started")
}

pub(crate) fn controlled(
    query: Vec<String>,
) -> (Monitor, Receiver<Heard>, std::process::ChildStdin) {
    let mut child = Command::new("cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let input = child.stdin.take().unwrap();
    let (monitor, receiver) = Monitor::from_child(child, query);
    (monitor, receiver, input)
}

fn next(receiver: &Receiver<Heard>) -> Heard {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match receiver.try_recv() {
            Ok(message) => return message,
            Err(async_channel::TryRecvError::Empty) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(1));
            }
            other => panic!("monitor did not answer: {other:?}"),
        }
    }
}

#[test]
fn a_real_change_queued_with_the_initial_refresh_re_reads_the_open_trash() {
    let (changes, changed) = mpsc::channel();
    let (heard, receiver) = async_channel::unbounded();
    changes.send(Refresh::Initial).unwrap();
    changes.send(Refresh::Changed).unwrap();
    let query = vec![
        "sh".into(),
        "-c".into(),
        "echo 'trash::item-count: 1'".into(),
    ];
    let worker = std::thread::spawn(move || run(changed, query, heard));
    assert_eq!(next(&receiver), Heard::Changed);
    assert_eq!(next(&receiver), Heard::Count(1));
    drop(changes);
    worker.join().unwrap();
}

#[test]
fn the_initial_refresh_alone_only_reads_the_count() {
    let (changes, changed) = mpsc::channel();
    let (heard, receiver) = async_channel::unbounded();
    changes.send(Refresh::Initial).unwrap();
    let query = vec![
        "sh".into(),
        "-c".into(),
        "echo 'trash::item-count: 2'".into(),
    ];
    let worker = std::thread::spawn(move || run(changed, query, heard));
    assert_eq!(next(&receiver), Heard::Count(2));
    assert!(
        receiver.try_recv().is_err(),
        "startup alone is not a change"
    );
    changes.send(Refresh::Changed).unwrap();
    assert_eq!(next(&receiver), Heard::Changed);
    assert_eq!(next(&receiver), Heard::Count(2));
    drop(changes);
    worker.join().unwrap();
}

#[test]
fn several_real_changes_in_the_first_burst_invalidate_once_even_if_the_count_fails() {
    let (changes, changed) = mpsc::channel();
    let (heard, receiver) = async_channel::unbounded();
    changes.send(Refresh::Initial).unwrap();
    for _ in 0..3 {
        changes.send(Refresh::Changed).unwrap();
    }
    let query = vec!["sh".into(), "-c".into(), "exit 1".into()];
    let worker = std::thread::spawn(move || run(changed, query, heard));
    assert_eq!(next(&receiver), Heard::Changed);
    assert_eq!(next(&receiver), Heard::Failed("trash.monitor.count_failed"));
    assert!(receiver.try_recv().is_err(), "one invalidation per burst");
    drop(changes);
    worker.join().unwrap();
}
