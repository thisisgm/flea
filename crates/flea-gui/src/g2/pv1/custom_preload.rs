//! Two adjacent stills can supply first paint. Native decoding still owns final quality.
use super::QuickLook;
use gpui::{Context, RenderImage, Window};
use std::{path::PathBuf, sync::Arc};

pub(crate) struct Preloaded {
    pub identity: crate::g2::pv1::photo_source::Identity,
    pub image: Arc<RenderImage>,
}
impl QuickLook {
    pub(super) fn custom_preload(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(custom) = &mut self.custom else {
            return;
        };
        if !custom.options.preload {
            custom.preloads.clear();
            return;
        }
        if custom.preload_task.is_some() {
            return;
        }
        let paths: Vec<_> = [custom.current.checked_sub(1), custom.current.checked_add(1)]
            .into_iter()
            .flatten()
            .filter_map(|i| custom.items.get(i))
            .filter(|item| {
                flea::ui_logic_pv1::kinds::quick_look_kind(&item.row.icon, &item.row.name, false)
                    == flea::ui_logic_pv1::kinds::State::Image
            })
            .map(|i| i.path.clone())
            .collect();
        let current = custom.path().map(str::to_owned);
        custom
            .preloads
            .retain(|p, _| paths.contains(p) || Some(p) == current.as_ref());
        let pending: Vec<_> = paths
            .into_iter()
            .filter(|p| !custom.preloads.contains_key(p) && !custom.preload_failed.contains(p))
            .collect();
        if pending.is_empty() {
            return;
        }
        let scale = window.scale_factor();
        let frame = custom.card(window.viewport_size()).size;
        let target = (
            (f32::from(frame.width) * scale).clamp(1., 1024.) as u32,
            (f32::from(frame.height) * scale).clamp(1., 1024.) as u32,
        );
        custom.preload_task = Some(cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_executor()
                .spawn(async move {
                    pending
                        .into_iter()
                        .map(|name| {
                            let path = PathBuf::from(&name);
                            let image = (|| {
                                let identity = crate::preview_quality::reuse::file_identity(&path)?;
                                let header = flea::backend::imagesize::header(&path)?;
                                let (w, h) = if header.orientation >= 5 {
                                    (header.height, header.width)
                                } else {
                                    (header.width, header.height)
                                };
                                let fit = (target.0 as f64 / w.max(1) as f64)
                                    .min(target.1 as f64 / h.max(1) as f64)
                                    .min(1.);
                                let target = (
                                    (w as f64 * fit).round().max(1.) as u32,
                                    (h as f64 * fit).round().max(1.) as u32,
                                );
                                let image = super::photo::decode(&path, target, &|| true).ok()?;
                                (crate::preview_quality::reuse::file_identity(&path)
                                    == Some(identity))
                                .then_some(Preloaded { identity, image })
                            })();
                            (name, image)
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if let Some(custom) = &mut view.custom {
                    for (name, image) in loaded {
                        if let Some(image) = image {
                            custom.preloads.insert(name, image);
                        } else {
                            custom.preload_failed.insert(name);
                        }
                    }
                    custom.preload_task = None;
                    // Failed reads are remembered only within this bounded neighborhood.
                    if custom.preload_failed.len() > 8 {
                        custom.preload_failed.clear();
                    }
                }
                cx.notify();
            });
        }));
    }
}
