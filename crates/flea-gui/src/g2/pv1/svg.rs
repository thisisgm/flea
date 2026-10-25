//! The card's vector raster: the open SVG drawn at its fit, and each retired picture dropped from the atlas.
use super::*;

impl QuickLook {
    /// The open vector's raster at the drawn fit times the window scale (#336); a raster file draws itself.
    pub(super) fn svg_image(
        &mut self,
        path: &Path,
        fit_w: f64,
        fit_h: f64,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> RasterState {
        use flea::ui_logic_pv1::pixels::{decode_size, is_vector_name, needs_redecode};
        let vector = path.file_name().and_then(|n| n.to_str()).is_some_and(is_vector_name);
        if !vector {
            return RasterState::default();
        }
        let scale = f64::from(window.scale_factor());
        let (dw, dh) = decode_size(fit_w, fit_h, scale, 0.0, 0.0, true);
        if dw == 0 || dh == 0 {
            return RasterState::default();
        }
        let fresh = self.svg.as_ref().is_some_and(|s| {
            s.path == path && s.size == (dw, dh) && !needs_redecode(s.scale, scale)
        });
        if fresh {
            return self.svg.as_ref().map(SvgPicture::state).unwrap_or_default();
        }
        // The same path keeps drawing its last raster until the new one lands; a new path retires it.
        let prior = self.svg.take();
        let keep = prior.as_ref().filter(|s| s.path == path).and_then(|s| s.image.clone());
        if let Some(prior) = prior {
            if prior.path != path {
                self.retire(prior.image, cx);
            }
        }
        self.svg = Some(SvgPicture {
            path: path.to_path_buf(),
            scale,
            size: (dw, dh),
            image: keep,
            refusal: None,
        });
        // A new request invalidates the job in flight; the next one starts when that job ends.
        self.raster_latest.fetch_add(1, Ordering::SeqCst);
        self.start_raster(cx);
        self.svg.as_ref().map(SvgPicture::state).unwrap_or_default()
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
        let epoch = self.raster_latest.load(Ordering::SeqCst);
        let raster = self
            .raster
            .clone()
            .unwrap_or_else(|| raster::default_raster(cx.svg_renderer()));
        self.raster_in_flight = true;
        // The read, parse and raster run on the background executor, never in the frame that asked.
        let job = raster::Job {
            path: path.clone(),
            size,
            epoch,
            latest: self.raster_latest.clone(),
            raster,
        };
        let work = cx.background_executor().spawn(async move { raster::run(&job) });
        cx.spawn(async move |this, cx| {
            let landed = work.await;
            let _ = this.update(cx, |ql, cx| {
                ql.raster_in_flight = false;
                let current = ql.raster_latest.load(Ordering::SeqCst) == epoch
                    && ql.svg.as_ref().is_some_and(|s| s.path == path && s.size == size);
                if !current {
                    // This request moved on; the latest wanted size starts now, still one job at a time.
                    ql.start_raster(cx);
                    return;
                }
                match landed {
                    raster::RasterOutcome::Raster(image) => {
                        if let Some(svg) = ql.svg.as_mut() {
                            svg.refusal = None;
                            if let Some(old) = svg.image.replace(image) {
                                ql.retire(Some(old), cx);
                            }
                        }
                    }
                    raster::RasterOutcome::Refused(refusal) => {
                        if let Some(svg) = ql.svg.as_mut() {
                            svg.refusal = Some(refusal);
                        }
                    }
                    raster::RasterOutcome::Superseded => {}
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
                    for image in std::mem::take(&mut ql.retired) {
                        let _ = window.drop_image(image);
                    }
                });
            });
        });
    }

    /// Retires the open vector's raster and invalidates every in-flight request for it.
    pub(super) fn retire_svg(&mut self, cx: &mut Context<Self>) {
        let image = self.svg.take().and_then(|s| s.image);
        self.retire(image, cx);
        let _ = self.raster_latest.fetch_add(1, Ordering::SeqCst);
    }
}
