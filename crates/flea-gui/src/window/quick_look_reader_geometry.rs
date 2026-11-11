//! Card IPC geometry follows the laid-out views, including an ended media surface's content bounds.
use super::Reader;
use crate::g2::quick_look_overlay::Hit;
use flea::json::escape;

impl Reader<'_> {
    pub(super) fn surface_rect(&self, picture: bool) -> String {
        let bounds = self.open().and_then(|look| {
            let observed = look.observations.borrow();
            if !picture {
                return observed.surface;
            }
            if let Some(super::View::Media(media)) = self.view() {
                let bounds = observed.surface?;
                let player = media.player.read(self.cx);
                if player.state.has_video
                    && player.state.media == flea::ui_logic_media_preview::player::Media::Ended
                {
                    return Some(bounds);
                }
                let frame = player.frame.as_ref()?;
                let size = frame.size(0);
                let [x, y, width, height] = flea::ui_logic_media_preview::picture::fit(
                    size.width.0 as f32,
                    size.height.0 as f32,
                    f32::from(bounds.size.width),
                    (f32::from(bounds.size.height) - crate::g2::media_preview::ink::size::STRIP)
                        .max(0.),
                );
                return Some(gpui::Bounds::new(
                    bounds.origin + gpui::point(gpui::px(x), gpui::px(y)),
                    gpui::size(gpui::px(width), gpui::px(height)),
                ));
            }
            observed.picture
        });
        bounds.map_or_else(String::new, |bounds| {
            format!(
                "{} {} {} {}",
                f32::from(bounds.left()).round(),
                f32::from(bounds.top()).round(),
                f32::from(bounds.size.width).round(),
                f32::from(bounds.size.height).round(),
            )
        })
    }

    pub(super) fn pdf_state(&self) -> String {
        let Some((look, pdf, doc)) = self
            .open()
            .zip(self.pdf())
            .zip(self.doc())
            .map(|((look, pdf), doc)| (look, pdf, doc))
        else {
            return "null".into();
        };
        let Some(viewer) = &pdf.viewer else {
            return "null".into();
        };
        let observed = look.observations.borrow();
        let controls = doc.chrome(&look.i18n, self.cx).strip.iter().map(|item| {
            let hit = if item.index == 5 { Hit::Close } else { Hit::Strip(item.index) };
            let centre = observed.strip.get(item.index).copied().flatten().filter(|_| item.visible).map_or_else(String::new, |bounds| {
                let point = bounds.center();
                format!("{} {}", f32::from(point.x).round(), f32::from(point.y).round())
            });
            let hovered = observed.strip_hovered.get(item.index).copied().unwrap_or(false);
            format!(
                r#"{{"name":"{}","enabled":{},"visible":{},"centre":"{centre}","hovered":{hovered},"pressed":{},"ring":{}}}"#,
                escape(&item.label), item.enabled, item.visible,
                look.pressed() == Some(hit),
                item.focused || (item.enabled && look.focused() == Some(hit)),
            )
        }).collect::<Vec<_>>().join(",");
        format!(
            r#"{{"page":{},"pages":{},"zoom":{},"scrollY":{},"focused":{},"control":{},"controls":[{controls}]}}"#,
            pdf.surface.page,
            pdf.surface.page_count(),
            viewer.zoom,
            pdf.pan.1,
            self.pdf_focused,
            viewer.control.map_or(-1, |index| index as i64),
        )
    }
}
