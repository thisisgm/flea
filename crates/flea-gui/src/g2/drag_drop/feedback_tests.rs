//! Held file drags paint a flock and publish the destination before any transfer.
use super::mounted_tests::{centre, frame, open, Root};
use gpui::{point, px, AppContext, Modifiers, MouseButton, TestAppContext, VisualTestContext};

#[test]
fn vf_look_held_drag_has_a_native_flock_and_folder_feedback() {
    let cx = &mut crate::list::bench::real_cx();
    let root = Root::new("vf-look");
    let (handle, _) = open(cx, root.src());
    frame(cx, handle);
    let from = centre(cx, handle, 1);
    let to = centre(cx, handle, 0);
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    visual.simulate_mouse_move(
        point(from.x + px(12.), from.y),
        Some(MouseButton::Left),
        Modifiers::none(),
    );
    visual.simulate_mouse_move(
        to,
        Some(MouseButton::Left),
        Modifiers {
            shift: true,
            ..Modifiers::none()
        },
    );
    visual.run_until_parked();
    visual.simulate_mouse_move(
        to,
        Some(MouseButton::Left),
        Modifiers {
            shift: true,
            ..Modifiers::none()
        },
    );
    visual.run_until_parked();
    handle
        .update(&mut visual, |shell, window, cx| {
            let payload = shell
                .file_drag_payload(window, cx)
                .expect("the drag lifted a file");
            assert!(
                payload.image().is_some(),
                "the flock must follow the pointer outside this window"
            );
            assert_eq!(
                super::join::flock(cx).read(cx).family,
                shell.active_typeface().font.family,
                "the platform image names its rows in the listing's face"
            );
            let states = shell.list_states(0, window, cx);
            assert_eq!(states.drop_target, Some(0));
            assert_eq!(states.drop_label.as_deref(), Some("move here"));
            let lines = shell.status_messages(cx, |messages, _| {
                messages
                    .activities()
                    .iter()
                    .map(|a| a.text.clone())
                    .collect::<Vec<_>>()
            });
            let alpha = "Move 1 item to alpha · ctrl copies, shift moves";
            assert!(lines.iter().any(|s| s == alpha), "{lines:?}");
            let lines = |shell: &mut crate::state::Shell,
                         cx: &mut gpui::Context<crate::state::Shell>| {
                shell.status_messages(cx, |messages, _| {
                    messages
                        .activities()
                        .iter()
                        .map(|a| a.text.clone())
                        .collect::<Vec<_>>()
                })
            };
            // The rail writes the same activity owner without touching the cached pane line.
            shell.say_held_drag(window, cx);
            assert!(!lines(shell, cx).iter().any(|s| s == alpha));
            shell.refresh_file_cue(window, cx);
            let relined = lines(shell, cx);
            assert!(relined.iter().any(|s| s == alpha), "{relined:?}");
            assert!(shell.escape_file_drag(window, cx));
            assert!(shell.list_states(0, window, cx).drop_target.is_none());
            let after = lines(shell, cx);
            assert!(!after.iter().any(|s| s.contains("alpha")), "{after:?}");
        })
        .unwrap();
}

#[gpui::test]
fn vf_look_native_held_drag_lights_the_destination_without_a_transfer(cx: &mut TestAppContext) {
    use super::router::{self, Native};
    use flea::ui_logic_drag_drop::{Marker, ROWS_MIME, URILIST};
    use gpui::{ExternalPaths, FileDropEvent, InputEvent, NativeDragAction};
    use std::sync::Arc;
    let root = Root::new("vf-look-native");
    let (handle, mut requests) = open(cx, root.src());
    frame(cx, handle);
    let to = centre(cx, handle, 0);
    let source = format!("{}/b.txt", root.src());
    let marker = Marker {
        instance: "another-process".into(),
        source: root.src(),
        rows: vec![1],
        shift: true,
        deletable: true,
        copy: false,
        dev: 0,
        listing: 2,
    }
    .encode();
    let uris = flea::ui_logic_file_clipboard::build_urilist(std::slice::from_ref(&source));
    let types = vec![ROWS_MIME.to_owned(), URILIST.to_owned()];
    let get = |mime: &str| match mime {
        ROWS_MIME => Some(Arc::<[u8]>::from(marker.as_bytes())),
        URILIST => Some(Arc::<[u8]>::from(uris.as_slice())),
        _ => None,
    };
    handle
        .update(cx, |shell, window, cx| {
            shell.follow_file_drags(window, cx);
            assert!(router::shared(cx).land(&Native::new(
                window.window_handle().window_id().as_u64(),
                (f32::from(to.x), f32::from(to.y)),
                false,
                &types,
                &get,
                Some(NativeDragAction::Move),
                false,
                true
            )));
        })
        .unwrap();
    cx.update_window(handle.into(), |_, window, cx| {
        window.dispatch_event(
            FileDropEvent::Entered {
                position: to,
                paths: ExternalPaths([std::path::PathBuf::from(&source)].into_iter().collect()),
            }
            .to_platform_input(),
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    frame(cx, handle);
    handle
        .update(cx, |shell, window, cx| {
            let states = shell.list_states(0, window, cx);
            assert_eq!(states.drop_target, Some(0));
            assert_eq!(states.drop_label.as_deref(), Some("move here"));
        })
        .unwrap();
    cx.update_window(handle.into(), |_, window, cx| {
        window.dispatch_event(FileDropEvent::Exited.to_platform_input(), cx);
    })
    .unwrap();
    cx.run_until_parked();
    frame(cx, handle);
    handle
        .update(cx, |shell, window, cx| {
            assert!(shell.list_states(0, window, cx).drop_target.is_none())
        })
        .unwrap();
    assert!(
        !std::iter::from_fn(&mut requests)
            .any(|line| line.contains("collisions") || line.contains("transfer")),
        "holding and cancelling never sends a write"
    );
}
