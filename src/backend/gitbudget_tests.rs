use super::*;
use crate::backend::gitstatus::parse_with;
use std::time::Duration;

#[test]
fn git_allocation_budget_refuses_before_growth_and_checks_overflow() {
    let mut budget = Budget::new(Instant::now() + Duration::from_secs(5), &|| false);
    budget.limit = 128;
    let mut bytes = Vec::<u8>::new();
    budget.reserve(&mut bytes, 64).unwrap();
    bytes.extend_from_slice(&[1; 64]);
    let before = (bytes.capacity(), budget.used.get());
    assert_eq!(
        budget.reserve(&mut bytes, 1).unwrap_err(),
        "Git status snapshot is too large."
    );
    assert_eq!((bytes.capacity(), budget.used.get()), before);
    assert_eq!(bytes, vec![1; 64]);
    assert!(budget.claim(usize::MAX).is_err());
    assert!(budget.reserve(&mut bytes, usize::MAX).is_err());
    assert_eq!((bytes.capacity(), budget.used.get()), before);
}

#[test]
fn git_aggregate_budget_includes_paths_originals_branch_and_index() {
    for retained in [false, true] {
        let mut input = b"# branch.head main\0".to_vec();
        for index in 0..32 {
            let records = if retained {
                format!("1 D. N... 100644 000000 000000 a 0 kept/{index:03}\0? kept/{index:03}\0")
            } else {
                format!("2 R. N... 1 1 1 a b R100 new/{index:03}\0old/{index:03}\0")
            };
            input.extend_from_slice(records.as_bytes());
        }
        let complete = Budget::new(Instant::now() + Duration::from_secs(5), &|| false);
        complete.claim(input.len()).unwrap();
        let snapshot = parse_with(&input, &complete).unwrap();
        assert_eq!(snapshot.changes.len(), 32);
        let required = complete.used.get();
        for limit in [input.len(), required - 1] {
            let mut budget = Budget::new(Instant::now() + Duration::from_secs(5), &|| false);
            budget.limit = limit;
            budget.claim(input.len()).unwrap();
            assert_eq!(
                parse_with(&input, &budget).unwrap_err(),
                "Git status snapshot is too large."
            );
            assert!(budget.used.get() <= limit);
        }
        let mut exact = Budget::new(Instant::now() + Duration::from_secs(5), &|| false);
        exact.limit = required;
        exact.claim(input.len()).unwrap();
        assert_eq!(parse_with(&input, &exact).unwrap().changes.len(), 32);
    }
}

#[test]
fn git_cancellation_interrupts_a_single_long_record_before_allocation() {
    let mut input = b"# branch.head main\0? ".to_vec();
    input.extend_from_slice(&vec![b'a'; 65536]);
    input.push(0);
    let polls = Cell::new(0);
    let cancelled = || {
        polls.set(polls.get() + 1);
        polls.get() >= 12
    };
    let budget = Budget::new(Instant::now() + Duration::from_secs(5), &cancelled);
    assert_eq!(
        parse_with(&input, &budget).unwrap_err(),
        "Git status was cancelled."
    );
    assert_eq!(polls.get(), 12);
    assert!(budget.used.get() < 256);
}

#[test]
fn git_deadline_expires_during_record_scanning() {
    let polls = Cell::new(0);
    let checkpoint = || {
        polls.set(polls.get() + 1);
        if polls.get() == 12 {
            std::thread::sleep(Duration::from_millis(3));
        }
        false
    };
    let budget = Budget::new(Instant::now() + Duration::from_millis(2), &checkpoint);
    let mut input = b"# branch.head main\0? ".to_vec();
    input.extend_from_slice(&vec![b'a'; 65536]);
    input.push(0);
    assert_eq!(
        parse_with(&input, &budget).unwrap_err(),
        "Git status timed out."
    );
}

#[test]
fn git_sort_is_interruptible_and_orders_without_scratch_storage() {
    let budget = Budget::new(Instant::now() + Duration::from_secs(5), &|| false);
    for length in 0..130 {
        let mut values: Vec<_> = (0..length).map(|i| ((i * 37) % 19, i)).rev().collect();
        let mut expected = values.clone();
        expected.sort_unstable();
        sort(&mut values, &budget, Ord::cmp).unwrap();
        assert_eq!(values, expected);
    }
    assert_eq!(budget.used.get(), 0);
    let compared = Cell::new(false);
    let cancelled = || compared.get();
    let budget = Budget::new(Instant::now() + Duration::from_secs(5), &cancelled);
    let mut values = [7, 2, 5, 1, 9];
    assert_eq!(
        sort(&mut values, &budget, |a, b| {
            compared.set(true);
            a.cmp(b)
        })
        .unwrap_err(),
        "Git status was cancelled."
    );
    let budget = Budget::new(Instant::now() + Duration::from_millis(2), &|| false);
    assert_eq!(
        sort(&mut values, &budget, |a, b| {
            std::thread::sleep(Duration::from_millis(3));
            a.cmp(b)
        })
        .unwrap_err(),
        "Git status timed out."
    );
}

#[test]
fn git_branch_replacement_text_matches_lossy_utf8_without_unbudgeted_copies() {
    let budget = Budget::new(Instant::now() + Duration::from_secs(5), &|| false);
    for bytes in [
        b"main".as_slice(),
        b"m\xffa\xe2\x82",
        b"\xe2\x82\xac\xff\xff",
        b"",
    ] {
        assert_eq!(
            budget.branch(bytes).unwrap(),
            String::from_utf8_lossy(bytes)
        );
    }
}
