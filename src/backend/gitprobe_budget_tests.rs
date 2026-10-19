use super::*;
use crate::backend::gitstatus;

#[test]
fn git_repository_metadata_accepts_its_limit_and_refuses_one_byte_more() {
    const EXPECTED_METADATA_LIMIT: usize = 4096;
    let d = crate::backend::gitfixture::Fixture::new();
    let mut bytes = vec![b'x'; EXPECTED_METADATA_LIMIT];
    d.write("metadata", &bytes);
    assert_eq!(
        small_file(&d.path("metadata")).unwrap().len(),
        EXPECTED_METADATA_LIMIT
    );
    bytes.push(b'x');
    d.write("metadata", &bytes);
    assert_eq!(
        small_file(&d.path("metadata")).unwrap_err(),
        "Git status repository metadata is too large."
    );
}

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
