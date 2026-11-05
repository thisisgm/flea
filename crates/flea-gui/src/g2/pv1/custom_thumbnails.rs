//! Thumbnail reads are limited to the visible filmstrip and the two adjacent files.
use super::{custom_chrome::band, QuickLook};
use gpui::{Context, Window};
use std::path::PathBuf;

impl QuickLook {
    pub(super) fn custom_thumbnails(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(custom) = &mut self.custom else {
            return;
        };
        let shown = custom.filmstrip && !custom.fullscreen;
        let range = if shown {
            band(
                custom.items.len(),
                custom.current,
                f32::from(window.viewport_size().width),
            )
        } else if custom.options.preload {
            custom.current.saturating_sub(1)..(custom.current + 2).min(custom.items.len())
        } else {
            0..0
        };
        let paths: Vec<_> = range
            .filter_map(|i| {
                let item = &custom.items[i];
                (flea::ui_logic_pv1::kinds::quick_look_kind(&item.row.icon, &item.row.name, false)
                    == flea::ui_logic_pv1::kinds::State::Image)
                    .then(|| item.path.clone())
            })
            .collect();
        custom.thumbnails.retain(|p, _| paths.contains(p));
        if custom.thumbnail_task.is_some() {
            return;
        }
        let pending: Vec<_> = paths
            .into_iter()
            .filter(|p| !custom.thumbnails.contains_key(p))
            .collect();
        if pending.is_empty() {
            return;
        }
        for path in &pending {
            custom.thumbnails.insert(path.clone(), None);
        }
        let scale = window.scale_factor();
        let target = ((144. * scale) as u32, (96. * scale) as u32);
        custom.thumbnail_task = Some(cx.spawn(async move |this, cx| {
            let decoded = cx
                .background_executor()
                .spawn(async move {
                    pending
                        .into_iter()
                        .map(|p| {
                            let image =
                                super::photo::decode_sharp(&PathBuf::from(&p), target, &|| true)
                                    .ok();
                            (p, image)
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if let Some(custom) = &mut view.custom {
                    for (path, image) in decoded {
                        if let Some(slot) = custom.thumbnails.get_mut(&path) {
                            *slot = image;
                        }
                    }
                    custom.thumbnail_task = None;
                }
                cx.notify();
            });
        }));
    }
}
