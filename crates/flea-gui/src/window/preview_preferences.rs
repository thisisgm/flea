//! Preview choices use the normal settings writer and the session's shared sound.
use crate::{g2::{pv1::doc::{Doc, View}, pv3::sound::MediaSound}, state::Shell};
use flea::jsondoc::Json;
use gpui::{Context, Window};
use std::rc::Rc;

pub(crate) fn wrap(doc: &Doc, cx: &gpui::App) -> Option<bool> {
    match &doc.view {
        View::Code(view) => Some(view.read(cx).wrap),
        View::Markdown(view) => Some(view.read(cx).wrap),
        View::Html(view) => Some(view.read(cx).wrap),
        _ => None,
    }
}

impl Shell {
    pub(crate) fn preview_wrap(&self) -> bool {
        self.launch.settings.data.get("preview").and_then(|p| p.get("wrap")).and_then(Json::as_bool) == Some(true)
    }

    pub(crate) fn keep_preview_wrap(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let wrap = self.quick_look.as_ref().and_then(|host| host.view.read(cx).subject())
            .and_then(|subject| subject.body.doc()).and_then(|doc| wrap(doc, cx));
        if let Some(wrap) = wrap.filter(|wrap| *wrap != self.preview_wrap()) {
            self.change_settings(Json::Obj(vec![("preview".into(), Json::Obj(vec![("wrap".into(), Json::Bool(wrap))]))]), window, cx);
        }
    }

    pub(crate) fn ensure_preview_sound(&self, cx: &mut Context<Self>) {
        let shell = cx.entity().downgrade();
        let keep = Rc::new(move |level: u8, cx: &mut gpui::App| {
            let _ = shell.update(cx, |shell, cx| {
                shell.settings.patches.borrow_mut().push(Json::Obj(vec![("preview".into(), Json::Obj(vec![("volume".into(), Json::Num(level.to_string()))]))]));
                cx.notify();
            });
        });
        if cx.try_global::<MediaSound>().is_none() {
            let level = self.launch.settings.data.get("preview").and_then(|p| p.get("volume")).and_then(Json::as_f64).unwrap_or(100.).clamp(0., 100.) as u8;
            MediaSound::install(level, Some(keep), cx);
        } else {
            MediaSound::set_keep(keep, cx);
        }
    }
}
