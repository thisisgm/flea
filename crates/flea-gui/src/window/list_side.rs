//! A preview beside the List view.
//!
//! The list pane gives up the right part of its width. A file under the cursor draws there through the
//! ordinary selection preview; a folder draws its own entries, folders first. On while `preview.column`
//! is on (Alt+P) and `preview.list` is not false; `preview.listWidth` is the percent of the pane it takes.
use super::composition::{Geometry, Rect};
use crate::{list::Palette, palette::Theme, state::Shell};
use flea::jsondoc::Json;
use gpui::{div, prelude::*, px, AnyElement, Context, SharedString, Window};

/// The list keeps at least this much room; below twice of it there is no preview.
const LIST_MIN: f32 = 360.;
const SIDE_MIN: f32 = 240.;

pub(crate) fn enabled(data: &Json) -> bool {
    let preview = data.get("preview").unwrap_or(&Json::Null);
    preview.get("column").and_then(Json::as_bool) != Some(false)
        && preview.get("list").and_then(Json::as_bool) == Some(true)
}

fn share(data: &Json, live: Option<f32>) -> f32 {
    if let Some(live) = live {
        return live;
    }
    data.get("preview")
        .and_then(|preview| preview.get("listWidth"))
        .and_then(Json::as_f64)
        .map_or(0.4, |percent| (percent / 100.).clamp(0.2, 0.7)) as f32
}

/// Cuts the preview from the pane's right, past a one-pixel hairline.
/// live is the share of an edge being dragged in this window, which wins over the stored one.
pub(crate) fn split(pane: &mut Rect, data: &Json, live: Option<f32>) -> Option<Rect> {
    if pane.width < LIST_MIN + SIDE_MIN + 1. {
        return None;
    }
    let width = (pane.width * share(data, live))
        .floor()
        .clamp(SIDE_MIN, pane.width - LIST_MIN - 1.);
    pane.width -= width + 1.;
    Some(Rect {
        x: pane.x + pane.width + 1.,
        y: pane.y,
        width,
        height: pane.height,
    })
}

/// The held edge between the list and its preview.
#[derive(Clone, Copy)]
pub(crate) struct SideEdge;

struct EdgeGhost;

impl gpui::Render for EdgeGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// The window follows a held edge anywhere, and a release anywhere ends it.
pub(crate) fn edge_listeners<E: InteractiveElement>(root: E, cx: &mut Context<Shell>) -> E {
    root.on_drag_move::<SideEdge>(cx.listener(
        |shell, event: &gpui::DragMoveEvent<SideEdge>, window, cx| {
            shell.list_side_drag(f32::from(event.event.position.x), window, cx);
        },
    ))
    .capture_any_mouse_up(cx.listener(|shell, _, window, cx| {
        shell.list_side_release(window, cx);
    }))
}

fn edge(cx: &mut Context<Shell>) -> impl IntoElement {
    div()
        .id("list-side-edge")
        .absolute()
        .left_0()
        .top_0()
        .h_full()
        .w(px(10.))
        .cursor(gpui::CursorStyle::ResizeLeftRight)
        .on_drag(SideEdge, |_, _, _, cx| cx.new(|_| EdgeGhost))
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(cx.listener(|shell, event: &gpui::ClickEvent, window, cx| {
            // A double click on the edge goes back to the default width.
            if event.click_count() == 2 {
                shell.list_side_live.set(None);
                shell.store_list_width(40., window, cx);
            }
        }))
}

impl Shell {
    /// The preview beside the List view, at the rect the geometry cut for it.
    pub(crate) fn list_side(
        &self,
        geometry: &Geometry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        // Any frame that draws no folder peek leaves the one it drew: its release and its read in flight go.
        let (Some(rect), Some(host)) = (geometry.side, self.panes[0].as_ref()) else {
            super::list_peek::leave();
            return None;
        };
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let palette = Palette::inherited(&window.text_style(), theme);
        let typeface = host.list.borrow().typeface.clone();
        let frame = div()
            .absolute()
            .left(px(rect.x - 1.))
            .top(px(rect.y))
            .w(px(rect.width + 1.))
            .h(px(rect.height))
            .overflow_hidden()
            .border_l_1()
            .border_color(palette.muted_frame)
            .font(typeface.font.clone());
        let row = host.pane.held.row(host.pane.cursor);
        let folder = row
            .filter(|row| row.dir && !row.name.is_empty() && host.selected.len() <= 1)
            .map(|row| super::list_peek::join(host.pane.path(), &row.name));
        let Some(path) = folder else {
            super::list_peek::leave();
            let preview = host.preview.clone().filter(|view| view.read(cx).visible);
            return Some(
                frame
                    .child(div().relative().size_full().children(preview))
                    .child(edge(cx))
                    .into_any_element(),
            );
        };
        let hidden = self
            .launch
            .settings
            .data
            .get("hidden")
            .and_then(Json::as_bool)
            == Some(true);
        // The folder is read only where the preview loads by itself, or once Load released it.
        let class = host
            .preview
            .as_ref()
            .and_then(|preview| preview.read(cx).class.clone())
            .filter(|(directory, _)| directory == host.pane.path())
            .map_or_else(|| "?".to_owned(), |(_, class)| class);
        let data = &self.launch.settings.data;
        let automatic = super::selection_preview::host::settings(data).automatic;
        let released = super::list_peek::released(&path, host.pane.held.listing);
        let preview = data.get("preview").unwrap_or(&Json::Null);
        let rule = |class: &str| super::selection_preview::host::loads(preview, automatic, class);
        let loads = released || (class != "?" && rule(&class));
        if loads {
            // Unreleased, the folder's own storage is checked too: a mount point under a local parent may be held.
            let gate = (!released).then(|| super::list_peek::Gate {
                parent: host.pane.path().to_owned(),
                loads: super::list_mount::CLASSES.map(rule),
            });
            self.peek(&path, hidden, gate, cx);
        } else {
            super::list_peek::leave();
        }
        let name = row.map(|row| row.name.clone()).unwrap_or_default();
        let shown = super::list_peek::shown(&path, hidden, released);
        let hint = || {
            self.locale
                .catalog
                .tr_args("preview.loadHint", &["Ctrl+Space"])
        };
        let (summary, entries) = match shown.filter(|_| loads) {
            None if !loads && (class != "?" || !automatic) => (hint(), Vec::new()),
            None => ("…".to_owned(), Vec::new()),
            Some(super::list_peek::Answer::Held) => (hint(), Vec::new()),
            Some(super::list_peek::Answer::Failed(error)) => (error, Vec::new()),
            Some(super::list_peek::Answer::Listed(entries, total)) => (
                match total {
                    0 => self.locale.catalog.tr("folder.empty"),
                    n => self.locale.catalog.trn("items.count", n as i64),
                },
                entries,
            ),
        };
        let metrics = crate::list::Metrics::for_typeface(&typeface);
        let line = px(metrics.row);
        let pad = px(metrics.pad);
        let head = div()
            .flex()
            .flex_col()
            .flex_none()
            .px(pad)
            .pt(pad)
            .pb(pad)
            .child(
                div()
                    .text_size(typeface.body)
                    .text_color(palette.foreground)
                    .truncate()
                    .child(SharedString::from(format!("\u{f07b}  {name}"))),
            )
            .child(
                div()
                    .text_size(typeface.caption)
                    .text_color(palette.metadata)
                    .child(SharedString::from(summary)),
            );
        let rows = entries.into_iter().map(|(name, dir)| {
            let (mark, ink) = if dir {
                ("\u{f07b}", palette.accent)
            } else {
                ("\u{f15b}", palette.muted)
            };
            div()
                .flex()
                .items_center()
                .flex_none()
                .h(line)
                .px(pad)
                .gap(pad)
                .text_size(typeface.body)
                .child(div().flex_none().text_color(ink).child(mark))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(if dir {
                            palette.accent
                        } else {
                            palette.foreground
                        })
                        .child(SharedString::from(name)),
                )
        });
        Some(
            frame
                .child(
                    div()
                        .size_full()
                        .flex()
                        .flex_col()
                        .child(head)
                        .child(div().flex().flex_col().overflow_hidden().children(rows)),
                )
                .child(edge(cx))
                .into_any_element(),
        )
    }

    fn store_list_width(&mut self, percent: f64, window: &mut Window, cx: &mut Context<Self>) {
        let leaf = Json::Obj(vec![("listWidth".into(), Json::Num(format!("{percent}")))]);
        let patch = Json::Obj(vec![("preview".into(), leaf)]);
        self.change_settings(patch, window, cx);
    }

    /// The held edge follows the pointer: the preview takes what lies right of it, within 20 to 70 percent.
    pub(crate) fn list_side_drag(&mut self, x: f32, window: &mut Window, cx: &mut Context<Self>) {
        let geometry = self.geometry(window);
        let Some(side) = geometry.side else { return };
        let left = geometry.panes[0].x;
        let right = side.x + side.width;
        if right - left <= 0. {
            return;
        }
        self.list_side_live
            .set(Some(((right - x) / (right - left)).clamp(0.2, 0.7)));
        cx.notify();
    }

    /// A list preview edge released where the body never saw the release still settles.
    pub(crate) fn settle_list_side(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.list_side_live.get().is_some() && !cx.has_active_drag() {
            self.list_side_release(window, cx);
        }
    }

    /// Release: the held share becomes preview.listWidth, in whole percent.
    pub(crate) fn list_side_release(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(live) = self.list_side_live.get() else {
            return;
        };
        self.store_list_width((f64::from(live) * 100.).round(), window, cx);
        self.list_side_live.set(None);
        cx.notify();
    }
}
