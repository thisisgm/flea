//! Read-only Trash state and centres of rows laid out by the last native frame.
use super::{join::TrashHost, presentation::Stage};
use crate::g2::file_ops::view::{delete_body, delete_title};
use crate::ipc::Adapter;
use flea::{
    ipc::wire::Reply,
    jsondoc::{self, Json},
};
use gpui::App;

pub struct TrashAdapter<'a> {
    pub host: Option<&'a TrashHost>,
    pub message: Option<&'a str>,
    pub overlay: Option<&'a gpui::Entity<crate::g2::file_ops::host::Overlay>>,
    pub cx: &'a App,
}
fn object(fields: Vec<(&str, Json)>) -> Json {
    Json::Obj(
        fields
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    )
}
fn number(value: impl ToString) -> Json {
    Json::Num(value.to_string())
}

fn visible_pixel(value: f32, start: gpui::Pixels, end: gpui::Pixels) -> Option<f32> {
    let first = f32::from(start).ceil();
    let last = f32::from(end).ceil() - 1.;
    (first <= last).then(|| value.round().clamp(first, last))
}

impl TrashAdapter<'_> {
    // The one readiness both readers share: nothing held, the swap Ready and the committed snapshot drawn.
    fn settled(&self, host: &TrashHost, pane: &super::pane::TrashPane) -> bool {
        host.presentation.stage == Stage::Ready
            && !pane.interaction_held()
            && pane.paint.borrow().drawn()
    }

    fn state(&self) -> String {
        let Some(host) = self.host else {
            return jsondoc::render(&object(vec![
                ("opened", Json::Bool(false)),
                ("count", number(0)),
            ]));
        };
        let pane = host.pane.read(self.cx);
        let view = pane.view();
        let confirmation = self
            .overlay
            .and_then(|overlay| {
                let overlay = overlay.read(self.cx);
                let (model, catalog) = (&overlay.model, &overlay.catalog);
                let token = model
                    .external_delete
                    .filter(|token| host.card_token == Some(*token))?;
                let crate::g2::file_ops::model::Dialog::Delete {
                    review,
                    scope,
                    focus,
                    busy,
                    session: _,
                } = model.dialog.as_ref()?
                else {
                    return None;
                };
                Some(object(vec![
                    ("opened", Json::Bool(true)),
                    ("count", number(review.count.unwrap_or(0))),
                    ("token", number(token)),
                    ("all", Json::Bool(review.all)),
                    ("busy", Json::Bool(*busy)),
                    (
                        "destructiveFocus",
                        Json::Bool(*focus == flea::ui_logic_file_ops::trash::Button::Delete),
                    ),
                    ("title", Json::Str(delete_title(catalog, review, *busy))),
                    (
                        "body",
                        Json::Str(delete_body(catalog, review, *scope, *busy)),
                    ),
                ]))
            })
            .unwrap_or_else(|| object(vec![("opened", Json::Bool(false))]));
        let rows = view
            .rows
            .iter()
            .map(|row| {
                object(vec![
                    ("uri", Json::Str(row.uri.clone())),
                    ("original", Json::Str(row.original.clone())),
                    ("size", number(row.size)),
                    ("directory", Json::Bool(row.directory)),
                    ("deleted", row.deleted.clone().map_or(Json::Null, Json::Str)),
                    ("identity", Json::Str(row.identity.clone())),
                    ("selected", Json::Bool(view.is_selected(&row.uri))),
                ])
            })
            .collect();
        jsondoc::render(&object(vec![
            ("opened", Json::Bool(view.opened)),
            ("total", number(view.total)),
            ("count", number(host.count)),
            (
                "countText",
                Json::Str(flea::ui_logic_trash_browser::words::strip_count(
                    &pane.catalog,
                    pane.paint.borrow().total,
                    pane.paint.borrow().bytes,
                )),
            ),
            // One readiness with trashRowCentre: held, loading or not yet drawn is busy, never idle with no centre.
            (
                "busy",
                Json::Bool(view.busy() || (view.opened && !self.settled(host, pane))),
            ),
            ("operationActive", Json::Bool(view.operation_active)),
            ("cursor", number(view.cursor)),
            ("first", number(view.first)),
            ("selectedCount", number(view.selected_count())),
            ("selectionToken", number(view.selection.token)),
            ("selectionCount", number(view.selection.count)),
            (
                "headerLabels",
                Json::Arr(pane.header_labels().into_iter().map(Json::Str).collect()),
            ),
            ("upEnabled", Json::Bool(false)),
            ("rows", Json::Arr(rows)),
            ("confirmation", confirmation),
            (
                "swapState",
                object(vec![
                    (
                        "holds",
                        if host.presentation.last_presented == 0 {
                            Json::Null
                        } else {
                            number(host.presentation.holds)
                        },
                    ),
                    (
                        "fallbacks",
                        if host.presentation.last_presented == 0 {
                            Json::Null
                        } else {
                            number(host.presentation.fallbacks)
                        },
                    ),
                    (
                        "blankFrames",
                        if host.presentation.last_presented == 0 {
                            Json::Null
                        } else {
                            number(host.presentation.blank_frames)
                        },
                    ),
                    (
                        "loadingFrames",
                        if host.presentation.last_presented == 0 {
                            Json::Null
                        } else {
                            number(host.presentation.loading_frames)
                        },
                    ),
                    (
                        "blinkBacks",
                        if host.presentation.last_presented == 0 {
                            Json::Null
                        } else {
                            number(host.presentation.blink_backs)
                        },
                    ),
                    ("presented", number(host.presentation.last_presented)),
                ]),
            ),
        ]))
    }

    fn centre(&self, arguments: &[String]) -> String {
        let Some(host) = self.host else {
            return String::new();
        };
        let pane = host.pane.read(self.cx);
        let view = pane.view();
        if !self.settled(host, pane) {
            return String::new();
        }
        let paint = pane.paint.borrow();
        let Some(index) = arguments
            .first()
            .and_then(|arg| arg.parse::<usize>().ok())
            .filter(|index| {
                view.opened
                    && view.shown
                    && view.row_at(*index).is_some()
                    && paint.painted.contains(index)
            })
        else {
            return String::new();
        };
        let Some(bounds) = paint.bounds else {
            return String::new();
        };
        let Some(visible) = paint.visible_bounds else {
            return String::new();
        };
        let top = f32::from(bounds.origin.y) + index as f32 * view.row_height - paint.scroll_y;
        let y = top + view.row_height / 2.;
        if y < visible.origin.y.into() || y >= visible.bottom().into() {
            return String::new();
        }
        let Some(x) = visible_pixel(
            f32::from(visible.origin.x) + f32::from(visible.size.width) / 2.,
            visible.origin.x,
            visible.right(),
        ) else {
            return String::new();
        };
        let Some(y) = visible_pixel(y, visible.origin.y, visible.bottom()) else {
            return String::new();
        };
        format!("{x} {y}")
    }
}

impl Adapter for TrashAdapter<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
        match function {
            "trashState" => Some(Reply::value(self.state())),
            "trashRowCentre" => Some(Reply::value(self.centre(arguments))),
            "trashControlCentre" => Some(Reply::value(self.control_centre(arguments))),
            "trashEmptyState" => Some(Reply::value(self.empty_state())),
            "lastMessage" if self.message.is_some() => {
                Some(Reply::value(self.message.expect("displayed message")))
            }
            _ if self.host.is_some_and(|host| host.menu.borrow().opened()) => {
                let menu = self.host.expect("open menu").menu.borrow();
                crate::g2::context_menus::ipc::MenuAdapter {
                    menu: &menu,
                    origin: (0., 0.),
                }
                .call(function, arguments)
            }
            _ => None,
        }
    }
}
