//! CLOCK_MONOTONIC observations taken by the backend, independent of the sampler's scheduling.
use std::fs;
use std::path::Path;
use std::time::Duration;

// src/backend/searchprior.rs GRACE, slept after run.rs has flushed the listing.
const GRACE: Duration = Duration::from_secs(1);

#[derive(Default)]
pub(crate) struct Timing {
    listed: Option<u64>,
    calls: Vec<u64>,
}

impl Timing {
    pub(crate) fn assert_prior_after_grace(&self, argv: &str) {
        let listed = self.listed.expect("the probe timestamps listing emission");
        assert!(
            !self.calls.is_empty(),
            "the prior has a spawn timestamp: {argv}"
        );
        for &call in &self.calls {
            assert!(
                call >= listed && u128::from(call - listed) >= GRACE.as_nanos(),
                "the prior starts only after GRACE: listed={listed} call={call}; argv={argv}"
            );
        }
    }
}

// Sample input: 9-byte records, a kind byte (b'l' listed, b's' spawn) then a little-endian CLOCK_MONOTONIC nanosecond stamp.
pub(crate) fn read(log: &Path) -> Timing {
    let bytes = fs::read(log.with_extension("events")).expect("backend timestamps");
    let (records, remainder) = bytes.as_chunks::<9>();
    let mut timing = Timing::default();
    for record in records {
        let stamp = u64::from_le_bytes(record[1..].try_into().unwrap());
        match record[0] {
            b'l' => {
                timing.listed.get_or_insert(stamp);
            }
            b's' => timing.calls.push(stamp),
            kind => panic!("unknown timing record {kind}"),
        }
    }
    assert!(remainder.is_empty(), "every timestamp is complete");
    assert!(timing.listed.is_some(), "the backend emitted its listing");
    timing
}

#[test]
fn grace_boundary_is_exact_without_a_sampler_clock() {
    let grace = GRACE.as_nanos() as u64;
    let at_grace = Timing {
        listed: Some(123),
        calls: vec![123 + grace],
    };
    at_grace.assert_prior_after_grace("prior");
    let too_early = Timing {
        listed: Some(123),
        calls: vec![122 + grace],
    };
    assert!(std::panic::catch_unwind(|| too_early.assert_prior_after_grace("prior")).is_err());
    let before_listing = Timing {
        listed: Some(123),
        calls: vec![122],
    };
    assert!(std::panic::catch_unwind(|| before_listing.assert_prior_after_grace("prior")).is_err());
}
