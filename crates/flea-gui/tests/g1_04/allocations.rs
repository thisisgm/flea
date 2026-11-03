use super::*;
use flea_gui::window::input::Keyboard;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
};

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

struct Allocator;

fn record() {
    if ENABLED.try_with(Cell::get).unwrap_or(false) {
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
    }
}

unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record();
        unsafe { System.realloc(pointer, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

fn counted(operation: impl FnOnce()) -> usize {
    ALLOCATIONS.with(|count| count.set(0));
    ENABLED.with(|enabled| enabled.set(true));
    operation();
    ENABLED.with(|enabled| enabled.set(false));
    ALLOCATIONS.with(Cell::get)
}

#[test]
fn dispatch_pointer_hints_and_sequences_allocate_nothing_with_positive_control() {
    assert!(
        counted(|| {
            black_box(Box::new(7));
        }) > 0,
        "allocator control did not count"
    );
    let event = gpui::KeyDownEvent {
        keystroke: gpui::Keystroke {
            key: "y".into(),
            key_char: Some("y".into()),
            us_key: Some("y".into()),
            ..Default::default()
        },
        is_held: false,
        prefer_character_input: false,
    };
    let shifted = gpui::KeyDownEvent {
        keystroke: gpui::Keystroke {
            key: ">".into(),
            key_char: Some(">".into()),
            modifiers: gpui::Modifiers::control(),
            is_keypad: false,
            us_key: Some(">".into()),
        },
        is_held: false,
        prefer_character_input: false,
    };
    let held = gpui::Modifiers::control_shift();
    let mut keyboard = Keyboard::default();
    let count = counted(|| {
        for preset in keys::PRESETS {
            for context in Context::ALL {
                for row in keys::PRESET_KEYS.iter().chain(keys::SHARED_KEYS) {
                    black_box(keys::lookup(preset, context, Frontend::Gui, row.input()));
                    black_box(keys::presentation::hint_for(preset, row.action));
                }
            }
        }
        for row in keys::POINTER {
            black_box(keys::pointer(
                row.place, row.row, row.button, row.mask, row.taps,
            ));
        }
        for _ in 0..1000 {
            black_box(keyboard.pair_armed("vim", Default::default()));
            black_box(keyboard.resolve(
                "vim",
                &event,
                Default::default(),
                false,
                Default::default(),
            ));
            black_box(keyboard.resolve("default", &shifted, held, false, Default::default()));
        }
    });
    assert_eq!(count, 0);
}
