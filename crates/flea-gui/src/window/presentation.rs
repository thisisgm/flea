use super::locale::{self, Snapshot as LocaleSnapshot};
use crate::{
    list::Typeface,
    presentation::{Fonts, Service, Snapshot},
    state::{labels, Shell},
};
use flea::ui_logic_st1::textsize;
use gpui::{Context, Window};

impl Shell {
    pub fn follow_locale(&mut self, service: &locale::Service, cx: &mut Context<Self>) {
        self.locale_service = Some(service.clone());
        let updates = service.subscribe();
        self._locale_reader = Some(cx.spawn(async move |this, cx| {
            while let Ok(snapshot) = updates.recv().await {
                if this
                    .update(cx, |shell, cx| shell.apply_locale(snapshot, cx))
                    .is_err()
                {
                    break;
                }
            }
        }));
    }

    pub(crate) fn apply_locale(&mut self, locale: LocaleSnapshot, cx: &mut Context<Self>) {
        if self.locale.revision == locale.revision {
            return;
        }
        self.rail_locale(&locale.catalog, cx);
        self.labels = labels(&locale);
        for host in self.panes.iter_mut().flatten() {
            host.translated(&locale);
        }
        self.locale = locale;
        self.output_translated(cx);
        self.settings_translated(cx);
        if let Some(chrome) = &self.chrome.view {
            let path = chrome.read_with(cx, |view, _| view.path.clone());
            path.update(cx, |view, _| view.set_catalog(self.locale.catalog.clone()));
        }
        cx.notify();
    }

    pub fn follow_presentation(
        &mut self,
        service: &Service,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.presentation_service = Some(service.clone());
        let updates = service.subscribe();
        self._presentation_reader = Some(cx.spawn_in(window, async move |this, cx| {
            while let Ok(snapshot) = updates.recv().await {
                if cx
                    .update(|window, cx| {
                        this.update(cx, |shell, cx| {
                            shell.apply_presentation(snapshot, window, cx);
                        })
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
    }

    pub(crate) fn apply_presentation(
        &mut self,
        snapshot: Snapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let unchanged = cx.try_global::<Fonts>() == Some(&snapshot.fonts)
            && cx.try_global::<crate::palette::Theme>() == Some(&snapshot.theme)
            && cx.try_global::<crate::palette::ParsedTheme>() == Some(&snapshot.parsed);
        if unchanged {
            self.sync_font_settings(cx);
            return;
        }
        snapshot.install(cx);
        self.font_key = None;
        self.sync_font_settings(cx);
        if let Some(overlay) = &self.overlay {
            let palette = crate::g2::op1::palette::CardPalette::from_theme(
                &window.text_style(),
                snapshot.theme,
                self.corner,
            );
            overlay.update(cx, |overlay, cx| {
                overlay.palette = palette;
                cx.notify();
            });
        }
        window.refresh();
        cx.notify();
    }

    pub(crate) fn active_typeface(&self) -> Typeface {
        self.panes[self.focus_side]
            .as_ref()
            .map(|host| host.list.borrow().typeface.clone())
            .unwrap_or_else(Typeface::omarchy_default)
    }

    pub(crate) fn sync_font_settings(&mut self, cx: &mut Context<Self>) {
        let stored = Fonts::stored(&self.launch.settings.data);
        let fonts = match cx.try_global::<Fonts>() {
            Some(fonts) => fonts,
            None if textsize::following(stored) => return,
            None => {
                let fonts =
                    Fonts::from_sources("", "", self.active_typeface().font.family.to_string());
                return self.apply_fonts(fonts, stored, cx);
            }
        };
        if self
            .font_key
            .as_ref()
            .is_some_and(|(previous, mode)| previous == fonts && *mode == stored)
        {
            return;
        }
        self.apply_fonts(fonts.clone(), stored, cx);
    }

    fn apply_fonts(&mut self, fonts: Fonts, stored: textsize::Stored, cx: &mut Context<Self>) {
        if self
            .font_key
            .as_ref()
            .is_some_and(|(previous, mode)| previous == &fonts && *mode == stored)
        {
            return;
        }
        let face = fonts.typeface(&self.launch.settings.data);
        for host in self.panes.iter_mut().flatten() {
            let mut list = host.list.borrow_mut();
            list.set_typeface(face.clone());
            host.pane.set_row_height(list.file_metrics().row);
        }
        for surface in &self.surfaces {
            surface.restyle(&face, cx);
        }
        self.restyle_settings(&face, cx);
        self.font_key = Some((fonts, stored));
    }
}
