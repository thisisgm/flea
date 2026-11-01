//! The network card saves a new place through the same locked Favorites operation as the rail.
use crate::state::Shell;
use flea::jsondoc::Json;
use gpui::{Context, Window};

impl Shell {
    pub(crate) fn save_network_favourite(
        &mut self,
        id: u64,
        uri: &str,
        label: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let operation = Json::Obj(vec![
            ("op".into(), Json::Str("add".into())),
            (
                "record".into(),
                Json::Obj(vec![
                    ("label".into(), Json::Str(label.into())),
                    ("path".into(), Json::Str(uri.into())),
                ]),
            ),
        ]);
        if let Err(detail) = flea::favourites::changed(&self.launch.settings.data, &operation) {
            self.network_favourite_result(id, false, &detail, cx);
            return;
        }
        // A save during another Favorites write is refused at once, as Favourites.add was in v0.3.12.
        if !self.settings.favourite_writes.is_empty() {
            self.settings_error("places.favorite.busy", cx);
            let detail = self.locale.catalog.tr("network.save_busy");
            self.network_favourite_result(id, false, &detail, cx);
            return;
        }
        let delayed = self.settings.worker.is_some();
        if delayed {
            self.settings.network_favourite = Some((self.settings.revision + 1, id));
        }
        self.settings_favourite(operation, window, cx);
        if !delayed {
            self.network_favourite_result(id, true, "", cx);
        }
    }

    pub(super) fn finish_network_favourite(
        &mut self,
        through: u64,
        result: Result<&str, &str>,
        cx: &mut Context<Self>,
    ) {
        let Some((revision, id)) = self.settings.network_favourite else {
            return;
        };
        if revision != through {
            return;
        }
        self.settings.network_favourite = None;
        let detail = match result {
            Ok(detail) => detail.into(),
            Err("") => self.locale.catalog.tr("settings.error.not_saved"),
            Err(detail) => detail.into(),
        };
        self.network_favourite_result(id, result.is_ok(), &detail, cx);
    }

    pub(super) fn finish_committed_favourite(&mut self, through: u64, cx: &mut Context<Self>) {
        self.settings.answered(through);
        if self
            .settings
            .favourite_focus
            .is_some_and(|(id, _)| id <= through)
        {
            self.settings.favourite_focus = None;
        }
        let key = "places.favorite.saved_unread";
        let detail = self.locale.catalog.tr(key);
        self.finish_network_favourite(through, Ok(&detail), cx);
        self.settings_error(key, cx);
    }

    fn network_favourite_result(
        &mut self,
        id: u64,
        success: bool,
        detail: &str,
        cx: &mut Context<Self>,
    ) {
        if let Some(network) = self.network() {
            let card = network.card.clone();
            card.update(cx, |card, cx| card.saved(id, success, detail, cx));
        }
    }
}

#[cfg(test)]
#[path = "network_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "network_busy_tests.rs"]
mod busy_tests;
