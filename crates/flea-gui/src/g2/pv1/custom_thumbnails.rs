//! Thumbnail reads are limited to the visible filmstrip and the two adjacent files.
use super::{custom_chrome::band, QuickLook};
use gpui::{Context, RenderImage, Window};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

fn image_thumbnail(image: image::RgbaImage) -> Arc<RenderImage> {
    Arc::new(RenderImage::new(vec![image::Frame::new(image)]))
}

fn cached_thumbnail(path: &Path, mtime: i64, max: (u32, u32)) -> Option<Arc<RenderImage>> {
    let thumbnail = match flea::backend::thumbcache::Cache::new().lookup(path, mtime, "") {
        flea::backend::thumbcache::Hit::Ready(path) => image::open(path).ok()?.to_rgba8(),
        flea::backend::thumbcache::Hit::Failed | flea::backend::thumbcache::Hit::Miss => {
            return None;
        }
    };
    Some(image_thumbnail(image::imageops::thumbnail(
        &thumbnail, max.0, max.1,
    )))
}

fn pdf_thumbnail(path: &PathBuf, max: (u32, u32)) -> Option<Arc<RenderImage>> {
    let document = crate::g2::pv2::poppler::Document::open(path).ok()?;
    let (page_w, page_h) = document.page_size(0)?;
    let width = (f64::from(max.0).min(f64::from(max.1) * page_w / page_h))
        .round()
        .max(1.) as u32;
    let raster = document.render(0, width)?;
    let image = image::RgbaImage::from_raw(raster.width, raster.height, raster.bgra)?;
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(image)])))
}

impl QuickLook {
    pub(super) fn custom_thumbnails(&mut self, window: &Window, cx: &mut Context<Self>) {
        let current_path = self
            .custom
            .as_ref()
            .and_then(|custom| custom.path())
            .map(str::to_owned);
        let office_thumb = self
            .subject
            .as_ref()
            .and_then(|subject| subject.body.doc())
            .and_then(|doc| match &doc.view {
                super::doc::View::Office(office) => office.read(cx).thumb.clone(),
                _ => None,
            });
        let Some(custom) = &mut self.custom else {
            return;
        };
        if let (Some(path), Some(image)) = (current_path, office_thumb) {
            custom.thumbnails.insert(path, Some(image));
        }
        let shown = custom.filmstrip_visible();
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
            .map(|i| {
                let item = &custom.items[i];
                (
                    item.path.clone(),
                    item.row.mtime,
                    flea::ui_logic_pv1::kinds::quick_look_kind(
                        &item.row.icon,
                        &item.row.name,
                        false,
                    ),
                )
            })
            .collect();
        custom
            .thumbnails
            .retain(|p, _| paths.iter().any(|(path, _, _)| path == p));
        if custom.thumbnail_task.is_some() {
            return;
        }
        let pending: Vec<_> = paths
            .into_iter()
            .filter(|(path, _, _)| !custom.thumbnails.contains_key(path))
            .collect();
        if pending.is_empty() {
            return;
        }
        for (path, _, _) in &pending {
            custom.thumbnails.insert(path.clone(), None);
        }
        let scale = window.scale_factor();
        let target = ((76. * scale) as u32, (56. * scale) as u32);
        custom.thumbnail_task = Some(cx.spawn(async move |this, cx| {
            let decoded = cx
                .background_executor()
                .spawn(async move {
                    pending
                        .into_iter()
                        .map(|(path, mtime, kind)| {
                            let source = PathBuf::from(&path);
                            let image =
                                cached_thumbnail(&source, mtime, target).or_else(|| match kind {
                                    flea::ui_logic_pv1::kinds::State::Image => {
                                        super::photo::decode_sharp(&source, target, &|| true).ok()
                                    }
                                    flea::ui_logic_pv1::kinds::State::Pdf => {
                                        pdf_thumbnail(&source, target)
                                    }
                                    _ => None,
                                });
                            (path, image)
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
