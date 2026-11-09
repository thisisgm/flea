//! Save's New Folder… names the folder in a card, then makes it with the exclusive mkdir and enters it.
use super::filename::{FieldKey, Filename};
use super::Picker;
use crate::g2::file_ops::{palette::CardPalette, press::Press, view};
use flea::{
    json::escape,
    jsondoc::{self, Json},
    ui_logic_file_ops::card::{ButtonState, Kind},
    ui_logic_picker_portal as logic,
};
use gpui::{
    div, prelude::*, px, App, ClickEvent, Context, FocusHandle, KeyDownEvent, MouseButton,
    Stateful, Window,
};

// Unlock's error line under its field, 16 px at text size 14.
const ERROR_LINE: f32 = 16.;

impl Picker {
    // A card on screen or a create out holds the chooser's own actions.
    pub(super) fn folder_busy(&self) -> bool {
        self.chrome.folder.is_some() || self.chrome.folder_request.is_some()
    }

    pub(super) fn open_new_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !logic::saving(&self.req)
            || !self.listing.location.starts_with('/')
            || self.backend_unavailable()
            || self.accept_state().submitting()
            || self.folder_busy()
        {
            return;
        }
        let catalog = &self.locale.catalog;
        let tracking = crate::g2::status_feedback::caption::tracking_for(catalog.language());
        // The window's own New Folder name and the field name its prompt uses.
        let (seed, label) = (catalog.tr("prompt.newFolder"), catalog.tr("prompt.folder"));
        let field = cx.new(|cx| Filename::folder_name(&seed, label, tracking, cx));
        self.chrome.folder_edits = Some(cx.subscribe(&field, |picker, _, event, cx| {
            if matches!(event, FieldKey::Edited(_)) {
                picker.folder_edited(cx);
            }
        }));
        let focus = field.read(cx).focus.clone();
        self.chrome.folder = Some(field);
        self.chrome.folder_parent = self.listing.location.clone();
        self.chrome.folder_error.clear();
        self.chrome.folder_pointer = Default::default();
        window.focus(&focus, cx);
        cx.notify();
    }

    // An edit clears a standing refusal, so the error line and frame go the moment the name changes.
    fn folder_edited(&mut self, cx: &mut Context<Self>) {
        if self.chrome.folder_error.is_empty() {
            return;
        }
        self.chrome.folder_error.clear();
        if let Some(field) = &self.chrome.folder {
            field.update(cx, |field, cx| {
                field.refused = false;
                cx.notify();
            });
        }
        cx.notify();
    }

    // Cancel, esc and a made folder close the card; a create still out then holds the chooser (restore.rs).
    fn end_new_folder(&mut self, cx: &mut Context<Self>) {
        self.chrome.folder = None;
        self.chrome.folder_edits = None;
        self.chrome.folder_error.clear();
        self.chrome.folder_parent.clear();
        self.chrome.folder_pointer = Default::default();
        // PickerWindow.qml focusDefault: Save As takes the keyboard back.
        self.browse.focus_filename = true;
        cx.notify();
    }

    fn folder_name(&self, cx: &App) -> String {
        self.chrome
            .folder
            .as_ref()
            .map(|field| field.read(cx).text().to_owned())
            .unwrap_or_default()
    }

    // Create waits on a name, as New File's Create does, and on any create already out.
    fn can_create_folder(&self, cx: &App) -> bool {
        self.chrome.folder_request.is_none()
            && !self.backend_unavailable()
            && !self.folder_name(cx).is_empty()
    }

    // The name goes as typed; the backend refuses what no folder can be called and never renumbers it.
    fn create_folder(&mut self, cx: &mut Context<Self>) {
        if !self.can_create_folder(cx) {
            return;
        }
        let parent = self.chrome.folder_parent.clone();
        let line = format!(
            r#"{{"c":"mkdir","path":"{}","name":"{}"}}"#,
            escape(&parent),
            escape(&self.folder_name(cx))
        );
        if self.checks.send(&line) {
            self.chrome.folder_request = Some(parent);
        } else {
            let said = self.locale.catalog.tr("picker.browser.save_stopped_field");
            self.refuse_folder(said, cx);
        }
        cx.notify();
    }

    fn refuse_folder(&mut self, sentence: String, cx: &mut Context<Self>) {
        self.chrome.folder_error = sentence;
        if let Some(field) = &self.chrome.folder {
            field.update(cx, |field, cx| {
                field.refused = true;
                cx.notify();
            });
        }
    }

    pub(super) fn folder_reply(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        let Ok(document) = jsondoc::parse(text) else {
            return false;
        };
        let kind = document.get("t").and_then(Json::as_str).unwrap_or_default();
        let refusal =
            kind == "error" && document.get("where").and_then(Json::as_str) == Some("mkdir");
        if kind != "made" && !refusal {
            return false;
        }
        let Some(parent) = self.chrome.folder_request.take() else {
            return true;
        };
        if refusal {
            // The window's own New Folder sentence, so a taken name reads the same in both.
            let message = document
                .get("msg")
                .and_then(Json::as_str)
                .unwrap_or_default();
            let sentence = flea::ui_logic_status_feedback::errors::sentence("mkdir", message, "")
                .render(&self.locale.catalog);
            if self.chrome.folder.is_some() {
                self.refuse_folder(sentence, cx);
            } else {
                self.say(sentence, true, cx);
            }
        } else {
            self.end_new_folder(cx);
            if parent == self.listing.location {
                if let Some(path) = document.get("path").and_then(Json::as_str) {
                    if std::path::Path::new(path).parent() == Some(std::path::Path::new(&parent)) {
                        self.open_folder(path, cx);
                    }
                }
            }
        }
        cx.notify();
        true
    }

    // The card holds the keyboard: Tab walks its field and buttons, Enter creates, esc closes it.
    pub(super) fn folder_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(field) = self.chrome.folder.clone() else {
            return;
        };
        let key = event.keystroke.key.as_str();
        let typing = field.read(cx).focus.is_focused(window);
        if typing && field.read(cx).entry.marked.is_some() {
            field.update(cx, |field, cx| field.key(event, cx));
            return;
        }
        match key {
            "tab" => self.step_folder_focus(event.keystroke.modifiers.shift, window, cx),
            "escape" => self.end_new_folder(cx),
            _ if typing => {
                match field.update(cx, |field, cx| field.key(event, cx)) {
                    FieldKey::Enter => self.create_folder(cx),
                    // Typed text arrives through the field's input handler, so an unhandled key goes on to it.
                    FieldKey::Ignored => return,
                    FieldKey::Edited(_) | FieldKey::Moved | FieldKey::Tab => {}
                }
            }
            "enter" | "space" if self.chrome.folder_cancel.is_focused(window) => {
                self.end_new_folder(cx)
            }
            "enter" | "space" if self.chrome.folder_create.is_focused(window) => {
                self.create_folder(cx)
            }
            "enter" => self.create_folder(cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    fn folder_stops(&self, cx: &App) -> Vec<FocusHandle> {
        let mut stops = Vec::new();
        if let Some(field) = &self.chrome.folder {
            stops.push(field.read(cx).focus.clone());
        }
        stops.push(self.chrome.folder_cancel.clone());
        if self.can_create_folder(cx) {
            stops.push(self.chrome.folder_create.clone());
        }
        stops
    }

    fn step_folder_focus(&mut self, back: bool, window: &mut Window, cx: &mut Context<Self>) {
        let stops = self.folder_stops(cx);
        let at = stops.iter().position(|focus| focus.is_focused(window));
        let next = at.map_or(0, |at| {
            if back {
                (at + stops.len() - 1) % stops.len()
            } else {
                (at + 1) % stops.len()
            }
        });
        if let Some(stop) = stops.get(next) {
            window.focus(stop, cx);
        }
        cx.notify();
    }

    // While the card is open the keyboard stays on one of its stops; true when the card owns focus this frame.
    pub(super) fn place_folder_focus(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.chrome.folder.is_none() {
            return false;
        }
        let stops = self.folder_stops(cx);
        if !stops.iter().any(|focus| focus.is_focused(window)) {
            if let Some(field) = stops.first() {
                window.focus(field, cx);
            }
        }
        true
    }

    // 0401-picker-new-folder: the window's New File card, named New Folder, over the whole chooser.
    pub(super) fn folder_card(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<gpui::Div>> {
        let field = self.chrome.folder.clone()?;
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let palette =
            CardPalette::from_theme(&window.text_style(), theme, crate::compositor::corner(cx));
        let face = crate::list::Typeface::for_window(window, cx);
        let catalog = self.locale.catalog.clone();
        let size = super::size::Size::of(window, cx);
        let create = self.can_create_folder(cx);
        let picker = cx.entity().downgrade();
        let wire = move |press: Press, button: Stateful<gpui::Div>| {
            let (hover, down, up, click) = (
                picker.clone(),
                picker.clone(),
                picker.clone(),
                picker.clone(),
            );
            button
                .on_hover(move |hovered, _, cx| {
                    let _ = hover.update(cx, |picker, cx| {
                        let pointer = &mut picker.chrome.folder_pointer;
                        if *hovered {
                            pointer.hovered = Some(press);
                        } else if pointer.hovered == Some(press) {
                            pointer.hovered = None;
                        }
                        cx.notify();
                    });
                })
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    let _ = down.update(cx, |picker, cx| {
                        picker.chrome.folder_pointer.pressed = Some(press);
                        cx.notify();
                    });
                })
                .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                    let _ = up.update(cx, |picker, cx| {
                        picker.chrome.folder_pointer.pressed = None;
                        cx.notify();
                    });
                })
                // Enter and Space act in folder_key on their press; gpui's keyboard click would act a second time.
                .on_click(move |event: &ClickEvent, _, cx| {
                    if event.is_keyboard() {
                        return;
                    }
                    let _ = click.update(cx, |picker, cx| match press {
                        Press::NewFileCancel => picker.end_new_folder(cx),
                        _ => picker.create_folder(cx),
                    });
                })
        };
        let pointer = self.chrome.folder_pointer;
        let scene = view::Scene {
            copy_field: None,
            frame: None,
            typeface: &face,
            palette: &palette,
            catalog: &catalog,
            pointer: &pointer,
            wire: &wire,
            pinned: None,
            feedback: None,
        };
        // Unlock's line under the field: held empty until a create fails, so the buttons never move.
        let line = div()
            .debug_selector(|| "picker-folder-error".into())
            .flex_none()
            .h(size.px(ERROR_LINE))
            .line_height(size.px(ERROR_LINE))
            .text_size(px(size.caption))
            .text_color(palette.error_on_surface)
            .child(self.chrome.folder_error.clone());
        let state = |primary, focused, disabled| ButtonState {
            primary,
            focused,
            disabled,
            ..ButtonState::default()
        };
        let buttons = vec![
            view::button(
                &scene,
                Press::NewFileCancel,
                catalog.tr("jobs.button.cancel"),
                state(false, self.chrome.folder_cancel.is_focused(window), false),
            )
            .track_focus(&self.chrome.folder_cancel),
            view::button(
                &scene,
                Press::NewFileConfirm,
                catalog.tr("jobs.newfile.create"),
                state(true, self.chrome.folder_create.is_focused(window), !create),
            )
            .track_focus(&self.chrome.folder_create),
        ];
        let viewport = window.viewport_size();
        let card = view::shell(
            &scene,
            Kind::NewFile,
            (f32::from(viewport.width), f32::from(viewport.height)),
            catalog.tr("menu.row.new_folder"),
            vec![field.into_any_element(), line.into_any_element()],
            buttons,
        )
        .debug_selector(|| "picker-folder-card".into());
        Some(view::scrim(&palette, 0, card))
    }
}
