//! Element frames on the platform's real text system: one TrackElement per drawn track, and heap counts per frame.
use flea::protocol::RowFacts;
use flea_gui::columns::{
    element::TrackElement,
    paint::{ClipMarks, Focus, FolderSizes, Painters},
    Held, TrackView, Typeface,
};
use gpui::{
    div, font, prelude::*, px, size, Context, IntoElement, Render, TestAppContext, TestDispatcher,
    Window, WindowHandle,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    rc::Rc,
};

struct Counting;

thread_local! {
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
}

fn count() {
    let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
}

// SAFETY: every call forwards to the system allocator unchanged; the count is a thread-local add.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count();
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

/// Heap allocations this thread has made so far.
pub fn allocations() -> u64 {
    ALLOCATIONS.with(Cell::get)
}

/// A test app on the platform's real text system, so shaping and glyph lookups are real.
pub fn real_cx() -> TestAppContext {
    let text = gpui_platform::current_platform(true).text_system();
    TestAppContext::build_with_text_system(TestDispatcher::new(0), None, text)
}

pub fn typeface() -> Typeface {
    Typeface {
        font: font("JetBrains Mono"),
        ..Typeface::omarchy_default()
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

/// The drawn area as the shell would compose it: each track placed at its x, painted by its slot's state.
pub struct View {
    pub tracks: Rc<[TrackView]>,
    pub held: Rc<Held>,
    pub focus: Rc<Focus>,
    pub active_scroll: f32,
    pub scrolls: [f32; 5],
    pub clip: Option<Rc<dyn ClipMarks>>,
    pub sizes: Option<Rc<dyn FolderSizes>>,
    pub painters: Painters,
    pub typeface: Typeface,
    pub words: Rc<flea_gui::row::format::Words>,
}

impl Render for View {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.painters.retain(&self.tracks);
        let mut area = div().size_full().relative();
        for (at, view) in self.tracks.iter().enumerate() {
            let paint = self.painters.painter(view.slot, &self.typeface);
            let scroll = if view.slot == flea_gui::columns::geometry::Slot::Active {
                self.active_scroll
            } else {
                self.scrolls[view.slot.index()]
            };
            let element = TrackElement::new(
                Rc::clone(&self.tracks),
                at,
                Some(Rc::clone(&self.held)),
                Rc::clone(&self.focus),
                scroll,
                self.clip.clone(),
                paint,
                Rc::clone(&self.words),
            );
            let element = match &self.sizes {
                Some(sizes) => element.with_sizes(Rc::clone(sizes)),
                None => element,
            };
            area = area.child(
                div()
                    .absolute()
                    .left(px(view.x))
                    .top(px(0.))
                    .w(px(view.width))
                    .h_full()
                    .child(element),
            );
        }
        area
    }
}

pub fn open(
    cx: &mut TestAppContext,
    tracks: Rc<[TrackView]>,
    held: Rc<Held>,
    width: f32,
    height: f32,
) -> WindowHandle<View> {
    cx.open_window(size(px(width), px(height)), |_, _| View {
        tracks,
        held,
        focus: Rc::new(Focus::default()),
        active_scroll: 0.,
        scrolls: [0.; 5],
        clip: None,
        sizes: None,
        painters: Painters::default(),
        typeface: typeface(),
        words: Rc::new(super::fixture::words()),
    })
}

/// Draws one whole frame, the work a GPUI frame does before the renderer.
pub fn frame(cx: &mut TestAppContext, window: WindowHandle<View>) {
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .expect("the frame window is open");
}

/// Changes what the window draws before the next frame.
pub fn set(cx: &mut TestAppContext, window: WindowHandle<View>, change: impl FnOnce(&mut View)) {
    window
        .update(cx, |view, _, _| change(view))
        .expect("the frame window is open");
}

pub fn read<R>(
    cx: &mut TestAppContext,
    window: WindowHandle<View>,
    look: impl FnOnce(&View) -> R,
) -> R {
    window
        .update(cx, |view, _, _| look(view))
        .expect("the frame window is open")
}
