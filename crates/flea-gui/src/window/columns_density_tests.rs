//! #388: every Columns track takes Row density's row, the List's at the same face, through one source.
use super::host;
use crate::bfx_cols_tests::Painted;
use crate::columns::geometry::{Slot, FOOTER};
use crate::g2::file_search::contents::{BLOCK_H, ROW_H};
use crate::list::{Density, Typeface};
use crate::row::tags::SWATCH_MARK;
use crate::state::Shell;
use gpui::{px, Context, TestAppContext, Window, WindowHandle};

/// v0.3.12's Theme.fileRowHeight at text 14.
pub(super) const DENSITIES: [(&str, f32); 4] = [
    ("tight", 23.),
    ("compact", 31.),
    ("normal", 37.),
    ("comfortable", 45.),
];
/// v0.3.12's Theme.fileRowHeight at text 12.
const DENSITIES_12: [(&str, f32); 4] = [
    ("tight", 20.),
    ("compact", 26.),
    ("normal", 32.),
    ("comfortable", 38.),
];
/// Columns draws no list header, so its tracks start under the 27 px window chrome.
const AREA_TOP: f32 = 27.;
const PARENT_ROWS: usize = 60;
/// The parent row naming the active folder, which draws lifted with its chevron.
const LIFTED: usize = 1;

fn set_density(shell: &mut Shell, density: &str, window: &mut Window, cx: &mut Context<Shell>) {
    let patch = flea::jsondoc::parse(&format!(r#"{{"density":"{density}"}}"#)).unwrap();
    let document = flea::uistate::patched(&shell.launch.settings.data, &patch).unwrap();
    shell.receive_settings(
        crate::window::settings::store::Update::Loaded {
            document,
            through: 0,
            saved: false,
        },
        window,
        cx,
    );
}

/// A window at /srv/a/b/c whose parent holds 60 folders; `lifted` names c among them and `tagged` tags the first active folder.
pub(super) fn open(
    cx: &mut TestAppContext,
    lifted: bool,
    tagged: bool,
) -> (WindowHandle<Shell>, impl FnMut() -> Option<(usize, usize)>) {
    let (window, requests) = host::open(cx, "/srv/a/b/c");
    let mut many: Vec<String> = (0..PARENT_ROWS).map(|n| format!("p{n:02}")).collect();
    if lifted {
        many[LIFTED] = "c".into();
    }
    host::change(cx, window, |shell, window, cx| {
        let rows: Vec<(&str, bool)> = many.iter().map(|name| (name.as_str(), true)).collect();
        host::peek(shell, "/srv/a/b", &rows, false, window, cx);
        host::settle(shell, window, cx);
        if !tagged {
            return;
        }
        let host_pane = shell.panes[0].as_mut().unwrap();
        let mut tagged = host::row("d01", true);
        tagged.tags = Some(vec!["Red".into(), "Blue".into()]);
        host_pane.pane.held = std::rc::Rc::new(crate::list::pane::Held {
            listing: host_pane.pane.held.listing,
            total: 2,
            start: 0,
            rows: vec![tagged, host::row("d02", true)],
        });
    });
    (window, requests)
}

pub(super) fn render(
    cx: &mut TestAppContext,
    window: WindowHandle<Shell>,
    density: &str,
) -> Vec<Painted> {
    host::change(cx, window, |shell, window, cx| {
        set_density(shell, density, window, cx)
    });
    host::frame(cx, window);
    crate::bfx_cols_tests::frame(cx, window)
}

#[test]
fn b23_columns_rows_follow_row_density_like_the_list() {
    let mut cx = host::context();
    let (window, _requests) = open(&mut cx, true, false);
    for (density, want) in DENSITIES {
        let painted = render(&mut cx, window, density);
        host::change(&mut cx, window, |shell, window, cx| {
            let list_row = shell.panes[0]
                .as_ref()
                .unwrap()
                .list
                .borrow()
                .file_metrics()
                .row;
            assert_eq!(list_row, want, "{density}: the List row at body 14");
            let rect = host::call(shell, "rowRect", &["0"], window, cx).stdout;
            let rect: Vec<f32> = rect
                .split_whitespace()
                .map(|n| n.parse().unwrap())
                .collect();
            assert_eq!(rect[3], want, "{density}: rowRect 0 is one density row");
            let host_pane = shell.panes[0].as_mut().unwrap();
            assert_eq!(
                host_pane.columns_row_height(),
                want,
                "{density}: keyboard row"
            );
            let frame = host_pane
                .columns_key_frame(crate::list::pane::Grow::default())
                .expect("the active track drew");
            assert_eq!(frame.row, want, "{density}: key frame row");
            let columns = host_pane.columns.as_mut().unwrap();
            for slot in [Slot::Parent, Slot::Active, Slot::Child] {
                let paint = columns.painters.get(slot).unwrap().borrow();
                let drawn = paint.drawn.as_ref().unwrap();
                assert_eq!(drawn.row_height, want, "{density}: {slot:?} pitch");
                let top = drawn.bounds.top() - px(drawn.scroll);
                if slot == Slot::Parent {
                    assert_eq!(
                        paint.visible().nth(LIFTED),
                        Some(LIFTED),
                        "{density}: the lifted row is among the rows checked"
                    );
                }
                for (at, index) in paint.visible().enumerate().take(2) {
                    let row_top = top + px(at as f32 * want);
                    let centre = row_top + px(want / 2.);
                    // Anything that touches the row is checked, so a drifted mark fails rather than drops out.
                    let touches = |p: &&Painted| {
                        p.bounds.left() >= drawn.bounds.left()
                            && p.bounds.left() < drawn.bounds.right()
                            && p.bounds.bottom() > row_top
                            && p.bounds.top() < row_top + px(want)
                    };
                    let mid = |p: &Painted| p.bounds.top() + p.bounds.size.height / 2.;
                    let lines: Vec<_> = painted
                        .iter()
                        .filter(|p| p.run != 0)
                        .filter(touches)
                        .collect();
                    assert!(
                        !lines.is_empty(),
                        "{density}: {slot:?} row {index} paints its name"
                    );
                    for line in lines {
                        assert!(
                            (mid(line) - centre).abs() <= px(1.),
                            "{density}: {slot:?} line {:?} centre {centre:?}",
                            mid(line)
                        );
                    }
                    let marks: Vec<_> = painted
                        .iter()
                        .filter(|p| p.run == 0)
                        .filter(touches)
                        .collect();
                    assert!(
                        marks
                            .iter()
                            .any(|p| !p.mark.contains("chevron") && p.mark != SWATCH_MARK),
                        "{density}: {slot:?} row {index} paints its row mark: {marks:?}"
                    );
                    if slot == Slot::Parent && index == LIFTED {
                        assert!(
                            marks.iter().any(|p| p.mark.contains("chevron")),
                            "{density}: the lifted folder paints its chevron: {marks:?}"
                        );
                    }
                    // The row mark and the chevron centre on their own row, as the List's do.
                    for mark in marks {
                        assert_eq!(
                            mid(mark),
                            centre,
                            "{density}: {slot:?} row {index} {}",
                            mark.mark
                        );
                    }
                }
                let first = paint.visible().next().unwrap();
                let at = paint
                    .slot_box(first)
                    .expect("a drawn row has a thumbnail slot");
                assert_eq!(
                    at.1 + at.2 / 2.,
                    f32::from(top) + want / 2.,
                    "{density}: thumbnail slot"
                );
            }
            // The parent's 60 rows scroll to their last row and the footer band, no further.
            let end = columns.area.scroll_by(Slot::Parent, 1e6);
            assert_eq!(
                end,
                PARENT_ROWS as f32 * want + FOOTER - columns.area.height(),
                "{density}: peek scroll end"
            );
            columns.area.scroll_by(Slot::Parent, -1e6);
            let centre = host::call(shell, "columnParentRowCentre", &["1"], window, cx).stdout;
            let y: f32 = centre.split_whitespace().nth(1).unwrap().parse().unwrap();
            assert_eq!(
                y,
                (AREA_TOP + 1.5 * want).round(),
                "{density}: parent row centre"
            );
        });
    }
}

#[test]
fn b23_tag_swatches_centre_on_the_density_row() {
    let mut cx = host::context();
    let (window, _requests) = open(&mut cx, true, true);
    for (density, want) in DENSITIES {
        let painted = render(&mut cx, window, density);
        host::change(&mut cx, window, |shell, _, _| {
            let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
            let paint = columns.painters.get(Slot::Active).unwrap().borrow();
            let drawn = paint.drawn.as_ref().unwrap();
            let centre = drawn.bounds.top() - px(drawn.scroll) + px(want / 2.);
            let swatches: Vec<_> = painted
                .iter()
                .filter(|p| p.mark == SWATCH_MARK && drawn.bounds.contains(&p.bounds.origin))
                .collect();
            assert!(
                !swatches.is_empty(),
                "{density}: the tagged row paints its swatches"
            );
            for swatch in swatches {
                let mid = swatch.bounds.top() + swatch.bounds.size.height / 2.;
                assert_eq!(mid, centre, "{density}: swatch centre");
            }
        });
    }
}

#[test]
fn b23_a_denser_row_pulls_a_peek_scroll_back_inside_its_rows() {
    let mut cx = host::context();
    // No lifted row in the parent, so only the clamp can move its scroll.
    let (window, _requests) = open(&mut cx, false, false);
    render(&mut cx, window, "comfortable");
    host::change(&mut cx, window, |shell, _, _| {
        let columns = shell.panes[0].as_mut().unwrap().columns.as_mut().unwrap();
        columns.area.scroll_by(Slot::Parent, 1e6);
    });
    render(&mut cx, window, "tight");
    host::change(&mut cx, window, |shell, _, _| {
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        assert_eq!(
            columns.area.scroll(Slot::Parent),
            PARENT_ROWS as f32 * DENSITIES[0].1 + FOOTER - columns.area.height(),
            "the parent's scroll ends at its tight rows"
        );
    });
}

#[test]
fn b23_a_density_change_brings_the_lifted_row_back_whole() {
    let mut cx = host::context();
    let (window, _requests) = open(&mut cx, true, false);
    render(&mut cx, window, "comfortable");
    let mut end = 0.;
    host::change(&mut cx, window, |shell, _, _| {
        let columns = shell.panes[0].as_mut().unwrap().columns.as_mut().unwrap();
        end = columns.area.scroll_by(Slot::Parent, 1e6);
    });
    // The same density again keeps the user's scroll, so only the change re-contains.
    render(&mut cx, window, "comfortable");
    host::change(&mut cx, window, |shell, _, _| {
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        assert_eq!(
            columns.area.scroll(Slot::Parent),
            end,
            "an unchanged row keeps the scroll"
        );
    });
    render(&mut cx, window, "tight");
    host::change(&mut cx, window, |shell, _, _| {
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        assert_eq!(
            columns.area.scroll(Slot::Parent),
            LIFTED as f32 * DENSITIES[0].1,
            "the parent scrolls up to its lifted row at the tight pitch"
        );
    });
}

#[test]
fn b23_columns_row_is_the_list_row_at_every_text_size() {
    let mut cx = host::context();
    let (window, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, window, |shell, _, _| {
        let host_pane = shell.panes[0].as_ref().unwrap();
        let mut list = host_pane.list.borrow_mut();
        for (base, rows) in [(12, DENSITIES_12), (14, DENSITIES)] {
            list.set_typeface(Typeface::omarchy_default().at_base(base));
            for (density, want) in rows {
                list.density = Density::parse(density);
                assert_eq!(list.columns_row(), want, "text {base} {density}");
                assert_eq!(list.file_metrics().row, want, "List text {base} {density}");
                // The dual pane resizes only its slot and mark, never the row.
                list.view.dual = true;
                assert_eq!(list.file_metrics().row, want, "dual text {base} {density}");
                list.view.dual = false;
            }
        }
        // A contents search sets the List's block row and leaves the density row to the peek tracks.
        list.view.contents = true;
        assert_eq!(list.file_metrics().row, BLOCK_H);
        assert_eq!(list.columns_row(), DENSITIES[3].1);
        drop(list);
        assert_eq!(host_pane.columns_rows(), (BLOCK_H, ROW_H));
        let mut list = host_pane.list.borrow_mut();
        list.view.contents = false;
        list.density = Density::Compact;
    });
}
