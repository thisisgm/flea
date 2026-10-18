use super::*;
use crate::backend::gitstatus;

#[test]
fn git_capture_and_parse_share_memory_cancellation_and_deadline() {
    let cancelled = std::cell::Cell::new(false);
    let check = || cancelled.get();
    let budget = Budget::new(Instant::now() + Duration::from_secs(5), &check);
    let bytes = capture_budget(&["/usr/bin/true".into()], &budget).unwrap();
    assert!(bytes.is_empty());
    // A second stdout-sized allocation cannot fit beside the two charged drain buffers.
    assert_eq!(
        budget.claim(32 * 1024 * 1024).unwrap_err(),
        "Git status snapshot is too large."
    );
    cancelled.set(true);
    assert_eq!(
        gitstatus::parse_with(b"# branch.head main\0", &budget).unwrap_err(),
        "Git status was cancelled."
    );

    let deadline = Instant::now() + Duration::from_secs(1);
    let budget = Budget::new(deadline, &|| false);
    capture_budget(&["/usr/bin/true".into()], &budget).unwrap();
    // Reuse the operation's absolute deadline after capture rather than creating a new allowance.
    std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
    assert_eq!(
        gitstatus::parse_with(b"# branch.head main\0", &budget).unwrap_err(),
        "Git status timed out."
    );
}
