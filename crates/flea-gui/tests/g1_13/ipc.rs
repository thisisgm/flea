//! The strips' IPC readers answer the registry names with the snapshot's words.
use flea_gui::ipc::{
    strips::{Snapshot, StripsAdapter},
    Adapter,
};

fn snapshot() -> Snapshot {
    Snapshot {
        path: "/fx/v/work".into(),
        total: 9,
        selected_count: 0,
        listing_state: "ready".into(),
        filesystem: "tmpfs · 64.1 MB free".into(),
        counts: "1 of 9 shown".into(),
        primary: "1 of 9 shown".into(),
        secondary: " · esc clears".into(),
        is_error: false,
        detail: String::new(),
        busy: false,
        activity_state: "{\"activities\":[]}".into(),
    }
}

fn read(adapter: &StripsAdapter, function: &str) -> Option<String> {
    adapter.call(function, &[]).map(|reply| {
        assert_eq!(reply.code, 0);
        // Reply::value terminates stdout with one newline, as the sr1 adapter tests pin.
        reply
            .stdout
            .strip_suffix('\n')
            .unwrap_or(&reply.stdout)
            .to_owned()
    })
}

#[test]
fn every_served_name_is_registered() {
    for function in [
        "statusPrimary",
        "statusSecondary",
        "statusError",
        "statusDetail",
        "statusBusy",
        "statusActivityState",
        "statusFooterState",
    ] {
        assert!(
            flea::ipc::registry::validate("flea", function, &[]).is_ok(),
            "{function} is not a registered IPC function"
        );
    }
}

#[test]
fn the_readers_carry_the_snapshot_through() {
    let snapshot = snapshot();
    let adapter = StripsAdapter {
        snapshot: &snapshot,
    };
    assert_eq!(
        read(&adapter, "statusPrimary").as_deref(),
        Some("1 of 9 shown")
    );
    assert_eq!(
        read(&adapter, "statusSecondary").as_deref(),
        Some(" · esc clears")
    );
    assert_eq!(read(&adapter, "statusError").as_deref(), Some("false"));
    assert_eq!(read(&adapter, "statusDetail").as_deref(), Some(""));
    assert_eq!(read(&adapter, "statusBusy").as_deref(), Some("false"));
    assert_eq!(
        read(&adapter, "statusActivityState").as_deref(),
        Some("{\"activities\":[]}")
    );
}

#[test]
fn the_footer_names_the_host_fields() {
    let snapshot = snapshot();
    let adapter = StripsAdapter {
        snapshot: &snapshot,
    };
    let footer = read(&adapter, "statusFooterState").unwrap();
    for part in [
        "\"path\":\"/fx/v/work\"",
        "\"total\":9",
        "\"selected\":0",
        "\"listingState\":\"ready\"",
        "\"filesystem\":\"tmpfs · 64.1 MB free\"",
        "\"counts\":\"1 of 9 shown\"",
    ] {
        assert!(footer.contains(part), "{footer} misses {part}");
    }
}

#[test]
fn an_error_reads_as_an_error() {
    let snapshot = Snapshot {
        is_error: true,
        primary: "Copy failed".into(),
        detail: "disk full".into(),
        ..snapshot()
    };
    let adapter = StripsAdapter {
        snapshot: &snapshot,
    };
    assert_eq!(
        read(&adapter, "statusPrimary").as_deref(),
        Some("Copy failed")
    );
    assert_eq!(read(&adapter, "statusError").as_deref(), Some("true"));
    assert_eq!(read(&adapter, "statusDetail").as_deref(), Some("disk full"));
}

#[test]
fn producer_and_theme_names_stay_unserved() {
    let snapshot = snapshot();
    let adapter = StripsAdapter {
        snapshot: &snapshot,
    };
    assert!(read(&adapter, "filterStamps").is_none());
    assert!(read(&adapter, "statusColor").is_none());
    assert!(read(&adapter, "noSuchFunction").is_none());
}
