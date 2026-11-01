//! Real frames on the platform text system: the one tile Element, its bounded cache, still-frame allocations, hits and the IPC seam.
use super::{allocations, board_view, named};
use flea::protocol::RowFacts;
use flea_gui::{
    grid::{
        element::{Decor, DropKind, GridElement, GridState, Overlay, Plain},
        FocusFacts, FocusSnapshot, GridView,
    },
    ipc::{grid::GridAdapter, Adapter},
    pane::Held,
};
use gpui::{
    div, font, prelude::*, px, size, Context, IntoElement, Render, TestAppContext, TestDispatcher,
    Window, WindowHandle,
};
use std::{cell::RefCell, rc::Rc};

pub struct Host {
    pub held: Rc<Held>,
    pub shown: Option<Rc<[usize]>>,
    pub view: GridView,
    pub focus: Rc<FocusSnapshot>,
    pub decor: Rc<dyn Decor>,
    pub overlay: Overlay,
    pub state: Rc<RefCell<GridState>>,
}

impl Render for Host {
    // The registration contract: a hidden grid builds no Element, so it does no renderer work.
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let grid = self.view.visible.then(|| {
            let focus: Rc<dyn FocusFacts> = self.focus.clone();
            GridElement::new(
                Rc::clone(&self.held),
                self.shown.clone(),
                self.view.clone(),
                focus,
                Rc::clone(&self.decor),
                self.overlay.clone(),
                Rc::clone(&self.state),
            )
        });
        div().size_full().children(grid)
    }
}

pub fn held(rows: Vec<RowFacts>) -> Rc<Held> {
    Rc::new(Held {
        listing: 1,
        total: rows.len(),
        start: 0,
        rows,
    })
}

pub fn rows(total: usize) -> Vec<RowFacts> {
    (0..total).map(named).collect()
}

pub fn real_cx() -> TestAppContext {
    let text = gpui_platform::current_platform(true).text_system();
    TestAppContext::build_with_text_system(TestDispatcher::new(0), None, text)
}

/// The board grid, 908 by 640, on the bench face so shaping is alike on every machine.
pub fn open(cx: &mut TestAppContext, held: Rc<Held>) -> WindowHandle<Host> {
    let view = board_view();
    cx.open_window(size(px(view.size.0), px(view.size.1)), |_, _| Host {
        held,
        shown: None,
        view,
        focus: Rc::new(FocusSnapshot::default()),
        decor: Rc::new(Plain),
        overlay: Overlay::default(),
        state: Rc::new(RefCell::new(GridState::new(font("JetBrains Mono")))),
    })
}

pub fn with<R>(
    cx: &mut TestAppContext,
    window: WindowHandle<Host>,
    change: impl FnOnce(&mut Host) -> R,
) -> R {
    window
        .update(cx, |host, _, _| change(host))
        .expect("the grid window is open")
}

/// Draws one whole frame, the work a GPUI frame does before the renderer.
pub fn frame(cx: &mut TestAppContext, window: WindowHandle<Host>) {
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .expect("the grid window is open");
}

pub fn state(cx: &mut TestAppContext, window: WindowHandle<Host>) -> Rc<RefCell<GridState>> {
    with(cx, window, |host| Rc::clone(&host.state))
}

// Allocations of five still frames over this many rows, after the frames that shape them.
fn still_frames(total: usize) -> (u64, u64) {
    let mut cx = real_cx();
    let window = open(&mut cx, held(rows(total)));
    let grid = state(&mut cx, window);
    for _ in 0..3 {
        frame(&mut cx, window);
    }
    let (heap, shaped) = (allocations(), grid.borrow().shaped().1);
    for _ in 0..5 {
        frame(&mut cx, window);
    }
    let still = allocations() - heap;
    let reshaped = grid.borrow().shaped().1 - shaped;
    (still, reshaped)
}

#[test]
fn one_element_draws_only_the_visible_tiles_and_a_still_frame_allocates_nothing_per_tile() {
    let mut cx = real_cx();
    let window = open(&mut cx, held(rows(2000)));
    let grid = state(&mut cx, window);
    frame(&mut cx, window);
    let drawn = grid
        .borrow()
        .drawn()
        .iter()
        .map(|tile| tile.view)
        .collect::<Vec<_>>();
    assert_eq!(
        drawn,
        (0..30).collect::<Vec<_>>(),
        "five rows of six meet a 640 view"
    );
    let (empty, few, many) = (still_frames(0), still_frames(6), still_frames(2000));
    assert_eq!((few.1, many.1), (0, 0), "a still frame shaped a tile");
    assert_eq!(
        few.0, many.0,
        "still frames allocated per drawn tile: six {} thirty {}",
        few.0, many.0
    );
    // A frame that draws any tile may make one allocation more than an empty grid, the same at six tiles and thirty.
    assert!(
        many.0 <= empty.0 + 5,
        "five still frames allocated {} against {} for an empty grid",
        many.0,
        empty.0
    );
    // Positive control: a scroll to unseen rows shapes and allocates, so the counter can see grid work.
    let (heap, shaped) = (allocations(), grid.borrow().shaped().1);
    with(&mut cx, window, |host| host.view.scroll = 1340.);
    frame(&mut cx, window);
    assert!(
        allocations() - heap > many.0 / 5,
        "a frame of new tiles allocated no more than a still one"
    );
    assert!(grid.borrow().shaped().1 > shaped, "new tiles were shaped");
}

#[test]
fn the_tile_cache_stays_bounded_to_the_drawn_rows_plus_the_cache_rows() {
    let mut cx = real_cx();
    let window = open(&mut cx, held(rows(2000)));
    let grid = state(&mut cx, window);
    let mut peak = 0;
    for step in 0..80 {
        with(&mut cx, window, |host| host.view.scroll = step as f32 * 67.);
        frame(&mut cx, window);
        peak = peak.max(grid.borrow().cached_tiles());
    }
    let bound = 6 * (6 + 2 * 2);
    assert!(
        peak <= bound,
        "the cache held {peak} tiles, over the {bound} the drawn rows and two cache rows allow"
    );
    let misses = grid.borrow().shaped().1;
    with(&mut cx, window, |host| host.view.scroll -= 134.);
    frame(&mut cx, window);
    assert_eq!(
        grid.borrow().shaped().1,
        misses,
        "a row back inside the cache rows reshapes nothing"
    );
    with(&mut cx, window, |host| host.held = held(rows(2000)));
    frame(&mut cx, window);
    assert_eq!(
        grid.borrow().shaped().1,
        misses,
        "an equal refresh reshapes nothing"
    );
    let mut renamed = rows(2000);
    let first = grid.borrow().drawn()[0].listing;
    renamed[first].name = "renamed.txt".into();
    with(&mut cx, window, |host| host.held = held(renamed));
    frame(&mut cx, window);
    assert_eq!(
        grid.borrow().shaped().1,
        misses + 1,
        "a changed row reshapes alone"
    );
}

#[test]
fn a_tile_the_window_left_keeps_its_row_until_a_new_listing() {
    let mut cx = real_cx();
    let window = open(&mut cx, held(rows(40)));
    let grid = state(&mut cx, window);
    frame(&mut cx, window);
    assert_eq!(
        grid.borrow()
            .drawn_row(5)
            .and_then(|tile| tile.caption_text().map(str::to_owned))
            .as_deref(),
        Some("g_00005.txt")
    );
    let tail = Rc::new(Held {
        listing: 1,
        total: 40,
        start: 20,
        rows: (20..40).map(named).collect(),
    });
    with(&mut cx, window, |host| host.held = tail);
    frame(&mut cx, window);
    assert_eq!(
        grid.borrow()
            .drawn_row(5)
            .and_then(|tile| tile.caption_text().map(str::to_owned))
            .as_deref(),
        Some("g_00005.txt"),
        "the same listing keeps a row the window left"
    );
    let fresh = Rc::new(Held {
        listing: 2,
        total: 40,
        start: 20,
        rows: (20..40).map(named).collect(),
    });
    with(&mut cx, window, |host| host.held = fresh);
    frame(&mut cx, window);
    assert_eq!(
        grid.borrow()
            .drawn_row(5)
            .and_then(|tile| tile.caption_text()),
        None,
        "a new listing leaves no row behind"
    );
}

#[test]
fn hits_take_the_slot_and_the_painted_caption_and_leave_the_margins_bare() {
    let mut cx = real_cx();
    let window = open(&mut cx, held(rows(20)));
    let grid = state(&mut cx, window);
    frame(&mut cx, window);
    let state = grid.borrow();
    let tile = state.drawn_row(7).expect("tile 7 is drawn").clone();
    let centre = |rect: flea_gui::grid::geometry::Rect| {
        (rect.x + rect.width / 2., rect.y + rect.height / 2.)
    };
    let (sx, sy) = centre(tile.slot);
    let (cx_, cy_) = centre(tile.painted);
    assert_eq!(state.tile_at(sx, sy), Some(7), "the icon box is the tile");
    assert_eq!(
        state.tile_at(cx_, cy_),
        Some(7),
        "the painted caption is the tile"
    );
    assert!(
        tile.painted.width > 0. && tile.painted.width < tile.caption.width,
        "the caption paints a width inside its label"
    );
    assert_eq!(
        state.tile_at(tile.cell.x + 3., tile.cell.y + 3.),
        None,
        "a cell corner is bare ground"
    );
    assert_eq!(
        state.tile_at(tile.caption.x + 1., tile.painted.y + 2.),
        None,
        "the label beside a short caption is bare"
    );
    assert_eq!(
        state.tile_at(tile.slot.x - 2., sy),
        None,
        "the margin beside the icon box is bare"
    );
    assert_eq!(state.tile_at(4., 4.), None, "the grid's own margin is bare");
    drop(state);
    with(&mut cx, window, |host| host.overlay.renaming = Some(7));
    frame(&mut cx, window);
    let state = grid.borrow();
    assert!(
        state.drawn_row(7).unwrap().editing,
        "the renaming tile is drawn as editing"
    );
    assert_eq!(
        state.tile_at(tile.cell.x + 3., tile.cell.y + 3.),
        Some(7),
        "an editing tile takes the whole cell"
    );
}

#[test]
fn a_rename_survives_scrolling_its_tile_out_of_the_drawn_window_and_back() {
    let mut cx = real_cx();
    let window = open(&mut cx, held(rows(2000)));
    let grid = state(&mut cx, window);
    with(&mut cx, window, |host| host.overlay.renaming = Some(8));
    frame(&mut cx, window);
    with(&mut cx, window, |host| host.view.scroll = 20_000.);
    frame(&mut cx, window);
    assert!(
        grid.borrow().drawn_row(8).is_none(),
        "the renaming tile left the drawn window"
    );
    assert!(
        grid.borrow().drawn().iter().all(|tile| !tile.editing),
        "no other tile lights an editor"
    );
    with(&mut cx, window, |host| host.view.scroll = 0.);
    frame(&mut cx, window);
    let state = grid.borrow();
    assert!(
        state.drawn_row(8).unwrap().editing,
        "the editor state is the overlay's, so it returns with its tile"
    );
    assert_eq!(state.drawn().iter().filter(|tile| tile.editing).count(), 1);
}

#[test]
fn the_cursor_frame_selection_and_hover_are_read_from_their_owner() {
    let mut cx = real_cx();
    let window = open(&mut cx, held(rows(20)));
    let grid = state(&mut cx, window);
    with(&mut cx, window, |host| {
        host.focus = Rc::new(FocusSnapshot {
            cursor: 3,
            selected: vec![4, 5],
            ..Default::default()
        });
        host.view.hovered = Some(9);
    });
    frame(&mut cx, window);
    let lit = |grid: &GridState| {
        grid.drawn()
            .iter()
            .map(|tile| (tile.cursor, tile.selected, tile.hovered))
            .collect::<Vec<_>>()
    };
    let flags = lit(&grid.borrow());
    assert_eq!(
        (flags[3], flags[4], flags[5], flags[9], flags[0]),
        (
            (true, false, false),
            (false, true, false),
            (false, true, false),
            (false, false, true),
            (false, false, false)
        )
    );
    with(&mut cx, window, |host| {
        host.focus = Rc::new(FocusSnapshot {
            cursor: 3,
            band: true,
            ..Default::default()
        })
    });
    frame(&mut cx, window);
    assert!(!lit(&grid.borrow())[9].2, "a sweeping band shows no hover");
}

#[test]
fn caption_state_answers_the_ipc_seam_from_the_drawn_frame() {
    let mut cx = real_cx();
    let mut names = rows(12);
    names[0].name = "screenshot-2026-08-30-final-review-for-gm-after-the-bench-v3.png".into();
    names[1].name = "my-vacation-1.jpeg".into();
    let window = open(&mut cx, held(names));
    let grid = state(&mut cx, window);
    with(&mut cx, window, |host| {
        host.overlay.drop = Some((1, DropKind::Copy))
    });
    frame(&mut cx, window);
    let view = with(&mut cx, window, |host| host.view.clone());
    let state = grid.borrow();
    let adapter = GridAdapter {
        view: &view,
        state: &state,
    };
    assert_eq!(adapter.call("gridColumns", &[]).unwrap().stdout.trim(), "6");
    let long = adapter.caption_state(0);
    // The break is the drawn caption's own, so the seam holds on whatever face the host resolves.
    let caption = state
        .drawn_row(0)
        .unwrap()
        .caption_text()
        .unwrap()
        .to_owned();
    let wrap = caption[..caption.find('\n').expect("the long name breaks")]
        .encode_utf16()
        .count();
    assert!(
        long.contains(&format!("\"wrapAt\":{wrap},"))
            && long.contains("\"broken\":true")
            && long.contains("\"lines\":2")
            && long.contains("\"truncated\":false"),
        "{long}"
    );
    assert!(
        state.drawn_row(0).unwrap().elided(),
        "an elided caption tells the tip to show"
    );
    let drop = adapter.caption_state(1);
    assert!(
        drop.contains("\"lines\":1") && drop.contains("\"wrapAt\":-1"),
        "a drop target lays one caption line: {drop}"
    );
    let tile = state.drawn_row(1).unwrap();
    assert!(
        tile.caption.bottom() + 16.5 <= tile.cell.bottom() + 0.5,
        "the drop label fits under the one line inside the tile"
    );
    assert_eq!(
        adapter.caption_state(999),
        "{}",
        "a row no tile draws answers an empty object"
    );
    assert_eq!(
        adapter
            .call("gridCaptionState", &["0".into()])
            .unwrap()
            .stdout
            .trim(),
        long
    );
    assert!(
        adapter.call("cursor", &[]).is_none(),
        "other functions fall through to the pane"
    );
}
