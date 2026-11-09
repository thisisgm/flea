//! The ShareBrowser overlay state, its IPC shape, and the click picks.
use super::*;

#[test]
fn the_browser_opens_moves_and_reports_its_ipc_shape() {
    let mut send = ProviderSend::default();
    assert!(!send.browser_open_ipc());
    assert_eq!(send.browser_cursor_ipc(), 0);
    assert_eq!(send.browser_entries_ipc(), "");
    send.browser_open(
        "smb://nas",
        "nas",
        &["media".to_string(), "docs".to_string()],
    );
    assert!(send.browser_open_ipc());
    assert_eq!(send.browser_entries_ipc(), "media\ndocs");
    send.browser_move(1);
    assert_eq!(
        send.browser_activate(),
        Some(("smb://nas/docs/".to_string(), "docs".to_string()))
    );
    assert!(send.browser_close());
    assert!(!send.browser_close());
}

// A click on a row picks that share and moves the cursor there; a gap, a missing row or a shut overlay picks nothing.
#[test]
fn a_click_picks_its_row_and_a_gap_picks_nothing() {
    let mut send = ProviderSend::default();
    assert_eq!(
        send.browser_click(Some(0)),
        None,
        "a shut overlay picks nothing"
    );
    send.browser_open(
        "smb://nas",
        "nas",
        &["media".to_string(), "docs".to_string()],
    );
    assert_eq!(send.browser_click(None), None, "a gap picks nothing");
    assert_eq!(
        send.browser_click(Some(5)),
        None,
        "past the last share picks nothing"
    );
    assert_eq!(send.browser_cursor_ipc(), 0);
    assert_eq!(
        send.browser_click(Some(1)),
        Some(("smb://nas/docs/".to_string(), "docs".to_string()))
    );
    assert_eq!(send.browser_cursor_ipc(), 1);
    assert!(send.browser_close());
    assert_eq!(
        send.browser_click(Some(0)),
        None,
        "a shut overlay picks nothing after a close"
    );
}
