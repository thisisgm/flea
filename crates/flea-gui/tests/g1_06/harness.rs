//! The G1.06 frame harness: a real text system, a pane drawn as header over list, fixtures and an allocation count.
use flea::protocol::RowFacts;
use flea_gui::list::{
    pane::Held, HeaderElement, ListElement, ListState, Metrics, RowStates, Typeface,
};
use gpui::{
    div, font, prelude::*, px, size, Context, IntoElement, Render, TestAppContext, TestDispatcher,
    Window, WindowHandle,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::{Cell, RefCell},
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

/// The board's geometry at Omarchy's default body, the 14 px a fresh install draws.
pub fn metrics() -> Metrics {
    Metrics::at(px(14.))
}

pub fn row(name: &str, icon: &str, size: u64, dir: bool) -> RowFacts {
    RowFacts {
        depth: 0,
        open: false,
        name: name.into(),
        dir,
        size,
        mtime: 1_760_600_000,
        mode: if dir { 0o040755 } else { 0o100644 },
        icon: icon.into(),
        link: None,
        real_modes: true,
        thumbless: false,
        kind: String::new(),
        inline: None,
        tags: None,
    }
}

/// Plain ASCII rows whose shaping is alike on every machine.
pub fn uniform(count: usize) -> Vec<RowFacts> {
    (0..count)
        .map(|ix| row(&format!("file-{ix:06}.txt"), "text-plain", 26, false))
        .collect()
}

pub fn held(rows: Vec<RowFacts>, listing: usize) -> Rc<Held> {
    Rc::new(Held {
        listing,
        total: rows.len(),
        start: 0,
        rows,
    })
}

/// A test app on the platform's real text system, so shaping, advances and glyph lookups are real.
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

/// One pane as the composition stacks it: the header's own height, then the list in the rest.
pub struct View {
    pub held: Rc<Held>,
    pub scroll_y: f32,
    pub states: RowStates,
    pub list: Rc<RefCell<ListState>>,
    pub list_height: f32,
}

impl Render for View {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let rows = ListElement::with_states(
            Rc::clone(&self.held),
            self.scroll_y,
            self.states.clone(),
            Rc::clone(&self.list),
        );
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(HeaderElement::new(Rc::clone(&self.list)))
            .child(div().w_full().h(px(self.list_height)).child(rows))
    }
}

pub struct Pane {
    pub window: WindowHandle<View>,
    pub list: Rc<RefCell<ListState>>,
}

/// A window this wide and this many list rows tall under the header, the cursor on the first row.
pub fn open(cx: &mut TestAppContext, held: Rc<Held>, width: f32, rows: f32) -> Pane {
    let list = Rc::new(RefCell::new(ListState::new(typeface())));
    let list_height = rows * metrics().row;
    let height = list_height + metrics().header;
    let states = RowStates::cursor_only(held.listing, 0);
    let shared = Rc::clone(&list);
    let window = cx.open_window(size(px(width), px(height)), |_, _| View {
        held,
        scroll_y: 0.,
        states,
        list: shared,
        list_height,
    });
    Pane { window, list }
}

impl Pane {
    /// Draws one whole frame at this scroll offset, the work a GPUI frame does before the renderer.
    pub fn frame(&self, cx: &mut TestAppContext, scroll_y: f32) {
        self.window
            .update(cx, |view, _, _| view.scroll_y = scroll_y)
            .expect("the pane window is open");
        self.draw(cx);
    }

    pub fn draw(&self, cx: &mut TestAppContext) {
        cx.update_window(self.window.into(), |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
        .expect("the pane window is open");
    }

    pub fn update(&self, cx: &mut TestAppContext, change: impl FnOnce(&mut View)) {
        self.window
            .update(cx, |view, _, _| change(view))
            .expect("the pane window is open");
    }

    /// Runs a closure with the window, for the calls that measure text as a frame does.
    pub fn with_window<R>(
        &self,
        cx: &mut TestAppContext,
        body: impl FnOnce(&mut ListState, &Held, &mut Window) -> R,
    ) -> R {
        let list = Rc::clone(&self.list);
        self.window
            .update(cx, |view, window, _| {
                body(&mut list.borrow_mut(), &view.held, window)
            })
            .expect("the pane window is open")
    }

    pub fn held(&self, cx: &mut TestAppContext) -> Rc<Held> {
        self.window
            .update(cx, |view, _, _| Rc::clone(&view.held))
            .expect("the pane window is open")
    }

    pub fn states(&self, cx: &mut TestAppContext) -> RowStates {
        self.window
            .update(cx, |view, _, _| view.states.clone())
            .expect("the pane window is open")
    }
}

/// The built-in English catalog alone: no Omarchy tree, no language file and no catalog directory are read.
pub fn english() -> flea::i18n::I18n {
    flea::i18n::I18n::load(flea::i18n::Paths {
        omarchy: "/nonexistent/omarchy".into(),
        language_file: "/nonexistent/language".into(),
        catalogs: Vec::new(),
    })
}
