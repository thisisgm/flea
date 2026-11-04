//! PB-DRAG-1: a drop shelf drag redeems its token on the drop, and its held verb is the one fixed at the lift.
use super::mounted_tests::{centre, frame, open, transfer, Root};
use super::router::{self, Native};
use crate::{list::bench, state::Shell};
use flea::ui_logic_dg2::{SHELF_MIME, URILIST};
use gpui::{
    ExternalPaths, FileDropEvent, InputEvent, Modifiers, Pixels, Point, VisualTestContext,
    WindowHandle,
};
use std::sync::Arc;

// The shelf's offer over `at`: its entry's URI and the payload, held or dropped, with the native Shift bit set.
fn shelf(
    vcx: &mut VisualTestContext,
    handle: WindowHandle<Shell>,
    at: Point<Pixels>,
    entry: &str,
    payload: &str,
    dropped: bool,
) {
    let uris = flea::ui_logic_dg1::build_urilist(&[entry.to_owned()]);
    let types = vec![URILIST.to_owned(), SHELF_MIME.to_owned()];
    let get = |mime: &str| match mime {
        URILIST => Some(Arc::<[u8]>::from(uris.as_slice())),
        SHELF_MIME => Some(Arc::<[u8]>::from(payload.as_bytes())),
        _ => None,
    };
    handle
        .update(vcx, |shell, window, cx| {
            shell.follow_file_drags(window, cx);
            assert!(router::shared(cx).land(&Native::new(
                window.window_handle().window_id().as_u64(),
                (f32::from(at.x), f32::from(at.y)),
                dropped,
                &types,
                &get,
                None,
                false,
                true
            )));
        })
        .unwrap();
    vcx.run_until_parked();
}

#[test]
fn pb_drag_1_a_shelf_drop_redeems_its_token_through_collisions() {
    let root = Root::new("shelf-token");
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open(&mut cx, root.src());
    frame(&mut cx, handle);
    let folder = centre(&mut cx, handle, 0);
    let entry = format!("{}/b.txt", root.src());
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    shelf(&mut vcx, handle, folder, &entry, "opaque-token\nmove", true);
    let transfer = transfer(&mut vcx, handle, &mut lines);
    assert!(transfer.contains(r#""shelf":"opaque-token""#), "{transfer}");
    assert!(transfer.contains(r#""paths":[]"#), "{transfer}");
    assert!(transfer.contains(r#""op":"""#), "{transfer}");
    assert!(
        transfer.contains(&format!(r#""dest":"{}/alpha""#, root.src())),
        "{transfer}"
    );
}

// The label a held shelf offer draws over the first folder row while `keys` are held in the window.
fn held_label(name: &str, payload: &str, keys: Modifiers) -> Option<String> {
    let root = Root::new(name);
    let mut cx = bench::real_cx();
    let (handle, _lines) = open(&mut cx, root.src());
    frame(&mut cx, handle);
    let folder = centre(&mut cx, handle, 0);
    let entry = format!("{}/b.txt", root.src());
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_modifiers_change(keys);
    shelf(&mut vcx, handle, folder, &entry, payload, false);
    vcx.update(|window, cx| {
        window.dispatch_event(
            FileDropEvent::Entered {
                position: folder,
                paths: ExternalPaths([std::path::PathBuf::from(&entry)].into_iter().collect()),
            }
            .to_platform_input(),
            cx,
        );
    });
    vcx.run_until_parked();
    frame(&mut vcx, handle);
    handle
        .update(&mut vcx, |shell, window, cx| {
            let states = shell.list_states(0, window, cx);
            assert_eq!(states.drop_target, Some(0));
            states.drop_label.as_deref().map(str::to_owned)
        })
        .unwrap()
}

#[test]
fn pb_drag_1_a_held_shelf_drag_says_its_fixed_verb_whatever_keys_are_held() {
    assert_eq!(
        held_label("shelf-keys", "", Modifiers::shift()).as_deref(),
        Some("move here"),
        "without a token the held Shift decides"
    );
    assert_eq!(
        held_label("shelf-copy", "opaque-token\ncopy", Modifiers::shift()).as_deref(),
        Some("copy here"),
        "Shift asks for a move, but the shelf fixed a copy"
    );
    assert_eq!(
        held_label("shelf-move", "opaque-token\nmove", Modifiers::control()).as_deref(),
        Some("move here"),
        "Ctrl asks for a copy, but the shelf fixed a move"
    );
}
