//! Pointer input for chrome and pane breadcrumbs.
use super::{
    strip::{Hit, Tap},
    view::{Out, PathView},
};
use gpui::{Context, Window};

impl PathView {
    /// A press on a strip: a parent opens on the first tap, and only inert ground answers a double click.
    pub fn press_strip(
        &mut self,
        slot: usize,
        x: f32,
        taps: usize,
        modifiers: u8,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.inputs.blocked {
            return;
        }
        let side = slot.saturating_sub(1).min(1);
        if slot > 0 {
            self.inputs.side = side;
            cx.emit(Out::Focus(side));
        }
        let hit = self.strips[slot].hit(x);
        match self.strips[slot].press(hit, taps, modifiers) {
            Tap::Open(path) => cx.emit(Out::Open {
                side: if slot == 0 { self.inputs.side } else { side },
                path,
            }),
            Tap::Edit => self.open(window, cx),
            Tap::Nothing => {}
        }
    }

    pub fn hover_strip(&mut self, slot: usize, x: Option<f32>, cx: &mut Context<Self>) {
        let hit: Option<Hit> = x.map(|x| self.strips[slot].hit(x));
        if self.strips[slot].hover(hit) {
            cx.notify();
        }
    }
}
