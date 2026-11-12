//! Arbitration and work receipts for the real overlay, independent of elapsed wall time.
use super::*;
use gpui::{point, px, size};

#[derive(Default)]
pub(crate) struct Probe(pub(super) RefCell<std::rc::Weak<RefCell<State>>>);
impl gpui::Global for Probe {}

pub(crate) fn diagnostic(
    shell: &crate::state::Shell,
    window: &gpui::Window,
    cx: &gpui::App,
) -> String {
    let owner = cx.global::<Probe>().0.borrow().upgrade().unwrap();
    let owner = owner.borrow();
    let host = shell.panes[0].as_ref().unwrap();
    let mut tracks = Vec::new();
    if let Some(columns) = &host.columns {
        for track in columns.area.tracks().iter() {
            if let Some(paint) = columns.painters.get(track.slot) {
                let paint = paint.borrow();
                if let Some(drawn) = &paint.drawn {
                    tracks.push(format!(
                        "{:?}:enabled={},hit={},current={},listing={:?},candidate={}",
                        track.slot,
                        drawn.view().enabled,
                        drawn.bounds.contains(&window.mouse_position()),
                        Rc::ptr_eq(&drawn.tracks, columns.area.tracks()),
                        drawn.held.as_ref().map(|held| held.listing),
                        super::columns::candidate(
                            &paint,
                            window.mouse_position(),
                            host.pane.held.listing,
                            window.viewport_size().width - px(14.),
                            1
                        )
                        .is_some()
                    ));
                }
            }
        }
    }
    format!("active={} mouse={:?} owner_pointer={:?} pressed={} suppressed={} source={} blocked={} busy={} flash={} tracks={tracks:?}",
        window.is_window_active(), window.mouse_position(), owner.pointer, owner.pressed,
        owner.suppressed, shell.name_tip_candidate(window, cx).is_some(), shell.input_blocked(cx),
        host.pane.busy(), shell.flash_covers(0))
}

pub(crate) fn receipt(cx: &gpui::App) -> (bool, usize, usize, bool) {
    let owner = cx
        .global::<Probe>()
        .0
        .borrow()
        .upgrade()
        .expect("mounted tip owner");
    let owner = owner.borrow();
    (
        owner.shaped.is_some(),
        owner.names_shaped,
        owner.tips_built,
        owner.timer.is_some(),
    )
}

pub(crate) fn assert_foreground(cx: &gpui::App) {
    let owner = cx.global::<Probe>().0.borrow().upgrade().unwrap();
    let owner = owner.borrow();
    let theme = cx
        .try_global::<crate::palette::Theme>()
        .copied()
        .unwrap_or_default();
    assert_eq!(owner.candidate.as_ref().unwrap().ink, theme.foreground);
}

fn candidate() -> Candidate {
    let row = Bounds::new(point(px(46.), px(31.)), size(px(700.), px(31.)));
    let theme = crate::palette::Theme::default();
    Candidate {
        identity: (1, 2, 0, 3),
        epoch: 2,
        name: "long-name.md".into(),
        font: gpui::font("monospace"),
        size: px(14.),
        row,
        hit: row,
        clip: row,
        text_x: px(46.),
        height: px(23.),
        pad: px(7.),
        ground: theme.background,
        ink: theme.foreground,
        framed: true,
        hair: theme.foreground.opacity(0.12),
    }
}

#[test]
fn fits_means_no_tip_even_with_ellipsis_in_the_real_name() {
    assert!(!cut("fits.md", "fits.md", px(50.), px(50.)));
    assert!(!cut("fits.md", "fits.md", px(50.005), px(50.)));
    assert!(!cut("real…name", "real…name", px(50.), px(100.)));
    assert!(!cut("", "", px(0.), px(0.)));
    assert!(cut("long-name.md", "lo…md", px(45.), px(50.)));
    assert!(cut("wide", "wide", px(60.), px(50.)));
}

#[test]
fn one_owner_replaces_the_candidate_and_never_keeps_stale_work() {
    let mut owner = State::default();
    assert!(owner.reconcile(Some(candidate())));
    owner.ready = true;
    assert!(!owner.reconcile(Some(candidate())));
    let mut other = candidate();
    other.identity.0 += 1;
    assert!(!owner.reconcile(Some(other)));
    assert!(!owner.ready);
    assert!(owner.shaped.is_none());
    assert!(owner.suppressed);
    assert!(!owner.reconcile(None));
    assert!(owner.candidate.is_none());
}

#[test]
fn dismissal_retires_pending_work_until_a_real_pointer_move() {
    let mut owner = State::default();
    let point = point(px(70.), px(45.));
    owner.moved(point, false);
    owner.reconcile(Some(candidate()));
    owner.ready = true;
    owner.dismiss();
    assert!(!owner.reconcile(Some(candidate())));
    assert!(owner.candidate.is_none() && !owner.ready && owner.timer.is_none());
    assert!(!owner.moved(point, false));
    assert!(owner.moved(point + point_offset(), false));
    assert!(owner.reconcile(Some(candidate())));
    owner.moved(point, true);
    assert!(!owner.reconcile(Some(candidate())));
}

fn point_offset() -> gpui::Point<Pixels> {
    point(px(1.), px(0.))
}

// Corner radii of every painted quad that covers the shown plate, at one compositor rounding.
pub(crate) fn plate_radii(cx: &mut gpui::VisualTestContext, rounding: f32) -> Vec<f32> {
    cx.update(|_, cx| cx.set_global(crate::compositor::Corner(rounding)));
    let (scale, quads) = crate::presentation::frame_gate::capture(cx);
    let plate = cx.update(|_, cx| {
        let owner = cx
            .global::<Probe>()
            .0
            .borrow()
            .upgrade()
            .expect("mounted tip owner");
        let owner = owner.borrow();
        let tip = owner.candidate.as_ref().expect("shown tip");
        let y = tip.row.top() + (tip.row.size.height - tip.height) / 2.;
        gpui::Bounds::from_corners(
            point(tip.text_x - tip.pad - px(1.), y),
            point(tip.row.right(), y + tip.height),
        )
    });
    quads
        .iter()
        .filter(|quad| quad.bounds == plate.scale(scale))
        .flat_map(|quad| {
            let radii = quad.corner_radii;
            [
                radii.top_left,
                radii.top_right,
                radii.bottom_left,
                radii.bottom_right,
            ]
            .map(|radius| radius.0 / scale)
        })
        .collect()
}
