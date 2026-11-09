//! The rail's rows as picker-open draws them: the keyboard's row on the accent wash and edge, the listing's folder on the grey fill.
use super::rail::WIDTH;
use super::Picker;
use crate::g2::file_ops::glyph::{Glyph, Paint};
use crate::g2::rail_places::rows::glyph;
use crate::list::Palette;
use flea::ui_logic_rail_places::rail::{Group, Kind, RailEntry};
use gpui::{div, prelude::*, px, Context, MouseButton, WeakEntity, Window};

// picker-open: Theme.space(172) at body 14 and its 24% rule, rows 29 px from a 7 px top, 14 px sides, a 15 px mark 9 px before its label.
const TOP: f32 = 7.;
const ROW: f32 = 29.;
const SIDE: f32 = 14.;
const GAP: f32 = 9.;
const MARK: f32 = 15.;
const DOT: f32 = 6.;
const RULE: f32 = 0.24;
const CURSOR_WASH: f32 = 0.18;
const PLACE_FILL: f32 = 0.18;
const UNMOUNTED: f32 = 0.5;
// The keyboard row's accent edge and the mount square's inset from the rail's edge.
const EDGE: f32 = 2.;
const DOT_INSET: f32 = 2.;

// SidebarRow.qml showsDot: shares, removable volumes and phones carry a mount square.
pub(super) fn dot(entry: &RailEntry) -> Option<bool> {
    ((entry.group == Group::Network && entry.kind != Kind::Dropbox)
        || (entry.group == Group::Device && matches!(entry.kind, Kind::Volume | Kind::Phone)))
    .then(|| entry.mounted.unwrap_or(false))
}

// The probe id a rail row's laid-out box is kept under, read back by the controls reader.
pub(super) fn rail_row(index: usize) -> String {
    format!("picker-rail-row-{index}")
}

/// The rail's own view beside the listing; it reads the picker and draws, and every press goes back to it.
pub struct RailView {
    pub picker: WeakEntity<Picker>,
}

impl Render for RailView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(picker) = self.picker.upgrade() else {
            return div().id("picker-rail");
        };
        let owner = picker.read(cx);
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let palette = Palette::inherited(&window.text_style(), theme);
        let focused = owner.rail.focus.is_focused(window);
        let current = owner.listing.location.as_str();
        let caption = crate::list::Typeface::for_window(window, cx).caption;
        let size = super::size::Size::of(window, cx);
        let rows = owner.rail.entries.iter().enumerate().map(|(index, entry)| {
            let label = owner.rail.labels.get(index).cloned().unwrap_or_default();
            // The keyboard's row takes the accent wash and edge; otherwise the folder the listing stands on takes the grey fill.
            let cursor = focused && index == owner.rail.cursor;
            let place = !focused && !entry.path.is_empty() && entry.path == current;
            let ink = if entry.error.is_some() {
                palette.error
            } else {
                palette.foreground
            };
            let handle = picker.clone();
            owner
                .chrome
                .probe
                .masked(div().relative(), &format!("rail:{index}"))
                .id(("picker-rail-row", index))
                .debug_selector(move || format!("picker-rail-row-{index}"))
                .relative()
                .flex_none()
                .flex()
                .flex_row()
                .items_center()
                .gap(size.px(GAP))
                .h(size.px(ROW))
                .px(size.px(SIDE))
                .whitespace_nowrap()
                .when(cursor, |row| {
                    row.bg(palette.accent.opacity(CURSOR_WASH)).child(
                        div()
                            .absolute()
                            .left_0()
                            .top_0()
                            .bottom_0()
                            .w(px(EDGE))
                            .bg(palette.accent_frame),
                    )
                })
                .when(place, |row| row.bg(palette.foreground.opacity(PLACE_FILL)))
                .child(owner.chrome.probe.mark(rail_row(index)))
                .children(glyph::svg(entry.glyph).map(|(key, data)| {
                    div().flex_none().child(Glyph {
                        paint: Paint::Svg(key, data),
                        side: size.len(MARK),
                        ink: palette.foreground,
                    })
                }))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .text_color(ink)
                        .child(label),
                )
                .children(dot(entry).map(|mounted| {
                    // SidebarRow.qml RailEject: a releasable row draws the eject mark in the square's own slot.
                    if entry.releasable {
                        let ink = if cursor {
                            palette.foreground
                        } else {
                            palette.muted_frame
                        };
                        div()
                            .flex_none()
                            .debug_selector(move || format!("picker-rail-eject-{index}"))
                            // SidebarRow.qml's row tap skips a press on the mark, and PickerPlaces.qml connects no eject.
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .children(glyph::svg("eject").map(|(key, data)| Glyph {
                                paint: Paint::Svg(key, data),
                                side: f32::from(caption),
                                ink,
                            }))
                    } else {
                        div()
                            .flex_none()
                            .debug_selector(move || format!("picker-rail-dot-{index}"))
                            .mr(px(DOT_INSET))
                            .size(size.px(DOT))
                            .bg(if mounted {
                                palette.executable
                            } else {
                                palette.muted_frame.opacity(UNMOUNTED)
                            })
                    }
                }))
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    handle.update(cx, |picker, cx| {
                        picker.rail.cursor = index;
                        cx.notify();
                    });
                    // A press leaves the keyboard where it was, as SidebarRow's tap does: picker-recent-list keeps it on the files.
                    cx.stop_propagation();
                })
                .on_click({
                    let handle = picker.clone();
                    move |_, _, cx| handle.update(cx, |picker, cx| picker.rail_choose(index, cx))
                })
        });
        div()
            .id("picker-rail")
            .debug_selector(|| "picker-rail".into())
            .flex_none()
            .h_full()
            .w(px(WIDTH + 1.))
            .pt(size.px(TOP))
            .border_r_1()
            .border_color(palette.foreground.opacity(RULE))
            .bg(palette.background)
            .text_size(px(size.body))
            .text_color(palette.foreground)
            .track_focus(&owner.rail.focus)
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .children(rows)
    }
}
