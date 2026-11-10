//! Language receipts come from the active row painter and the code view's submitted glyphs.
use crate::g2::quick_look_overlay::doc::View as LookView;
use crate::{
    state::Shell,
    window::{
        selection_preview::{docs::Doc, Mounted},
        startup::View,
    },
};
use flea::{ipc::wire::Reply, jsondoc::Json};
use gpui::App;

pub(crate) fn read(shell: &Shell, cx: &App, function: &str, arguments: &[String]) -> Option<Reply> {
    let host = shell.panes[shell.focus_side].as_ref()?;
    let state = match function {
        "langmarksState" => {
            let rows = match host.drawn_view? {
                View::List | View::Dual => {
                    let list = host.list.borrow();
                    let rows = list
                        .language_paint
                        .borrow()
                        .iter()
                        .filter_map(|row| {
                            list.drawn_rows
                                .row(row.index)
                                .map(|facts| row.json(&facts.name))
                        })
                        .collect();
                    rows
                }
                View::Grid => {
                    let grid = host.grid.as_ref()?.state.borrow();
                    let held = grid.frame_held.as_ref()?;
                    let rows = grid
                        .language_paint
                        .borrow()
                        .iter()
                        .filter_map(|row| held.row(row.index).map(|facts| row.json(&facts.name)))
                        .collect();
                    rows
                }
                View::Columns => {
                    let paint = host
                        .columns
                        .as_ref()?
                        .painters
                        .get(crate::columns::geometry::Slot::Active)?
                        .borrow();
                    let rows = paint
                        .language_paint
                        .borrow()
                        .iter()
                        .filter_map(|row| {
                            let drawn = paint.drawn.as_ref()?;
                            crate::columns::element::row_at(
                                &drawn.view().rows,
                                drawn.held.as_deref(),
                                row.index,
                            )
                            .map(|facts| row.json(facts.name()))
                        })
                        .collect();
                    rows
                }
            };
            crate::visual::object([
                (
                    "view",
                    Json::Str(crate::visual::view_name(host.drawn_view?).into()),
                ),
                ("cursor", Json::Num(host.pane.cursor.to_string())),
                ("rows", Json::Arr(rows)),
            ])
        }
        "langmarksCode" if arguments.first().is_some_and(|flag| flag == "true") => {
            let preview = shell.selection_preview(shell.focus_side)?.read(cx);
            if !preview.visible {
                return None;
            }
            let Mounted::Doc(Doc::Code(code)) = &preview.drawn.as_ref()?.mounted else {
                return None;
            };
            code.read(cx).language_capture()
        }
        "langmarksCode" => {
            let look = shell.quick_look.as_ref()?.view.read(cx);
            if !look.is_revealed() || !look.is_open() {
                return None;
            }
            let LookView::Code(code) = &look.subject()?.body.doc()?.view else {
                return None;
            };
            let mut state = code.read(cx).language_capture();
            if let Json::Obj(fields) = &mut state {
                let mark = look.observations.borrow().bar_mark.clone();
                let language = mark.map_or(Json::Null, |(key, ink, bounds)| {
                    let slug = key.strip_prefix("flea-lang:").unwrap_or("");
                    let color = crate::ipc::rows::colour(ink);
                    let size = f32::from(bounds.size.width).to_string();
                    crate::visual::object([
                        ("slug", Json::Str(slug.into())),
                        ("color", Json::Str(color.clone())),
                        ("slotInk", Json::Str(color)),
                        ("size", Json::Num(size.clone())),
                        ("slotSize", Json::Num(size)),
                        ("glyphVisible", Json::Bool(slug.is_empty())),
                    ])
                });
                fields.push(("language".into(), language));
            }
            state
        }
        _ => return None,
    };
    Some(Reply::value(flea::jsondoc::render(&state).trim_end()))
}
