//! The card's physical picture request and retirement of replaced atlas tiles.
use super::*;

impl QuickLook {
    pub(super) fn full_quality(&self, cx: &gpui::App) -> bool {
        if let Some(picture) = &self.svg {
            return picture.quality.ready;
        }
        let Some(doc) = self.subject.as_ref().and_then(|subject| subject.body.doc()) else {
            return true;
        };
        use super::doc::View;
        match &doc.view {
            View::Pdf(pdf) => crate::preview_quality::pdf_ready(&pdf.read(cx).surface),
            View::Office(office) => {
                crate::preview_quality::pdf_ready(&office.read(cx).pdf.read(cx).surface)
            }
            View::Media(media) => crate::preview_quality::media_ready(media.player.read(cx)),
            _ => true,
        }
    }

    /// Borrow only first paint; its zero request size forces a fresh original-file raster at layout.
    pub(crate) fn picture_interim(
        &mut self,
        path: PathBuf,
        image: Arc<RenderImage>,
        cx: &mut Context<Self>,
    ) {
        if self.svg.is_some() || !self.subject.as_ref().is_some_and(|subject| {
            matches!(&subject.body, Body::Image { path: current, .. } if *current == path)
        }) {
            return;
        }
        self.svg = Some(SvgPicture {
            path,
            scale: 0.,
            size: (0, 0),
            image: Some(image),
            refusal: None,
            quality: Default::default(),
            interim: true,
        });
        self.reveal(cx);
    }

    /// The open picture at its zoomed fit times device scale, bounded by source pixels for a photo.
    pub(super) fn picture_image(
        &mut self,
        path: &Path,
        fit_w: f64,
        fit_h: f64,
        source: (f64, f64),
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> RasterState {
        use flea::ui_logic_pv1::pixels::{decode_size, is_vector_name, needs_redecode};
        let vector = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(is_vector_name);
        let scale = f64::from(window.scale_factor());
        let (dw, dh) = decode_size(
            fit_w * self.zoom,
            fit_h * self.zoom,
            scale,
            source.0,
            source.1,
            vector,
        );
        let (dw, dh) = crate::bench_thumbs::preview_size(source, (dw, dh));
        if dw == 0 || dh == 0 {
            return RasterState::default();
        }
        let fresh = self.svg.as_ref().is_some_and(|s| {
            s.path == path
                && !needs_redecode(s.scale, scale)
                && (s.size == (dw, dh)
                    || (!vector && self.zoom > 1. && s.size.0 >= dw && s.size.1 >= dh))
        });
        if fresh {
            self.refine_picture(window, cx);
            return self.svg.as_ref().map(SvgPicture::state).unwrap_or_default();
        }
        if crate::bench_thumbs::enabled() {
            crate::bench_thumbs::begin("preview", &path.to_string_lossy());
        }
        // The same path keeps drawing its last raster until the new one lands; a new path retires it.
        let prior = self.svg.take();
        let keep = prior
            .as_ref()
            .filter(|s| s.path == path)
            .and_then(|s| s.image.clone());
        let interim = prior
            .as_ref()
            .is_some_and(|prior| prior.path == path && prior.interim);
        if let Some(mut prior) = prior {
            prior.quality.clear(cx);
            if prior.path != path && prior.interim {
                self.retire_interim(prior.image, cx);
            } else if prior.path != path {
                self.retire(prior.image, cx);
            }
        }
        self.svg = Some(SvgPicture {
            path: path.to_path_buf(),
            scale,
            size: (dw, dh),
            image: keep,
            refusal: None,
            quality: Default::default(),
            interim,
        });
        // A new request invalidates the job in flight; the next one starts when that job ends.
        self.raster_latest.fetch_add(1, Ordering::SeqCst);
        self.start_raster(cx);
        self.svg.as_ref().map(SvgPicture::state).unwrap_or_default()
    }

    fn refine_picture(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) {
        if self.raster_in_flight || self.raster.is_some() {
            return;
        }
        let Some(picture) = self.svg.as_mut().filter(|picture| picture.image.is_some()) else {
            return;
        };
        if picture.quality.ready {
            return;
        }
        let size = picture.size;
        let version = (self.raster_latest.load(Ordering::SeqCst), 0);
        if let Some(Ok(image)) =
            picture
                .quality
                .poll(picture.path.clone(), size, version, window, cx)
        {
            if picture
                .image
                .as_ref()
                .is_none_or(|old| !Arc::ptr_eq(old, &image))
            {
                let borrowed = std::mem::take(&mut picture.interim);
                let old = picture.image.replace(image);
                if borrowed {
                    self.retire_interim(old, cx);
                } else {
                    self.retire(old, cx);
                }
                cx.notify();
            }
        }
    }

    /// Starts one raster job for the current request; a job already in flight finishes first.
    pub(super) fn start_raster(&mut self, cx: &mut Context<Self>) {
        if self.raster_in_flight {
            return;
        }
        let Some(svg) = self.svg.as_ref() else {
            return;
        };
        let (path, size) = (svg.path.clone(), svg.size);
        // One sharp decode serves first paint too, after a column interim or for a streamed PNG; a JPEG keeps its bounded first frame.
        let interim = svg.interim;
        let sharp = self.raster.is_none()
            && !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(flea::ui_logic_pv1::pixels::is_host_decoded);
        let epoch = self.raster_latest.load(Ordering::SeqCst);
        let raster = self
            .raster
            .clone()
            .unwrap_or_else(|| raster::for_path(&path, cx.svg_renderer()));
        self.raster_in_flight = true;
        // The read, parse and raster run on the background executor, never in the frame that asked.
        let job = raster::Job {
            path: path.clone(),
            size,
            epoch,
            latest: self.raster_latest.clone(),
            raster,
        };
        let work = cx.background_executor().spawn(async move {
            if !sharp || !(interim || photo::is_png(&job.path)) {
                return (raster::run(&job), false);
            }
            let identity = crate::preview_quality::reuse::file_identity(&job.path);
            let current = || job.latest.load(Ordering::SeqCst) == job.epoch;
            let image = photo::decode_sharp(&job.path, job.size, &current);
            if !current() {
                return (raster::RasterOutcome::Superseded, false);
            }
            match image {
                Ok(image) => {
                    crate::preview_quality::reuse::keep(
                        &job.path,
                        (job.epoch, 0),
                        job.size,
                        &image,
                        identity,
                    );
                    (raster::RasterOutcome::Raster(image), true)
                }
                // A scaled-readable source may exceed the full decoder's bound or codec support.
                Err(()) => (raster::run(&job), false),
            }
        });
        cx.spawn(async move |this, cx| {
            let (landed, sharp) = work.await;
            let _ = this.update(cx, |ql, cx| {
                ql.raster_in_flight = false;
                let current = ql.raster_latest.load(Ordering::SeqCst) == epoch
                    && ql
                        .svg
                        .as_ref()
                        .is_some_and(|s| s.path == path && s.size == size);
                if !current {
                    // This request moved on; the latest wanted size starts now, still one job at a time.
                    ql.start_raster(cx);
                    return;
                }
                match landed {
                    raster::RasterOutcome::Raster(image) => {
                        if let Some(svg) = ql.svg.as_mut() {
                            svg.refusal = None;
                            if sharp {
                                svg.quality.ready = true;
                            }
                            let borrowed = std::mem::take(&mut svg.interim);
                            let old = svg.image.replace(image);
                            if borrowed {
                                ql.retire_interim(old, cx);
                            } else {
                                ql.retire(old, cx);
                            }
                        }
                    }
                    raster::RasterOutcome::Refused(refusal) => {
                        if let Some(svg) = ql.svg.as_mut() {
                            // A refused original must not leave the column's interim posing as the picture.
                            let interim = svg.interim.then(|| svg.image.take()).flatten();
                            svg.interim = false;
                            if svg.image.is_none() {
                                svg.refusal = Some(refusal);
                            }
                            ql.retire_interim(interim, cx);
                        }
                    }
                    raster::RasterOutcome::Superseded => {}
                }
                if !ql.revealed && ql.content_ready(cx) {
                    ql.reveal(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Puts a raster the card no longer draws on the retire list for the window that holds its tile.
    pub(super) fn retire(&mut self, image: Option<Arc<RenderImage>>, cx: &mut Context<Self>) {
        if let Some(image) = image {
            self.retired.push(image);
        }
        self.schedule_retire(cx);
    }

    /// The column may still paint a borrowed interim, so only its last owner frees the tile.
    pub(super) fn retire_interim(
        &mut self,
        image: Option<Arc<RenderImage>>,
        cx: &mut Context<Self>,
    ) {
        self.retire(image.filter(|image| Arc::strong_count(image) == 1), cx);
    }

    /// Frees retired tiles at the end of this effect cycle, so a close frees its tile before the next frame.
    pub(super) fn schedule_retire(&mut self, cx: &mut Context<Self>) {
        if self.retired.is_empty() || self.retire_scheduled {
            return;
        }
        let Some(window) = self.window else {
            return;
        };
        self.retire_scheduled = true;
        let entity = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, app| {
                let _ = entity.update(app, |ql, _| {
                    ql.retire_scheduled = false;
                    ql.drop_retired(window);
                });
            });
        });
    }

    /// Frees every retired tile from the window's atlas.
    pub(super) fn drop_retired(&mut self, window: &mut gpui::Window) {
        for image in std::mem::take(&mut self.retired) {
            #[cfg(test)]
            self.retired_ids.push(image.id);
            let _ = window.drop_image(image);
        }
    }

    /// Retires the open vector's raster and invalidates every in-flight request for it.
    pub(super) fn retire_svg(&mut self, cx: &mut Context<Self>) {
        let (image, interim) = self.svg.take().map_or((None, false), |mut s| {
            s.quality.clear(cx);
            (s.image, s.interim)
        });
        if interim {
            self.retire_interim(image, cx);
        } else {
            self.retire(image, cx);
        }
        let _ = self.raster_latest.fetch_add(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
#[path = "settle_tests.rs"]
mod settle_tests;
