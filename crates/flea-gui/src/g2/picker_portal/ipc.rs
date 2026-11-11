//! The fleapicker IPC target tests/picker.sh reads, answered from the chooser's own state.
use super::picker::Picker;
use crate::ipc::server::Answer;
use flea::ipc::{
    registry,
    wire::{Reply, Request},
};
use flea::json::escape;
use flea::ui_logic_picker_portal as logic;

// What only drawn geometry can answer; the listing and chrome owners supply it at the join.
pub trait PickerGeometry {
    fn answers(&self, _: &str) -> bool {
        true
    }
    fn controls(&self) -> String;
    fn row_centre(&self, index: usize) -> String;
    fn cohesion_controls(&self) -> String;
    fn save_field_centre(&self) -> String;
    // Whether the shared location entry is open, read from the drawn path view.
    fn path_bar_open(&self) -> bool;
    // The snapshot members only drawn surfaces know (jump, geometry, focus, theme), as `"key":value` pairs.
    fn snapshot_members(&self) -> String;
}

fn quoted(text: &str) -> String {
    format!("\"{}\"", escape(text))
}

impl Picker {
    fn checks_json(&self) -> String {
        let state = self.accept_state();
        format!(
            r#"{{"marksBusy":{},"saveBusy":{},"submitting":{},"canAccept":{},"saveReady":{},"collision":{},"review":{}}}"#,
            self.mark_request > 0,
            self.save_request > 0,
            state.submitting(),
            logic::can_accept(&state),
            state.save_ready(),
            state.save_collision(),
            self.review.as_ref().map_or(0, |review| review.review)
        )
    }

    fn uris_json(&self) -> String {
        let uris: Vec<String> = self.marks.iter().map(|mark| quoted(&mark.uri)).collect();
        format!("[{}]", uris.join(","))
    }

    fn snapshot(&self, geometry: &dyn PickerGeometry) -> String {
        let listing = &self.listing;
        let state = self.accept_state();
        let rows: Vec<String> = listing
            .names
            .iter()
            .map(|name| format!("{{\"n\":{}}}", quoted(name)))
            .collect();
        let marks: Vec<String> = self
            .marks
            .iter()
            .map(|mark| {
                format!(
                    r#"{{"path":{},"bytes":{},"uri":{}}}"#,
                    quoted(&mark.path),
                    mark.bytes,
                    quoted(&mark.uri)
                )
            })
            .collect();
        let message = self.message.as_ref();
        let members = geometry.snapshot_members();
        format!(
            concat!(
                r#"{{"path":{},"total":{},"held":{},"rows":[{}],"cursor":{},"cursorName":{},"marks":[{}],"state":{},"#,
                r#""listingFailed":{},"filter":{},"marksBusy":{},"saveBusy":{},"submitting":{},"backendUnavailable":{},"#,
                r#""canAccept":{},"saveReady":{},"collision":{},"collisionReview":{},"saveName":{},"saveError":{},"#,
                r#""message":{},"messageError":{},"hints":{},"title":{},"app":{}{}{}}}"#
            ),
            quoted(&listing.location),
            listing.total,
            listing.held,
            rows.join(","),
            listing.cursor,
            quoted(listing.row(listing.cursor).unwrap_or_default()),
            marks.join(","),
            quoted(listing.state),
            listing.failed,
            listing.chip,
            self.mark_request > 0,
            self.save_request > 0,
            state.submitting(),
            self.backend_unavailable(),
            logic::can_accept(&state),
            state.save_ready(),
            state.save_collision(),
            state.collision_review(),
            quoted(&listing.save_name),
            quoted(&self.save_error),
            quoted(message.map_or("", |message| message.text.as_ref())),
            message.is_some_and(|message| message.error),
            quoted(&self.hints()),
            quoted(&self.labels.title),
            quoted(&self.req.app),
            if members.is_empty() { "" } else { "," },
            members,
        )
    }

    // `quit` answers like the window's own close: the host runs window_closed, never a bare exit.
    pub fn ipc(&self, request: Request, geometry: Option<&dyn PickerGeometry>) -> Answer {
        let Request::Call {
            target,
            function,
            arguments,
        } = request
        else {
            return Answer {
                reply: registry::show(),
                quit: false,
            };
        };
        // Only fleapicker is loaded, so absent flea targets precede function and argument validation.
        if target == "flea" {
            return Answer {
                reply: registry::no_target(),
                quit: false,
            };
        }
        let function = match registry::validate(&target, &function, &arguments) {
            Ok(function) if function.target == "fleapicker" => function,
            Ok(function) => {
                return Answer {
                    reply: registry::refused(function),
                    quit: false,
                }
            }
            Err(reply) => return Answer { reply, quit: false },
        };
        let index = arguments
            .first()
            .and_then(|text| text.trim().parse::<i64>().ok())
            .and_then(|index| usize::try_from(index).ok());
        let geometry = geometry.filter(|geometry| geometry.answers(function.name));
        let listing = &self.listing;
        let value = match (function.name, geometry) {
            ("version", _) => env!("CARGO_PKG_VERSION").to_owned(),
            ("quit", _) => {
                return Answer {
                    reply: Reply {
                        code: 0,
                        stdout: String::new(),
                        stderr: String::new(),
                    },
                    quit: true,
                }
            }
            ("ready", _) => "true".into(),
            ("path", _) => listing.location.clone(),
            ("total", _) => listing.total.to_string(),
            ("shownTotal", _) => listing.shown_total.to_string(),
            ("cursor", _) => listing.cursor.to_string(),
            ("marks", _) => self
                .marks
                .iter()
                .map(|mark| mark.path.as_str())
                .collect::<Vec<_>>()
                .join(","),
            ("rowAt", _) => index
                .and_then(|index| listing.row(index))
                .unwrap_or_default()
                .to_owned(),
            ("cursorName", _) => listing.row(listing.cursor).unwrap_or_default().to_owned(),
            ("state", _) => listing.state.to_owned(),
            ("recent", _) => logic::is_recent(&listing.location).to_string(),
            ("accept", _) => self
                .labels
                .accept(&self.locale.catalog, &self.req, self.marks.len())
                .to_string(),
            ("chip", _) => listing.chip.to_string(),
            ("saveName", _) => listing.save_name.clone(),
            ("message", _) => self
                .message
                .as_ref()
                .map_or_else(String::new, |message| message.text.to_string()),
            ("checks", _) => self.checks_json(),
            ("markedUris", _) => self.uris_json(),
            ("controls", Some(geometry)) => geometry.controls(),
            ("rowCentre", Some(geometry)) => index
                .map(|index| geometry.row_centre(index))
                .unwrap_or_default(),
            ("cohesionControls", Some(geometry)) => geometry.cohesion_controls(),
            ("pathBarOpen", Some(geometry)) => geometry.path_bar_open().to_string(),
            ("saveState", Some(geometry)) => format!(
                r#"{{"name":{},"path":{},"collision":{},"error":{},"field":{}}}"#,
                quoted(&listing.save_name),
                quoted(
                    self.review
                        .as_ref()
                        .map_or("", |review| review.path.as_str())
                ),
                self.accept_state().save_collision(),
                quoted(&self.save_error),
                quoted(&geometry.save_field_centre())
            ),
            ("snapshot", Some(geometry)) => self.snapshot(geometry),
            _ => {
                return Answer {
                    reply: registry::refused(function),
                    quit: false,
                }
            }
        };
        Answer {
            reply: Reply::value(value),
            quit: false,
        }
    }
}
