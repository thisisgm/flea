//! The user's key file (`$XDG_CONFIG_HOME/flea/keys.toml`): read at start and on change, consulted after the key table leaves a listing key unbound.
use crate::state::Shell;
use flea::{
    jsondoc::Json,
    keys::user::{self, UserBinding},
    ui_logic_st1::pages::{user_binding_refusals, UserKeyFacts},
    ui_logic_st2::{contributions, store, wire::Plugin},
};
use gpui::{Context, KeyDownEvent, Modifiers, Task, Window};
use std::{io::Read, os::unix::fs::MetadataExt, path::PathBuf, time::Duration};

#[path = "user_keys_layout.rs"]
pub(crate) mod layout;

#[cfg(test)]
#[path = "user_keys_test_fixture.rs"]
pub(crate) mod test_fixture;

#[cfg(test)]
#[path = "user_keys_projection_tests.rs"]
mod projection_tests;

const POLL: Duration = Duration::from_secs(2);

// Failed reads retry even when a permission repair leaves coarse filesystem timestamps unchanged.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Stamp {
    Absent,
    Retry,
    File {
        inode: u64,
        size: u64,
        changed: (i64, i64),
        mode: u32,
    },
}

#[derive(Default)]
pub(crate) struct Join {
    pub(crate) facts: UserKeyFacts,
    pub(crate) path: Option<PathBuf>,
    stamp: Option<Stamp>,
    pub(crate) said: Vec<String>,
    poll: Option<Task<()>>,
    pub(crate) layout: layout::Cache,
    layout_watch: Option<gpui::Subscription>,
}

/// What a bound key runs: a built-in action from the key table, or a plugin action's `plugin:action` id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Bound {
    Builtin(&'static str),
    Plugin(String),
}

pub(super) fn read(path: &PathBuf, last: Option<Stamp>) -> Option<(Option<Stamp>, user::UserKeys)> {
    let problem = |detail: String| user::UserKeys {
        bindings: Vec::new(),
        problems: vec![user::Problem { line: 0, detail }],
    };
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let stamp = Some(Stamp::Absent);
            return (stamp != last).then(|| (stamp, user::UserKeys::default()));
        }
        Err(error) => return Some((Some(Stamp::Retry), problem(format!("metadata: {error}")))),
    };
    let stamp = Some(Stamp::File {
        inode: metadata.ino(),
        size: metadata.size(),
        changed: (metadata.ctime(), metadata.ctime_nsec()),
        mode: metadata.mode(),
    });
    if stamp == last {
        return None;
    }
    let mut text = String::new();
    // The shared regular-file open is nonblocking even if the checked path becomes a FIFO.
    let result = flea::backend::regfile::open_if_regular(path, 0).and_then(|file| {
        file.take(user::MAX_BYTES as u64 + 1)
            .read_to_string(&mut text)
    });
    let keys = match result {
        Ok(_) => user::parse(&text),
        Err(error) => return Some((Some(Stamp::Retry), problem(format!("read: {error}")))),
    };
    Some((stamp, keys))
}

/// Each switched-on plugin's menu actions, started or not: a bound key starts its plugin the way `flea --action` does.
pub(crate) fn plugin_actions(plugins: &[Plugin], data: &Json) -> Vec<(String, String, String)> {
    plugins
        .iter()
        .filter(|p| {
            p.enabled && p.failure.is_none() && store::enabled(data, &p.manifest.id, p.enabled)
        })
        .flat_map(|p| contributions::menu_actions(&p.manifest).unwrap_or_default())
        .map(|c| (flea::ui_logic_mn2::plugins::leaf(&c), c.label, c.glyph))
        .collect()
}

impl Shell {
    pub(crate) fn run_user_plugin_key(
        &mut self,
        id: &str,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !event.is_held {
            self.run_action(
                id,
                crate::window::external_action::Waiting::none(),
                window,
                cx,
            );
        }
        cx.stop_propagation();
    }

    pub(crate) fn unbound_listing_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if self.keyboard.context == flea::keys::Context::Listing
            && self.panes[self.focus_side]
                .as_ref()
                .is_some_and(|host| host.filter.shown.is_none())
            && crate::window::input::printable(event.keystroke.key_char.as_deref().unwrap_or(""))
        {
            let hint = flea::keys::presentation::hinted(
                &self.locale.catalog,
                "keys.hint.filter",
                self.key_preset(),
                "filter",
            );
            if !hint.is_empty() {
                self.status_messages(cx, |messages, now| {
                    messages.say(
                        &hint,
                        false,
                        "",
                        flea::ui_logic_fb1::status::Hint::None,
                        now,
                    )
                });
                cx.notify();
            }
            cx.stop_propagation();
        }
    }

    pub(crate) fn user_keys_listed(&mut self, line: &str) {
        if let Ok(flea::ui_logic_st2::wire::Reply::Listing(list)) =
            flea::ui_logic_st2::wire::parse(line)
        {
            self.user_keys.facts.catalog = Some(
                list.plugins
                    .iter()
                    .flat_map(|p| contributions::menu_actions(&p.manifest).unwrap_or_default())
                    .map(|c| flea::ui_logic_mn2::plugins::leaf(&c))
                    .collect(),
            );
        }
    }

    /// Starts the read-and-poll loop once per window; the path comes from the config home unless a test set one.
    pub(crate) fn follow_user_keys(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.user_keys.poll.is_some() {
            return;
        }
        // A test names its own file, so no test reads the build host's config home.
        if cfg!(test) && self.user_keys.path.is_none() {
            return;
        }
        if self.user_keys.path.is_none() {
            self.user_keys.path = flea::userfile::config_home()
                .ok()
                .map(|home| home.join(user::FILE));
        }
        let Some(path) = self.user_keys.path.clone() else {
            return;
        };
        let shell = cx.entity().downgrade();
        let handle = window.window_handle();
        self.user_keys.layout_watch = Some(cx.on_keyboard_layout_change(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                let _ = shell.update(cx, |shell, cx| shell.user_keys_changed(window, cx));
            });
        }));
        self.user_keys.poll = Some(cx.spawn_in(window, async move |this, cx| loop {
            let Ok(last) = this.update(cx, |shell, _| shell.user_keys.stamp) else {
                break;
            };
            let path = path.clone();
            let changed = cx
                .background_executor()
                .spawn(async move { read(&path, last) })
                .await;
            if let Some((stamp, keys)) = changed {
                let applied = this.update_in(cx, |shell, window, cx| {
                    shell.user_keys.stamp = stamp;
                    shell.user_keys.facts.keys = keys;
                    shell.user_keys_changed(window, cx);
                });
                if applied.is_err() {
                    break;
                }
            }
            cx.background_executor().timer(POLL).await;
        }));
    }

    /// The bindings, the enabled plugin actions or the preset changed: refresh Settings > Keys and say each new refusal once on stderr.
    pub(crate) fn user_keys_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.project_user_keys(cx);
        let data = &self.launch.settings.data;
        let listed = self
            .plugins
            .host
            .as_ref()
            .map(|host| plugin_actions(&host.read(cx).plugins, data));
        self.user_keys.facts.plugins = listed.unwrap_or_default();
        if self.user_keys.facts.keys.has_plugin_bindings() {
            if self.user_keys.facts.catalog.is_none() {
                if let Some(host) = &self.plugins.host {
                    host.read(cx).refresh();
                }
            }
            // A mime-limited plugin action is offered only once the menu's MIME table is read, so read it before the first press.
            self.load_menu_mimes(window, cx);
        }
        let preset = self.key_preset().to_owned();
        self.user_keys.resolve(&preset);
        let facts = &self.user_keys.facts;
        let path = self
            .user_keys
            .path
            .as_ref()
            .map_or_else(String::new, |p| p.display().to_string());
        let mut lines: Vec<String> = user_binding_refusals(facts, &preset)
            .iter()
            .map(|text| format!("flea: {path}: {}", text.resolve(&self.locale.catalog)))
            .collect();
        lines.extend(
            facts
                .keys
                .problems
                .iter()
                .map(|p| format!("flea: {path}:{}: {}", p.line, p.detail)),
        );
        for line in lines
            .iter()
            .filter(|line| !self.user_keys.said.contains(line))
        {
            eprintln!("{line}");
        }
        self.user_keys.said = lines;
        self.sync_settings_user_keys(cx);
        self.refresh_sheet_keyboard(cx);
    }

    pub(crate) fn sync_settings_user_keys(&mut self, cx: &mut Context<Self>) {
        let facts = self.user_keys.facts.clone();
        if let Some(view) = &self.settings.panel {
            view.update(cx, |view, cx| {
                if view.panel.user_keys.as_ref() != Some(&facts) {
                    view.panel.user_keys = Some(facts);
                    view.panel.rebuild();
                    cx.notify();
                }
            });
        }
    }

    pub(crate) fn key_preset(&self) -> &str {
        self.launch
            .settings
            .data
            .get("keys")
            .and_then(flea::jsondoc::Json::as_str)
            .unwrap_or("default")
    }

    /// The user's binding for a listing key the table left unbound, tried in the key table's own input order.
    pub(crate) fn user_key(
        &self,
        preset: &str,
        event: &KeyDownEvent,
        held: Modifiers,
    ) -> Option<Bound> {
        let facts = &self.user_keys.facts;
        if facts.keys.bindings.is_empty() || self.keyboard.composing(event) {
            return None;
        }
        let verdicts = facts.current_verdicts(preset)?;
        let declares = |id: &str| facts.declares(id);
        let found: Option<&UserBinding> = crate::window::input::event_inputs(event, held)
            .into_iter()
            .flatten()
            .find_map(|input| facts.keys.find(preset, input, &declares, verdicts));
        found.and_then(|binding| {
            if binding.is_plugin() {
                Some(Bound::Plugin(binding.action.clone()))
            } else {
                user::builtin(&binding.action).map(Bound::Builtin)
            }
        })
    }
}
