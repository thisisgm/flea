use super::locale::{self, Snapshot as LocaleSnapshot};
use crate::{
    list::Typeface,
    locale_geometry::Direction,
    presentation::{Fonts, Service, Snapshot},
    state::{labels, Shell},
};
use flea::ui_logic_settings_pages::textsize;
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

    /// The direction Omarchy's locale registry gives the language; the window mirrors and reads keys by it.
    pub(crate) fn mirrored(&self) -> bool {
        Direction::from_locale(self.locale.catalog.locale()).is_rtl()
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
        self.previews_translated(cx);
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
        super::glass_tag::follow(snapshot.theme.glass);
        self.font_key = None;
        self.sync_font_settings(cx);
        let face = self.active_typeface();
        let env = self.menu_env(&face);
        let menus = std::iter::once(&self.menu.menu).chain(
            self.rail
                .iter()
                .filter_map(|join| join.network.as_ref())
                .map(|network| &network.menu),
        );
        for menu in menus {
            let entries = menu.borrow().card.as_ref().map(|card| card.entries.clone());
            if let Some(entries) = entries {
                menu.borrow_mut().refresh(entries, &env, window);
            }
        }
        self.restyle_trash_menu(&face, window);
        self.restyle_frames(window, cx);
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
        // Windows without the shell's root read the global, so they follow the same face.
        cx.set_global(face.clone());
        for host in self.panes.iter_mut().flatten() {
            let mut list = host.list.borrow_mut();
            list.set_typeface(face.clone());
            host.pane.set_row_height(list.file_metrics().row);
            host.filter.strip.borrow_mut().typeface = face.clone();
            host.search.set_typeface(face.clone());
        }
        for surface in &self.surfaces {
            surface.restyle(&face, cx);
        }
        self.restyle_settings(&face, cx);
        // Views that read the face at render keep their last frame until told, so a font-set repaints each one.
        if let Some(trash) = &self.trash {
            trash.pane.update(cx, |_, cx| cx.notify());
            trash.messages.update(cx, |_, cx| cx.notify());
        }
        if let Some(chrome) = &self.chrome.view {
            chrome.update(cx, |_, cx| cx.notify());
        }
        for surface in &self.surfaces {
            if let Some(footer) = surface.footer.clone() {
                let line = footer.read(cx).line.clone();
                line.update(cx, |_, cx| cx.notify());
                footer.update(cx, |_, cx| cx.notify());
            }
        }
        self.font_key = Some((fonts, stored));
    }
}
