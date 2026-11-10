use crate::{
    state::Shell,
    window::{dispatch::Order, input::ActionEvent, startup::View},
};
use flea::{
    jsondoc::Json,
    ui_logic_settings_pages::{edit, pages, textsize},
};
use gpui::{Context, Window};

impl Shell {
    pub(crate) fn settings_action(
        &mut self,
        action: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match action {
            "settings" => self.open_settings(window, cx),
            "keymapSheet" => self.open_sheet(window, cx),
            "toggleHidden" => {
                // Pane.qml:553: Recent shows every item it lists, so the hidden setting stays put under it.
                if self.recent_refuses(self.focus_side, "places.recent.hidden_refused", cx) {
                    return true;
                }
                let hidden = !pages::is_true(&self.launch.settings.data, &["hidden"]);
                self.change_settings(edit::setting("hidden", Json::Bool(hidden)), window, cx);
            }
            "textSizeUp" | "textSizeDown" | "textSizeReset" => {
                let stored = textsize::parse(
                    pages::at(&self.launch.settings.data, &["display", "textSize"])
                        .unwrap_or(&Json::Null),
                );
                let size = if action == "textSizeReset" {
                    textsize::Stored::Follow
                } else {
                    textsize::stepped(stored, self.settings_base(cx), textsize::direction(action))
                };
                let patch = Json::Obj(vec![(
                    "display".into(),
                    Json::Obj(vec![("textSize".into(), textsize::to_json(&size))]),
                )]);
                self.change_settings(patch, window, cx);
                let base = self.settings_base(cx);
                let preset = pages::word(&self.launch.settings.data, &["keys"], "default");
                // TextSize.js announce: the footer says the new size and its way back, since a chord with the panel shut has no other readout.
                let sentence =
                    textsize::announce_for(size, base, preset).resolve(&self.locale.catalog);
                if let Some(host) = &mut self.panes[self.focus_side] {
                    host.status = sentence.clone().into();
                }
                let catalog = self.locale.catalog.clone();
                let settings_errors = [
                    "settings.error.unreadable",
                    "settings.error.favourites_unread",
                    "settings.error.not_saved",
                ]
                .map(|key| catalog.tr(key));
                let failed = self.settings.messages.as_ref().is_some_and(|line| {
                    line.read(cx)
                        .messages()
                        .errors()
                        .iter()
                        .any(|error| settings_errors.contains(&error.text))
                });
                if failed {
                    self.status_messages(cx, |messages, now| {
                        messages.say_receipt_behind(&catalog, &sentence, now)
                    });
                } else {
                    self.status_note(&sentence, cx);
                }
            }
            "viewList" | "viewColumns" | "viewGrid" | "viewDual" => {
                self.choose_settings_view(
                    match action {
                        "viewColumns" => View::Columns,
                        "viewGrid" => View::Grid,
                        "viewDual" => self.dual_key_view(),
                        _ => View::List,
                    },
                    window,
                    cx,
                );
            }
            _ => return false,
        }
        true
    }

    pub(super) fn settings_host_action(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if id == "viewDual" {
            self.choose_settings_view(View::Dual, window, cx);
        } else if id == "language" {
            self.launch_settings_program(
                "omarchy-menu",
                vec!["summon".into(), "setup.language".into()],
                window,
                cx,
            );
        } else if id == "updateFlea" {
            self.settings_update(true, window, cx);
        } else if matches!(
            id,
            flea::ui_logic_settings_pages::about::ISSUES_URL
                | flea::ui_logic_settings_pages::about::SPONSORS_URL
        ) {
            self.launch_settings_program("xdg-open", vec![id.into()], window, cx);
        } else if id == "makeDefault" {
            self.toggle_settings_claim(window, cx);
        } else if id == "addFavourite" {
            let Some(host) = &self.panes[self.focus_side] else {
                return;
            };
            let path = host.path.clone();
            self.add_favourite(&path, window, cx);
        } else if let Some(index) = id
            .strip_prefix("favourite:")
            .and_then(|s| s.parse::<usize>().ok())
        {
            let row = pages::at(&self.launch.settings.data, &["places", "favourites"])
                .and_then(Json::as_array)
                .and_then(|rows| rows.get(index))
                .cloned();
            // Places.js openEntry: a bad record says why in the status line, and nothing opens.
            if let Some(error) = row
                .as_ref()
                .and_then(flea::ui_logic_rail_places::places::record_error)
            {
                let catalog = self.locale.catalog.clone();
                let label = row
                    .as_ref()
                    .and_then(|row| row.get("label"))
                    .and_then(Json::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let text = catalog.tr_args(
                    "places.favorite.open_failed",
                    &[&label, &catalog.tr(error.key())],
                );
                self.status_messages(cx, |messages, now| {
                    messages.say(
                        &text,
                        true,
                        "",
                        flea::ui_logic_status_feedback::status::Hint::None,
                        now,
                    )
                });
                cx.notify();
                return;
            }
            let path = row
                .as_ref()
                .and_then(|row| row.get("path"))
                .and_then(Json::as_str)
                .map(str::to_owned);
            if let Some(path) = path {
                // SettingsPanel.qml opens a Favorite without dismissing the panel or moving its keyboard focus.
                let _ = self.step(
                    self.focus_side,
                    crate::window::navigate::Step::OpenPlace(path),
                    window,
                    cx,
                );
            }
        } else {
            let Some(action) = flea::keys::binding_rows("default", flea::keys::Frontend::Gui)
                .find(|action| action.action == id)
            else {
                return;
            };
            if !self.settings_action(action.action, window, cx) {
                let event = ActionEvent {
                    action: action.action,
                    context: self.keyboard.context,
                    seat: flea::keys::sequence::Seat::default(),
                    pair_armed: false,
                };
                self.dispatch(Order::Action(event), window, cx);
            }
        }
    }

    fn launch_settings_program(
        &mut self,
        program: &'static str,
        arguments: Vec<String>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        cx.spawn_in(window, async move |this, cx| {
            let ok = cx
                .background_executor()
                .spawn(async move {
                    std::process::Command::new(program)
                        .args(arguments)
                        .status()
                        .is_ok_and(|status| status.success())
                })
                .await;
            if !ok {
                let _ = this.update(cx, |shell, cx| {
                    if let Some(host) = &mut shell.panes[shell.focus_side] {
                        host.status = shell
                            .locale
                            .catalog
                            .tr_args("window.open_failed", &[program])
                            .into();
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }

    /// Favourites.add: the folder joins the favourites under its last path segment, "/" for the root.
    pub(crate) fn add_favourite(
        &mut self,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let label = path
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or("/");
        let operation = Json::Obj(vec![
            ("op".into(), Json::Str("add".into())),
            (
                "record".into(),
                Json::Obj(vec![
                    ("path".into(), Json::Str(path.into())),
                    ("label".into(), Json::Str(label.into())),
                ]),
            ),
        ]);
        self.settings_favourite(operation, window, cx);
    }

    pub(super) fn move_settings_favourite(
        &mut self,
        from: usize,
        to: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = pages::at(&self.launch.settings.data, &["places", "favourites"])
            .cloned()
            .unwrap_or(Json::Arr(Vec::new()));
        let mut fields = vec![
            (
                "op".into(),
                Json::Str(if to.is_some() { "move" } else { "remove" }.into()),
            ),
            ("index".into(), Json::Num(from.to_string())),
            ("expected".into(), current),
        ];
        if let Some(to) = to {
            fields.push(("to".into(), Json::Num(to.to_string())));
        }
        self.settings_favourite(Json::Obj(fields), window, cx);
    }

    pub(crate) fn settings_favourite(
        &mut self,
        operation: Json,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Ok(next) = flea::favourites::changed(&self.launch.settings.data, &operation) else {
            return;
        };
        let Some(favourites) = pages::at(&next, &["places", "favourites"]) else {
            return;
        };
        let patch = Json::Obj(vec![(
            "places".into(),
            Json::Obj(vec![("favourites".into(), favourites.clone())]),
        )]);
        let follow = match operation.get("to") {
            Some(Json::Num(to)) => to.parse::<usize>().ok(),
            _ => None,
        };
        let delayed = self.settings.worker.is_some();
        if delayed {
            self.settings.favourite_focus = follow.map(|to| (self.settings.revision + 1, to));
        }
        self.write_settings(patch, Some(operation), window, cx);
        if !delayed {
            if let Some(to) = follow {
                self.focus_settings_favourite(to, cx);
            }
        }
    }

    pub(super) fn focus_settings_favourite(&mut self, to: usize, cx: &mut Context<Self>) {
        if let Some(view) = &self.settings.panel {
            view.update(cx, |view, cx| {
                if let Some(at) = view
                    .panel
                    .rows
                    .iter()
                    .position(|row| row.favourite == Some(to))
                {
                    view.panel.cursor = at;
                    view.panel.reveal();
                    cx.notify();
                }
            });
        }
    }
}
