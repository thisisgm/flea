//! The Print, Convert and Combine card slot: op4's OutputView over the panes, fed by its owner pane's backend.
use crate::{
    g2::{
        file_ops::palette::CardPalette,
        print_convert::{
            host::{OutputEvent, OutputView},
            model::Action,
            seam::{Effect, Ticket},
        },
    },
    palette::Theme,
    state::Shell,
};
use flea::{
    protocol::RowFacts,
    ui_logic_print_convert::{
        output::{self, Kind},
        selection::{Selection, Source},
    },
};
use gpui::{App, AppContext, Context, Entity, FocusHandle, Subscription, Task, Window};
use std::path::Path;
#[path = "output_effects.rs"]
mod effects;
#[path = "output_thumbnails.rs"]
mod thumbnails;

// The footer activity owner for a running output, apart from the two panes' own transfer owners.
const ACTIVITY: u64 = 1 << 32;
// The suffixes LibreOffice converts to PDF, the same document family the backend's office check names.
const OFFICE: &[&str] = &[
    "doc", "docx", "odt", "rtf", "xls", "xlsx", "ods", "ppt", "pptx", "odp",
];

/// The one output card of the window and the pane whose backend serves it.
#[derive(Default)]
pub(crate) struct Join {
    pub(crate) view: Option<Entity<OutputView>>,
    pub(crate) owner: usize,
    /// Whether each pane's backend can convert, None until a formats reply says.
    pub(crate) convert: [Option<bool>; 2],
    /// Each pane's backend's archive formats, None until a formats reply says; Compress offers only these.
    pub(crate) archive: [Option<Vec<String>>; 2],
    /// Each pane's backend's extract tools, which Extract reads per archive tail.
    pub(crate) extract: [flea::ui_logic_context_menus::archive::Extraction; 2],
    generation: u64,
    origin: Option<FocusHandle>,
    events: Option<Subscription>,
    chosen: Option<Subscription>,
    pages: Option<Task<()>>,
}

/// A held row's output kind; the backend classifies again by mime before it runs anything.
pub(crate) fn kind(row: &RowFacts) -> Kind {
    if row.dir {
        return Kind::Other;
    }
    let suffix = output::extension(Path::new(&row.name));
    if suffix == "pdf" {
        Kind::Pdf
    } else if OFFICE.contains(&suffix.as_str()) {
        Kind::Office
    } else if row.icon == "image-x-generic" {
        Kind::Image
    } else if row.icon.starts_with("text-") {
        Kind::Text
    } else {
        Kind::Other
    }
}

impl Shell {
    /// A card is showing: it holds the keyboard and the pointer until it closes.
    pub(crate) fn output_showing(&self, cx: &App) -> bool {
        self.output
            .view
            .as_ref()
            .is_some_and(|view| view.read(cx).model.card.is_some())
    }

    fn output_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<OutputView> {
        if let Some(view) = &self.output.view {
            return view.clone();
        }
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let palette = CardPalette::from_theme(&window.text_style(), theme, self.corner);
        let catalog = self.locale.catalog.clone();
        let view = cx.new(|cx| OutputView::new(catalog, palette, cx));
        self.output.events = Some(cx.subscribe_in(
            &view,
            window,
            |shell, _, event: &OutputEvent, window, cx| {
                shell.output_effect(event.0.clone(), window, cx)
            },
        ));
        self.output.view = Some(view.clone());
        view
    }

    /// Opens a card over one pane's captured paths; a refused selection says why in the footer.
    pub(crate) fn open_output(
        &mut self,
        side: usize,
        action: Action,
        menu: u64,
        files: Vec<Source>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.output_showing(cx)
            || self.card_holds_keyboard(cx)
            || self
                .panes
                .get(side)
                .and_then(Option::as_ref)
                .is_none_or(|host| host.backend.is_none())
        {
            return false;
        }
        self.output.generation += 1;
        let selection = Selection {
            generation: self.output.generation,
            menu,
            files,
        };
        let origin = window.focused(cx);
        let view = self.output_view(window, cx);
        let catalog = self.locale.catalog.clone();
        let opened = view.update(cx, |view, cx| {
            view.set_catalog(catalog, cx);
            view.open(action, selection, window, cx)
        });
        if !opened {
            let error = view.read(cx).model.error.clone();
            if !error.is_empty() {
                self.status_messages(cx, |messages, now| {
                    messages.say(
                        &error,
                        true,
                        "",
                        flea::ui_logic_status_feedback::status::Hint::None,
                        now,
                    )
                });
            }
            return false;
        }
        self.output.owner = side;
        self.output.origin = origin;
        self.sync_output_thumbnails(cx);
        cx.notify();
        true
    }

    /// Ctrl+P prints the focused pane's selection, or its cursor row when nothing is marked.
    pub(crate) fn output_print_key(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let side = self.focus_side;
        let Some(host) = self.panes[side].as_ref() else {
            return false;
        };
        if host.stopped || host.pane.busy() || host.pane.held.total == 0 {
            return false;
        }
        let rows = if host.selected.is_empty() {
            vec![host.pane.cursor]
        } else {
            host.selected.clone()
        };
        let files = rows
            .iter()
            .filter_map(|&row| host.pane.held.row(row))
            .map(|row| Source {
                path: Path::new(&host.path)
                    .join(&row.name)
                    .to_string_lossy()
                    .into_owned(),
                kind: kind(row),
            })
            .collect::<Vec<_>>();
        if files.len() != rows.len() {
            return false;
        }
        self.open_output(side, Action::Print, 0, files, window, cx)
    }

    /// Menu rows Print, Convert and Combine Images open the card over the menu's validated snapshot.
    pub(crate) fn observe_output(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.output.chosen.is_some() {
            return;
        }
        let entity = cx.entity();
        self.output.chosen = Some(cx.subscribe_in(
            &entity,
            window,
            |shell, _, chosen: &crate::g2::context_menus::join::Chosen, window, cx| {
                let action = match chosen.action.as_str() {
                    "print" => Action::Print,
                    "convert" => Action::Convert,
                    "combineImages" => Action::Combine,
                    _ => return,
                };
                let side = chosen.captured.side;
                if !chosen.captured.ready || chosen.captured.background || chosen.paths.is_empty() {
                    return;
                }
                let Some(host) = shell.panes.get(side).and_then(Option::as_ref) else {
                    return;
                };
                let held = chosen
                    .captured
                    .rows
                    .iter()
                    .filter_map(|&row| host.pane.held.row(row))
                    .collect::<Vec<_>>();
                let files = chosen
                    .paths
                    .iter()
                    .map(|path| {
                        let leaf = Path::new(path).file_name().map(|n| n.to_string_lossy());
                        let found = held
                            .iter()
                            .find(|row| Some(row.name.as_str()) == leaf.as_deref());
                        Source {
                            path: path.clone(),
                            kind: found.map_or(Kind::Other, |row| kind(row)),
                        }
                    })
                    .collect();
                if !chosen.claim() {
                    return;
                }
                shell.open_output(side, action, chosen.captured.id, files, window, cx);
            },
        ));
    }

    /// What one backend converts, compresses and extracts, from its formats reply; the menu's provider path reads the same reply.
    pub(crate) fn record_formats(&mut self, side: usize, reply: &flea::jsondoc::Json) {
        self.output.convert[side] = reply.get("convert").and_then(flea::jsondoc::Json::as_bool);
        self.output.archive[side] = Some(
            reply
                .get("archive")
                .and_then(flea::jsondoc::Json::as_array)
                .unwrap_or(&[])
                .iter()
                .filter_map(flea::jsondoc::Json::as_str)
                .map(str::to_owned)
                .collect(),
        );
        let tool = |key| {
            reply
                .get("extract")
                .and_then(|tools| tools.get(key))
                .and_then(flea::jsondoc::Json::as_bool)
                == Some(true)
        };
        self.output.extract[side] = flea::ui_logic_context_menus::archive::Extraction {
            archive: tool("archive"),
            seven_zip: tool("sevenZip"),
            zip: tool("zip"),
        };
    }

    /// A backend line for the card; formats replies also record what that backend converts, compresses and extracts.
    pub(crate) fn output_line(&mut self, side: usize, line: &str, cx: &mut Context<Self>) -> bool {
        if line.contains("\"formats\"") {
            if let Ok(reply) = flea::jsondoc::parse(line) {
                if reply.get("t").and_then(flea::jsondoc::Json::as_str) == Some("formats") {
                    self.record_formats(side, &reply);
                }
            }
        }
        if side != self.output.owner {
            return false;
        }
        let Some(view) = self.output.view.clone() else {
            return false;
        };
        view.update(cx, |view, cx| view.line(line, cx))
    }

    /// Routes a backend event to the card first; true when the card took it and nothing else should.
    pub(crate) fn output_event(
        &mut self,
        side: usize,
        event: &crate::backend::Event,
        cx: &mut Context<Self>,
    ) -> bool {
        use crate::backend::Event;
        match event {
            Event::Producer(line) | Event::Operations(line) => self.output_line(side, line, cx),
            Event::Refused {
                operation,
                path,
                message,
                ..
            } => self.output_refused(side, operation, path, message, cx),
            Event::Drained | Event::Failed(_) => {
                self.output_backend_lost(side, cx);
                false
            }
            _ => false,
        }
    }

    /// A language change reaches an open card's own words.
    pub(crate) fn output_translated(&mut self, cx: &mut Context<Self>) {
        if let Some(view) = self.output.view.clone() {
            let catalog = self.locale.catalog.clone();
            view.update(cx, |view, cx| view.set_catalog(catalog, cx));
        }
    }

    /// A refused request reaches the card as the backend's error line; only its owner's refusals count.
    fn output_refused(
        &mut self,
        side: usize,
        operation: &str,
        path: &str,
        message: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let line = format!(
            r#"{{"t":"error","where":"{}","path":"{}","msg":"{}"}}"#,
            flea::json::escape(operation),
            flea::json::escape(path),
            flea::json::escape(message)
        );
        // A transport-wide refusal ends the card and still reaches the pane and its operations.
        self.output_line(side, &line, cx) && !matches!(operation, "backend" | "read")
    }

    /// A drained or failed owner backend ends any card it serves.
    fn output_backend_lost(&mut self, side: usize, cx: &mut Context<Self>) {
        self.output_line(side, r#"{"t":"error","where":"backend","msg":""}"#, cx);
    }
}
