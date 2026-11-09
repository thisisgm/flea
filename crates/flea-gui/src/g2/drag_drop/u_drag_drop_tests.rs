//! U-DRAG-DROP: a drag back onto its own folder is refused, a held drag lights no hover plate in the list or the rail and a tab press keeps the rename editor; the listed-pane and crumb helpers serve the sibling files.
use super::mounted_tests::{centre, drain, frame, open, Root};
use super::pb_devices_2_tests::{tab_point, tabs};
use super::rail_drop_tests::{home, rail, rail_centre};
use crate::{list::bench, state::Shell};
use gpui::{
    point, px, Modifiers, MouseButton, Pixels, Point, TestAppContext, VisualTestContext,
    WindowHandle,
};

pub(super) fn mounted(cx: &mut TestAppContext, root: &Root) -> WindowHandle<Shell> {
    let (handle, _lines) = open(cx, root.src());
    handle
        .update(cx, |shell, window, cx| shell.follow_file_drags(window, cx))
        .unwrap();
    frame(cx, handle);
    handle
}

// Press on `from`, travel past the slop to `away`, then rest on `back`, still held.
pub(super) fn out_and_back(
    vcx: &mut VisualTestContext,
    from: Point<Pixels>,
    away: Point<Pixels>,
    back: Point<Pixels>,
) {
    vcx.simulate_mouse_move(from, None, Modifiers::default());
    vcx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_move(
        point(from.x + px(3.), from.y + px(3.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    vcx.simulate_mouse_move(away, Some(MouseButton::Left), Modifiers::default());
    vcx.run_until_parked();
    vcx.simulate_mouse_move(
        point(back.x + px(1.), back.y),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    vcx.simulate_mouse_move(back, Some(MouseButton::Left), Modifiers::default());
    vcx.run_until_parked();
}

pub(super) fn status(vcx: &mut VisualTestContext, handle: WindowHandle<Shell>) -> String {
    handle
        .update(vcx, |shell, _, _| {
            shell.panes[0].as_ref().unwrap().status.to_string()
        })
        .unwrap()
}

#[test]
fn ey395_a_folder_dragged_away_and_back_onto_its_own_row_is_refused() {
    let root = Root::new("ey395");
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open(&mut cx, root.src());
    handle
        .update(&mut cx, |shell, window, cx| {
            shell.follow_file_drags(window, cx)
        })
        .unwrap();
    frame(&mut cx, handle);
    let folder = centre(&mut cx, handle, 0);
    let away = point(folder.x + px(90.), folder.y + px(160.));
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    out_and_back(&mut vcx, folder, away, folder);
    drain(&mut lines);
    vcx.simulate_mouse_up(folder, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    assert!(drain(&mut lines)
        .iter()
        .all(|line| !line.contains("transfer") && !line.contains("collisions")));
    assert_eq!(status(&mut vcx, handle), "Already in this folder.");
}

#[test]
fn ey356_a_held_drag_back_on_its_own_row_lights_no_hover_plate() {
    let root = Root::new("ey356");
    let mut cx = bench::real_cx();
    let handle = mounted(&mut cx, &root);
    let folder = centre(&mut cx, handle, 0);
    let file = centre(&mut cx, handle, 1);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    // A hover at rest still lights the row.
    vcx.simulate_mouse_move(file, None, Modifiers::default());
    vcx.simulate_mouse_move(folder, None, Modifiers::default());
    vcx.run_until_parked();
    let hovered = |vcx: &mut VisualTestContext| {
        handle
            .update(vcx, |shell, window, cx| {
                shell.list_states(0, window, cx).hovered
            })
            .unwrap()
    };
    assert_eq!(hovered(&mut vcx), Some(0), "a resting pointer hovers");
    let away = point(folder.x + px(90.), folder.y + px(160.));
    out_and_back(&mut vcx, folder, away, folder);
    assert!(vcx.update(|_, cx| cx.has_active_drag()), "the drag is held");
    assert_eq!(hovered(&mut vcx), None, "the refused own row stays dark");
    vcx.simulate_mouse_up(folder, MouseButton::Left, Modifiers::default());
}

#[test]
fn ey471_a_held_drag_over_the_rail_lights_no_hover_plate_while_held() {
    let root = Root::new("ey471");
    let home = home(&root);
    let dest = format!("{}/alpha", root.src());
    let mut cx = bench::real_cx();
    let (handle, _lines) = open(&mut cx, root.src());
    handle
        .update(&mut cx, |shell, window, cx| {
            tabs(shell, &dest);
            shell.follow_file_drags(window, cx);
        })
        .unwrap();
    frame(&mut cx, handle);
    rail(&mut cx, handle, &home);
    {
        use flea::ui_logic_rail_places::{
            order::{Heading, RailSection},
            rail::{Group, Kind, RailEntry},
        };
        let entry = |label: &str, path: &str, group, kind| RailEntry {
            group,
            kind,
            label: label.into(),
            path: path.into(),
            glyph: "house",
            ..RailEntry::default()
        };
        // A mounted releasable volume draws the eject mark, so the mark's own hover is in reach.
        let volume = RailEntry {
            mounted: Some(true),
            releasable: true,
            ..entry("Archive", &dest, Group::Device, Kind::Volume)
        };
        handle
            .update(&mut cx, |shell, _, cx| {
                let places = shell.rail.as_ref().unwrap().places.clone();
                places.update(cx, |producer, cx| {
                    producer.replace_sections(
                        vec![RailSection {
                            heading: Heading::Key("places.heading.places"),
                            rows: vec![entry("Home", &home, Group::Home, Kind::Home), volume],
                        }],
                        cx,
                    )
                });
            })
            .unwrap();
        frame(&mut cx, handle);
    }
    let tab = handle
        .update(&mut cx, |shell, _, _| tab_point(shell))
        .unwrap();
    let row = rail_centre(&mut cx, handle, 0);
    let mark = handle
        .update(&mut cx, |shell, _, cx| {
            let producer = shell.rail.as_ref().unwrap().places.read(cx);
            let hit = producer
                .laid
                .borrow()
                .row(1)
                .and_then(|row| row.eject_hit());
            hit.map(|hit| point(px(hit.x + hit.width / 2.), px(hit.y + hit.height / 2.)))
        })
        .unwrap()
        .expect("the volume row draws its eject mark");
    let rail_hover = |vcx: &mut VisualTestContext| {
        handle
            .update(vcx, |shell, _, cx| {
                let producer = shell.rail.as_ref().unwrap().places.read(cx);
                let model = producer.model.borrow();
                (model.hovered, model.mark_hovered)
            })
            .unwrap()
    };
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_mouse_move(mark, None, Modifiers::default());
    vcx.run_until_parked();
    assert_eq!(
        rail_hover(&mut vcx),
        (Some(1), Some(1)),
        "with no drag the pointer lights the row and its mark"
    );
    // The tab drag crosses the rail on its way out of the window, as hv1031.tabs.tear-left-hp does.
    vcx.simulate_mouse_move(tab, None, Modifiers::default());
    vcx.simulate_mouse_down(tab, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_move(
        point(tab.x - px(20.), tab.y + px(4.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    assert!(
        vcx.update(|_, cx| cx.has_active_drag()),
        "the tab drag is held"
    );
    vcx.simulate_mouse_move(row, Some(MouseButton::Left), Modifiers::default());
    vcx.simulate_mouse_move(
        point(row.x + px(1.), row.y),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    vcx.run_until_parked();
    assert_eq!(
        rail_hover(&mut vcx),
        (None, None),
        "no rail row lights under a drag"
    );
    vcx.simulate_mouse_move(mark, Some(MouseButton::Left), Modifiers::default());
    vcx.simulate_mouse_move(
        point(mark.x + px(1.), mark.y),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    vcx.run_until_parked();
    assert_eq!(
        rail_hover(&mut vcx),
        (None, None),
        "no eject mark lifts under a drag"
    );
    // Teardown only: the tab leaves the window with the pointer, so the drag ends without a drop here.
    vcx.update(|window, cx| cx.stop_active_drag(window));
}

// The bench window's pane height, as bfx_cols_tests.rs sizes it.
const PANE_HEIGHT: f32 = 646.;
// A pending listing takes its first rows whatever their serial; 2 is the one the pane fixtures stamp.
pub(super) const FIRST_LISTING: usize = 2;

// A coherent pane on `folder`, so the chrome path bar draws its crumbs.
pub(super) fn open_listed(
    cx: &mut TestAppContext,
    folder: String,
) -> (WindowHandle<Shell>, impl FnMut() -> Option<String>) {
    let (handle, _old) = open(cx, folder.clone());
    let (backend, lines) = crate::backend::Backend::test_operation_channel();
    handle
        .update(cx, |shell, window, cx| {
            let rows = shell.panes[0].as_ref().unwrap().pane.held.rows.clone();
            let mut host = crate::window::pane::PaneHost::new(
                folder.clone(),
                None,
                PANE_HEIGHT,
                &shell.locale,
            );
            host.backend = Some(backend);
            host.pane.use_executor(cx.background_executor().clone());
            host.pane
                .receive(flea::protocol::Reply::Listed {
                    path: folder.clone(),
                    total: rows.len(),
                })
                .unwrap();
            host.pane
                .receive(flea::protocol::Reply::Rows {
                    start: 0,
                    listing: FIRST_LISTING,
                    rows,
                })
                .unwrap();
            host.changed(&shell.locale);
            host.nav
                .after_reply(&mut host.pane, &folder, &mut host.selected);
            host.list.borrow_mut().typeface = bench::bench_typeface();
            shell.panes[0] = Some(host);
            shell.path_labels[0] = folder.clone().into();
            shell.launch.paths[0] = folder;
            shell.follow_file_drags(window, cx);
        })
        .unwrap();
    frame(cx, handle);
    frame(cx, handle);
    (handle, lines)
}

// A point on the parent crumb naming `parent` in the chrome strip, with the crumb's folder name as the footer says it.
pub(super) fn crumb(
    cx: &mut TestAppContext,
    handle: WindowHandle<Shell>,
    parent: &str,
) -> (Point<Pixels>, String) {
    handle
        .read_with(cx, |shell, cx| {
            let view = shell.chrome.view.as_ref().unwrap().read(cx).path.read(cx);
            let paint = view.paint.borrow();
            let strip = &view.strips[0];
            let index = strip
                .crumbs()
                .iter()
                .position(|crumb| crumb.path == parent)
                .expect("the parent has a crumb");
            let (left, _) = strip.extent(index).unwrap();
            let bounds = paint.strips[0].unwrap();
            let x = bounds.left()
                + px(crate::path::paint::strip_pad(false, &paint.scale()) + left + 3.);
            (
                point(x, bounds.top() + bounds.size.height / 2.),
                strip.crumbs()[index].text.trim_end_matches('/').to_owned(),
            )
        })
        .unwrap()
}

#[test]
fn ey393_a_tab_dragged_out_while_renaming_is_refused_and_the_editor_stays() {
    let root = Root::new("ey393");
    let dest = format!("{}/alpha", root.src());
    let mut cx = bench::real_cx();
    let (handle, _lines) = open(&mut cx, root.src());
    handle
        .update(&mut cx, |shell, window, cx| {
            tabs(shell, &dest);
            shell.follow_file_drags(window, cx);
        })
        .unwrap();
    frame(&mut cx, handle);
    handle
        .update(&mut cx, |shell, window, cx| {
            shell.panes[0].as_mut().unwrap().pane.cursor = 1;
            shell.dispatch(
                crate::window::dispatch::Order::Action(crate::window::input::ActionEvent::bare(
                    "rename",
                    flea::keys::Context::Listing,
                )),
                window,
                cx,
            );
        })
        .unwrap();
    frame(&mut cx, handle);
    let tab = handle
        .update(&mut cx, |shell, _, _| tab_point(shell))
        .unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_mouse_move(tab, None, Modifiers::default());
    vcx.simulate_mouse_down(tab, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    let editing = |vcx: &mut VisualTestContext| {
        handle
            .update(vcx, |shell, _, _| shell.rename.edit.is_some())
            .unwrap()
    };
    assert!(
        editing(&mut vcx),
        "a press on the tab strip keeps the editor open"
    );
    vcx.simulate_mouse_move(
        point(tab.x, tab.y + px(30.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    // The pointer leaves the window: the platform asks for the tab's payload, which a rename refuses.
    let torn = handle
        .update(&mut vcx, |shell, _, cx| {
            shell.tab_native_payload(cx).is_some()
        })
        .unwrap();
    assert!(!torn, "no window tears off");
    assert_eq!(
        status(&mut vcx, handle),
        "Finish the rename before dragging a tab out."
    );
    assert!(editing(&mut vcx), "the refusal keeps the editor open");
}
