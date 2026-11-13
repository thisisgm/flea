//! Test-only frame harness: listing fixtures, a real text system and allocation counts per frame.
use super::{pane::Held, ListElement, ListState, Typeface};
use flea::protocol::RowFacts;
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
        System.alloc(layout)
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count();
        System.alloc_zeroed(layout)
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count();
        System.realloc(ptr, layout, size)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

/// Heap allocations this thread has made so far.
pub fn allocations() -> u64 {
    ALLOCATIONS.with(Cell::get)
}

fn row(name: String, icon: &str, size: u64, dir: bool) -> RowFacts {
    RowFacts {
        listing_index: None,
        source: Some(format!("1:2:{}", if dir { 0o040000 } else { 0o100000 })),
        depth: 0,
        open: false,
        name,
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

/// A 2000-row listing of mixed kinds with a CJK stem every 17th row, and a plain ASCII one whose shaping is alike on every machine.
pub fn fixture(name: &str) -> Vec<RowFacts> {
    match name {
        "2k" => (0..2_000)
            .map(|ix| {
                let prefix = format!(
                    "{ix:06}_{}",
                    if ix % 17 == 0 {
                        "\u{8d44}\u{6599}"
                    } else {
                        "sample"
                    }
                );
                if ix < 8 {
                    row(prefix + "_folder", "folder", 4096, true)
                } else if ix % 5 == 0 {
                    row(prefix + ".png", "image-x-generic", 333, false)
                } else if ix % 7 == 0 {
                    row(prefix + ".rs", "text-x-script", 26, false)
                } else {
                    row(prefix + ".txt", "text-plain", 26, false)
                }
            })
            .collect(),
        "uniform" => (0..2_000)
            .map(|ix| row(format!("file-{ix:06}.txt"), "text-plain", 26, false))
            .collect(),
        other => panic!("no fixture named {other}"),
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

/// A test app on the platform's real text system with the bench face, so shaping and glyph lookups are real.
pub fn real_cx() -> TestAppContext {
    let text = gpui_platform::current_platform(true).text_system();
    TestAppContext::build_with_text_system(TestDispatcher::new(0), None, text)
}

pub fn bench_typeface() -> Typeface {
    Typeface {
        font: font("JetBrains Mono"),
        ..Typeface::omarchy_default()
    }
}

pub struct View {
    pub held: Rc<Held>,
    pub scroll_y: f32,
    pub cursor: usize,
    pub list: Rc<RefCell<ListState>>,
}

impl Render for View {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(ListElement::new(
            Rc::clone(&self.held),
            self.scroll_y,
            self.cursor,
            Rc::clone(&self.list),
        ))
    }
}

pub fn open(
    cx: &mut TestAppContext,
    held: Rc<Held>,
    width: f32,
    height: f32,
) -> WindowHandle<View> {
    let mut state = ListState::new(bench_typeface());
    // Dates wait for catalog words, so the bench loads the built-in English catalog the way startup does.
    state.set_words(crate::row::format::Words::from_catalog(&english()));
    let list = Rc::new(RefCell::new(state));
    cx.open_window(size(px(width), px(height)), |_, _| View {
        held,
        scroll_y: 0.,
        cursor: 0,
        list,
    })
}

/// The built-in English catalog alone, with no Omarchy or language file to read.
pub fn english() -> flea::i18n::I18n {
    flea::i18n::I18n::load(flea::i18n::Paths {
        omarchy: "/nonexistent/omarchy".into(),
        language_file: "/nonexistent/language".into(),
        catalogs: Vec::new(),
    })
}

/// Draws one whole frame at this scroll offset, the work a GPUI frame does before the renderer.
pub fn frame(cx: &mut TestAppContext, window: WindowHandle<View>, scroll_y: f32) {
    window
        .update(cx, |view, _, _| view.scroll_y = scroll_y)
        .expect("the bench window is open");
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .expect("the bench window is open");
}
