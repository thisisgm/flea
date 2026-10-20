use super::english;
use crate::ui_logic_fb1::status::{
    arm_left, arm_live, centre_role, centre_text, error_here, errors_at, notice_shown,
    transient_of, with_error, Arm, ErrorEntry, Hint, Note, Role, Slot,
};

fn slot<'a>(edit: impl FnOnce(&mut Slot<'a>)) -> Slot<'a> {
    let mut slot = Slot::default();
    edit(&mut slot);
    slot
}

#[test]
fn the_centre_precedence_is_error_then_activity_then_search_then_notice() {
    let quiet = Slot::default();
    assert_eq!(
        (centre_text(&quiet), centre_role(&quiet)),
        ("", Role::Foreground)
    );
    let searching = slot(|s| {
        s.searching = true;
        s.search_line = "3 found · Searching, 12 scanned";
    });
    assert_eq!(centre_text(&searching), "3 found · Searching, 12 scanned");
    assert_eq!(centre_role(&searching), Role::Foreground);
    let working = slot(|s| {
        s.sticky_here = true;
        s.sticky = "Compressing 2 of 5";
    });
    assert_eq!(
        (centre_text(&working), centre_role(&working)),
        ("Compressing 2 of 5", Role::Foreground)
    );
    let both = slot(|s| {
        s.searching = true;
        s.search_line = "3 found";
        s.sticky_here = true;
        s.sticky = "Copy 1 item to dest";
    });
    assert_eq!(
        (centre_text(&both), centre_role(&both)),
        ("Copy 1 item to dest", Role::Foreground)
    );
    let error_over_search = slot(|s| {
        s.transient = "Copy failed: c.txt · already exists";
        s.transient_is_error = true;
        s.searching = true;
        s.search_line = "3 found in 1.6 s";
    });
    assert_eq!(
        centre_text(&error_over_search),
        "Copy failed: c.txt · already exists"
    );
    assert_eq!(centre_role(&error_over_search), Role::Error);
    let failed_working = slot(|s| {
        s.transient = "Convert failed: no encoder";
        s.transient_is_error = true;
        s.sticky_here = true;
        s.sticky = "Converting 1 of 3";
    });
    assert_eq!(
        (centre_text(&failed_working), centre_role(&failed_working)),
        ("Convert failed: no encoder", Role::Error)
    );
    let notice_behind_search = slot(|s| {
        s.transient = "Moved 4 items to Trash";
        s.searching = true;
        s.search_line = "3 found · Searching, 12 scanned";
    });
    assert_eq!(
        centre_text(&notice_behind_search),
        "3 found · Searching, 12 scanned"
    );
    assert!(!error_here(&notice_behind_search));
    assert!(error_here(&error_over_search));
    assert_eq!(
        centre_role(&slot(|s| s.transient = "Moved 4 items to Trash")),
        Role::Foreground
    );
}

#[test]
fn a_prompt_ranks_only_while_its_arm_lives() {
    let prompt = "Press d again to move it to Trash, or Delete on its own.";
    let older = vec![ErrorEntry {
        text: "Copy failed: photo.heic · disk full".into(),
        detail: "No space left on device".into(),
        ..ErrorEntry::default()
    }];
    assert!(transient_of(&older, prompt, "", false).is_error);
    let dead = slot(|s| {
        s.transient = prompt;
        s.searching = true;
        s.search_line = "3 found in 1.6 s";
    });
    assert_eq!(centre_text(&dead), "3 found in 1.6 s");
    let arm = Arm {
        owner: 1,
        stamp: 1000,
    };
    let live = transient_of(&older, "", prompt, arm_live(Some(arm), 1, 1000));
    assert_eq!(
        (live.text.as_str(), live.is_error, live.detail.as_str()),
        (prompt, false, "")
    );
    for edit in [
        (|s: &mut Slot| {
            s.searching = true;
            s.search_line = "3 found";
        }) as fn(&mut Slot),
        |s| {
            s.sticky_here = true;
            s.sticky = "Copy 1 item to dest";
        },
    ] {
        let mut armed = slot(|s| {
            s.transient = prompt;
            s.arm_live = true;
        });
        edit(&mut armed);
        assert_eq!(centre_text(&armed), prompt);
    }
    assert_eq!(
        transient_of(&older, "Copied 1 item", "", false).text,
        older[0].text
    );
    assert!(!arm_live(Some(arm), 2, 1000));
    assert!(!arm_live(Some(arm), 1, 1001));
    assert!(!arm_live(Some(Arm { owner: 1, stamp: 0 }), 1, 0));
    assert!(!arm_live(None, 1, 1000));
    assert_eq!(
        transient_of(&older, "", prompt, false).detail,
        "No space left on device"
    );
    let said = Some(Arm {
        owner: 3,
        stamp: 1000,
    });
    assert_eq!(
        (
            arm_left(said, 1000 + 3999, 4000),
            arm_left(said, 5000, 4000),
            arm_left(said, 1500, 4000)
        ),
        (1, 0, 3500)
    );
    assert!(!notice_shown(&slot(|s| {
        s.transient = prompt;
        s.arm_live = true;
    })));
    assert!(notice_shown(
        &slot(|s| s.transient = "Renamed to notes.txt")
    ));
    assert!(!notice_shown(&slot(|s| {
        s.transient = "Renamed to notes.txt";
        s.searching = true;
        s.search_line = "3 found";
    })));
}

#[test]
fn a_placed_refusal_replaces_its_own_copy_and_leaves_with_its_place() {
    let share = "/run/user/1000/gvfs/mtp:host=SAMSUNG_Android";
    let no_trash = "This location has no Trash";
    let older = ErrorEntry {
        text: "Copy failed: photo.heic · disk full".into(),
        detail: "No space left on device".into(),
        ..ErrorEntry::default()
    };
    let mut refused = vec![older.clone()];
    with_error(&mut refused, no_trash, "", Hint::Deletes, Some(share));
    with_error(&mut refused, no_trash, "", Hint::Deletes, Some(share));
    assert_eq!(refused.len(), 2);
    assert_eq!(refused[1].place, share);
    let mut queued = Vec::new();
    with_error(&mut queued, no_trash, "", Hint::Deletes, Some(share));
    with_error(
        &mut queued,
        "Copy failed: a.txt · disk full",
        "",
        Hint::None,
        None,
    );
    with_error(&mut queued, no_trash, "", Hint::Deletes, Some(share));
    let texts: Vec<_> = queued.iter().map(|e| e.text.as_str()).collect();
    assert_eq!(texts, [no_trash, "Copy failed: a.txt · disk full"]);
    let mut plain = Vec::new();
    with_error(&mut plain, "Copy failed", "", Hint::None, None);
    assert_eq!(plain[0].place, "");
    let kept = refused.clone();
    assert!(!errors_at(&mut refused, share));
    assert_eq!(refused, kept);
    assert!(errors_at(&mut refused, "/run/user/1000/gvfs"));
    assert_eq!(refused, vec![older]);
    let mut only = Vec::new();
    with_error(&mut only, no_trash, "", Hint::Deletes, Some(share));
    errors_at(&mut only, "/run/user/1000/gvfs");
    assert_eq!(transient_of(&only, "", "", false).text, "");
}

#[test]
fn hints_are_typed_and_read_the_running_preset() {
    let catalog = english();
    assert_eq!(Hint::Undo.text(&catalog, "default").undo, "z undoes");
    assert_eq!(Hint::Paste.text(&catalog, "default").rest, "p pastes");
    assert_eq!(
        Hint::Deletes.text(&catalog, "default").rest,
        "shift-delete deletes"
    );
    assert!(Hint::None.text(&catalog, "default").is_empty());
    let background = Hint::Note(Note::Background, true).text(&catalog, "default");
    assert_eq!(
        background.joined(),
        "rclone uploads them in the background · z undoes"
    );
    let unconfirmed = Hint::Note(Note::Unconfirmed, true).text(&catalog, "default");
    assert_eq!(
        unconfirmed.joined(),
        "copied, but the drive did not confirm the folder · z undoes"
    );
    assert_eq!(
        Hint::Note(Note::Background, false)
            .text(&catalog, "default")
            .joined(),
        "rclone uploads them in the background"
    );
    assert!(Hint::Undo.has_undo("default"));
    assert!(Hint::Note(Note::Background, true).has_undo("default"));
    assert!(Hint::Note(Note::Unconfirmed, true).has_undo("default"));
    assert!(!Hint::Paste.has_undo("default"));
    assert!(!Hint::None.has_undo("default"));
    assert_eq!(
        Note::from_wire("rclone uploads them in the background"),
        Some(Note::Background)
    );
    assert_eq!(
        Note::from_wire("copied, but the drive did not confirm the folder"),
        Some(Note::Unconfirmed)
    );
    assert_eq!(Note::from_wire("1 failed"), None);
}
