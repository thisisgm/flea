//! The active keyboard group's keycaps: one snapshot per group change, read by the key sheet and every menu's key hints.
use crate::locale_geometry::isolate_ltr;
use flea::keys::display::KeycapSnapshot;
use std::cell::RefCell;

#[derive(Default)]
struct Active {
    name: Option<String>,
    snapshot: Option<KeycapSnapshot>,
    generation: u64,
    watch: Option<gpui::Subscription>,
}

thread_local! {
    static ACTIVE: RefCell<Active> = RefCell::default();
}

/// gpui names the effective XKB group by description and not index, so group 0 is a placeholder; an unknown group draws canonical legends.
pub fn sync(name: &str) -> Option<KeycapSnapshot> {
    ACTIVE.with_borrow_mut(|active| {
        if active.name.as_deref() != Some(name) {
            active.generation += 1;
            active.name = Some(name.to_owned());
            active.snapshot = KeycapSnapshot::from_group_name(name, 0, active.generation).ok();
        }
        active.snapshot
    })
}

/// The snapshot the last group change resolved, read while a menu is built and never while it paints.
pub fn current() -> Option<KeycapSnapshot> {
    ACTIVE.with_borrow(|active| active.snapshot)
}

/// Follow the active group from now on, so a switch re-resolves before the next menu or sheet is built.
pub fn watch(cx: &gpui::App) -> Option<KeycapSnapshot> {
    let snapshot = sync(cx.keyboard_layout().name());
    ACTIVE.with_borrow_mut(|active| {
        active.watch.get_or_insert_with(|| {
            cx.on_keyboard_layout_change(|cx| {
                sync(cx.keyboard_layout().name());
            })
        });
    });
    snapshot
}

/// Whether this thread follows the active group yet.
pub fn watching() -> bool {
    ACTIVE.with_borrow(|active| active.watch.is_some())
}

/// Hand a view the snapshot of each group switch while it lives, then draw it again.
pub fn follow<V: 'static>(
    cx: &mut gpui::Context<V>,
    apply: impl Fn(&mut V, Option<KeycapSnapshot>) + 'static,
) -> gpui::Subscription {
    let view = cx.entity().downgrade();
    cx.on_keyboard_layout_change(move |cx| {
        let snapshot = sync(cx.keyboard_layout().name());
        let _ = view.update(cx, |view, cx| {
            apply(view, snapshot);
            cx.notify();
        });
    })
}

/// Sample "ctrl-c / g g": a part the group cannot settle keeps its canonical legend, and RTL isolates each alternative left to right.
pub fn chord(canonical: &str, snapshot: Option<&KeycapSnapshot>, rtl: bool) -> String {
    let alternatives = canonical.split(" / ").map(|alternative| {
        let text = match snapshot {
            Some(snapshot) => alternative
                .split(' ')
                .map(|part| snapshot.label(part).unwrap_or(part))
                .collect::<Vec<_>>()
                .join(" "),
            None => alternative.to_owned(),
        };
        if rtl && !text.is_empty() {
            isolate_ltr(&text)
        } else {
            text
        }
    });
    alternatives.collect::<Vec<_>>().join(" / ")
}
