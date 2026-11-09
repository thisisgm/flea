//! Backend work and renderer lifetime for a single generation.
use super::*;
use crate::g2::{
    doc_preview::{
        self,
        view::{Metrics, Palette},
    },
    model_preview::{
        self,
        seam::{CacheReader, Wire},
    },
};
use flea::{json::escape, ui_logic_model_preview::control::Surface};
use gpui::{AppContext, ImageAssetLoader, Window};

struct ModelWire(Outbox);
impl Wire for ModelWire {
    fn send(&self, line: String) -> bool {
        self.0.borrow_mut().push(line);
        true
    }
}
struct CopyWire(Outbox);
impl doc_preview::view::CopyHost for CopyWire {
    fn pdf_copy(&self, path: &str, slot: &str) -> u64 {
        let id = token();
        self.0.borrow_mut().push(format!(
            r#"{{"c":"pdfcopy","id":{id},"slot":"{slot}","path":"{}"}}"#,
            escape(path)
        ));
        id
    }
}
impl SelectionPreview {
    pub(crate) fn follow(
        &mut self,
        subject: Option<Subject>,
        manual: bool,
        force: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(subject) = subject else {
            self.cancel_pending(window, cx);
            self.stop_media(window, cx);
            return;
        };
        if force {
            self.manual_subject = Some(subject.clone());
        }
        let manual = manual && self.manual_subject.as_ref() != Some(&subject);
        if self
            .manual_subject
            .as_ref()
            .is_some_and(|old| old != &subject)
        {
            self.manual_subject = None;
        }
        let remote = self
            .class
            .as_ref()
            .is_some_and(|(_, class)| matches!(class.as_str(), "network" | "phone"));
        let same = self
            .pending
            .as_ref()
            .or(self.drawn.as_ref())
            .is_some_and(|f| {
                f.subject == subject && f.remote == remote && (f.manual == manual || force)
            });
        if same && !force {
            self.poll(window, cx);
            return;
        }
        self.cancel_pending(window, cx);
        self.stop_media(window, cx);
        let now = self.now(cx);
        {
            let mut swap = self.port.0.borrow_mut();
            swap.now = now;
            swap.ready = false;
            swap.folder = false;
        }
        self.port.hold("");
        self.port.start(false);
        let generation = token();
        let kind = subject.kind();
        let path = subject.path();
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let metrics = crate::list::Metrics::for_typeface(&self.face);
        let mut watch = None;
        let mounted = match kind {
            Kind::Pdf if !manual => {
                let pdf = cx.new(|cx| {
                    PdfView::new(
                        true,
                        Palette {
                            background: theme.background,
                        },
                        Metrics {
                            gap: metrics.gap,
                            margin: 2. * metrics.pad,
                            row_height: metrics.row,
                        },
                        cx,
                    )
                });
                watch = Some(cx.observe(&pdf, |_, _, cx| cx.notify()));
                let outbox = self.outbox.clone();
                let slow = self
                    .class
                    .as_ref()
                    .is_some_and(|(_, class)| class != "local" && !class.is_empty());
                let (width, height) = self.frame_size(kind, if self.full_facts { 3 } else { 1 });
                pdf.update(cx, |pdf, cx| {
                    pdf.set_copy_host(Box::new(CopyWire(outbox)), "column", slow);
                    pdf.surface.set_frame(
                        (width - 2.).max(1.).into(),
                        (height - 2.).max(1.).into(),
                        window.scale_factor().into(),
                        metrics.gap.into(),
                    );
                    pdf.set_path(&path, window, cx);
                    pdf.set_active(true, window, cx);
                });
                Mounted::Pdf(pdf)
            }
            Kind::Model if !manual => {
                let wire = Rc::new(ModelWire(self.outbox.clone()));
                let model = cx.new(|cx| {
                    Model3dView::new(
                        Surface::Column,
                        wire,
                        Arc::new(CacheReader),
                        self.catalog.clone(),
                        cx,
                    )
                });
                watch = Some(cx.observe(&model, |_, _, cx| cx.notify()));
                model.update(cx, |model, cx| {
                    model.host.animations = false;
                    model.show(&path, 0, cx);
                });
                Mounted::Model(model)
            }
            Kind::Audio | Kind::Video => Mounted::Media(None),
            _ if !manual => {
                let look = self.doc_look(&subject, window, cx);
                let wire = wire::Wire(self.outbox.clone());
                match docs::Doc::mount(kind, &subject, &look, &wire, &self.catalog, window, cx) {
                    Some(doc) => {
                        watch = Some(doc.observe(cx));
                        Mounted::Doc(doc)
                    }
                    None => Mounted::Basic,
                }
            }
            _ => Mounted::Basic,
        };
        let mut frame = Frame {
            subject,
            generation,
            kind,
            mounted,
            meta: None,
            facts: None,
            thumb_answered: !kind.thumbed(),
            thumb_observation: None,
            source: None,
            image: None,
            animation: None,
            animation_source: None,
            quality: Default::default(),
            trace: crate::preview_trace::Trace::begin("column", &path),
            failed: false,
            manual,
            remote,
            _watch: watch,
        };
        if !manual && kind != Kind::Multi {
            // PreviewColumn.qml: the meta reply carries the line count, the media probe and an archive's own index.
            let text = matches!(
                kind,
                Kind::Text | Kind::Code | Kind::Markdown | Kind::Table | Kind::Html
            );
            let media = matches!(kind, Kind::Audio | Kind::Video);
            self.outbox.borrow_mut().push(format!(
                r#"{{"c":"meta","row":{},"token":{generation},"text":{text},"media":{media},"archive":{}}}"#,
                frame.subject.index,
                kind == Kind::Archive
            ));
            if kind.thumbed() {
                self.outbox.borrow_mut().push(frame.thumbnail_request());
            }
        }
        let mut frame = frame;
        self.ask_previewer(&mut frame, window, cx);
        self.pending = Some(frame);
        self.poll(window, cx);
    }

    // The theme and type a mounted document view draws with, read once per mount.
    pub(super) fn doc_look(
        &self,
        subject: &Subject,
        window: &Window,
        cx: &Context<Self>,
    ) -> docs::Look {
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let storage = self.class.as_ref().map_or("", |(_, class)| class.as_str());
        docs::Look {
            text_limit: flea::ui_logic_preview_core::ask::text_limit(storage),
            remote: matches!(storage, "network" | "phone"),
            theme,
            face: self.face.clone(),
            metrics: crate::list::Metrics::for_typeface(&self.face),
            muted: crate::list::Palette::inherited(&window.text_style(), theme).caption(false),
            mark: crate::g2::quick_look_overlay::glyph::Glyph::kind(
                &subject.row.icon,
                subject.row.mode,
            )
            .svg(),
        }
    }

    pub(crate) fn on_line(
        &mut self,
        doc: &Json,
        line: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let text = |key| doc.get(key).and_then(Json::as_str).unwrap_or("");
        let number = |key| doc.get(key).and_then(Json::as_f64).unwrap_or(-1.) as u64;
        // A drawn document keeps asking as it scrolls, so its later replies go to it as well as the pending one.
        let mut taken = false;
        for frame in [self.drawn.as_ref(), self.pending.as_ref()]
            .into_iter()
            .flatten()
        {
            if let Mounted::Doc(view) = &frame.mounted {
                let path = frame.subject.path();
                let mine = text("t") != "meta" || number("token") == frame.generation;
                taken |= mine && view.on_line(doc, line, &path, window, cx);
            }
        }
        let Some(frame) = self.pending.as_mut() else {
            return taken;
        };
        match text("t") {
            "meta" if number("token") == frame.generation => {
                frame.meta = Some(doc.clone());
                frame.facts = flea::ui_logic_preview_core::meta::parse(line);
            }
            "thumbed" if number("row") as usize == frame.subject.index => {
                if !frame.accept_thumbnail(doc) {
                    return taken;
                }
                // PaneWire.onThumbed answers the row too: the cursor row's own thumbnail lands from the same reply.
                cx.notify();
                return taken;
            }
            "model3d" => {
                if let Mounted::Model(model) = &frame.mounted {
                    let accepted = model.update(cx, |model, cx| model.on_line(line, cx));
                    if accepted && text("thumbnail").starts_with('/') {
                        frame.source = Some(std::path::PathBuf::from(text("thumbnail")).into());
                    }
                    return accepted;
                }
                return taken;
            }
            "pdfcopied" => {
                if let Mounted::Pdf(pdf) = &frame.mounted {
                    pdf.update(cx, |pdf, cx| {
                        pdf.copied(
                            number("id"),
                            text("path"),
                            text("err"),
                            doc.get("stalled").and_then(Json::as_bool) == Some(true),
                            cx,
                        )
                    });
                } else {
                    return taken;
                }
            }
            _ => return taken,
        }
        cx.notify();
        true
    }

    // A drawn frame loads too: a previewer plugin's picture can land after its frame was shown.
    fn load_source(frame: &mut Frame, window: &mut Window, cx: &mut Context<Self>) {
        let Some(source) = frame.source.as_ref().filter(|_| frame.image.is_none()) else {
            return;
        };
        if let Some(loaded) = window.use_asset::<ImageAssetLoader>(source, cx) {
            frame.failed |= loaded.is_err();
            frame.image = loaded.ok();
            if frame.image.is_none() {
                frame.source = None;
                if let Mounted::Model(model) = &frame.mounted {
                    model.read(cx).live.borrow_mut().thumbnail = false;
                    frame.failed = false;
                }
            }
        }
    }

    // An animated GIF's frames, from the original over its still: asked while the frame waits and after it lands.
    fn load_animation(frame: &mut Frame, window: &mut Window, cx: &mut Context<Self>) {
        if frame.animation.is_some() || !frame.animated_gif() {
            return;
        }
        if frame.animation_source.is_none() {
            frame.animation_source = Some(std::path::PathBuf::from(frame.subject.path()).into());
        }
        if let Some(source) = &frame.animation_source {
            if let Some(Ok(image)) = window.use_asset::<ImageAssetLoader>(source, cx) {
                if image.frame_count() > 1 {
                    frame.animation = Some(image);
                    cx.notify();
                }
            }
        }
    }

    pub(crate) fn poll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(frame) = self.drawn.as_mut() {
            Self::load_source(frame, window, cx);
            Self::load_animation(frame, window, cx);
        }
        let now = self.now(cx);
        let (width, height) = self.frame_size(
            self.pending.as_ref().map_or(Kind::Facts, |f| f.kind),
            if self.full_facts { 3 } else { 1 },
        );
        let Some(frame) = self.pending.as_mut() else {
            return;
        };
        // A sharp picture that is already decoded wins over a thumbnail that lands in the same poll.
        Self::early_picture(frame, (width, height), window, cx);
        Self::load_source(frame, window, cx);
        Self::load_animation(frame, window, cx);
        let renderer_ready = match &frame.mounted {
            Mounted::Pdf(pdf) => {
                let pdf = pdf.read(cx);
                frame.failed |= pdf.surface.failed();
                pdf.surface.shown_page().is_some() || pdf.surface.failed()
            }
            Mounted::Model(model) => {
                let theme = cx
                    .try_global::<crate::palette::Theme>()
                    .copied()
                    .unwrap_or_default();
                model.update(cx, |model, cx| {
                    model.prepare_column(
                        ((width - 2.).max(1.), (height - 2.).max(1.)),
                        window,
                        theme,
                        cx,
                    )
                });
                let model = model.read(cx);
                frame.failed |= matches!(model.phase, model_preview::view::Phase::Failed(_));
                matches!(model.phase, model_preview::view::Phase::Failed(_))
                    || (model.phase == model_preview::view::Phase::Ready
                        && ((model.live.borrow().thumbnail && frame.image.is_some())
                            || model.presented.borrow().image.is_some()))
            }
            Mounted::Doc(doc) => {
                frame.failed |= doc.failure(&self.catalog, cx).is_some();
                doc.ready(cx)
            }
            _ => {
                (frame.quality.ready && frame.image.is_some())
                    || frame.thumb_answered && (frame.source.is_none() || frame.image.is_some())
            }
        };
        let ready =
            frame.manual || frame.kind == Kind::Multi || (frame.meta.is_some() && renderer_ready);
        let mut swap = self.port.0.borrow_mut();
        swap.now = now;
        let minimum = swap
            .loading_at
            .map(|at| at + crate::pane::swap::MIN_SHOW_MS);
        if ready && minimum.is_none_or(|at| at <= now) {
            observation::release(&mut swap, now, "landed", false);
            swap.ready = true;
            swap.holding = false;
            swap.cap = None;
            swap.loading_at = None;
            drop(swap);
            let next = self.pending.take();
            if let Some(old) = self.drawn.take() {
                self.dispose(old, window, cx);
            }
            self.drawn = next;
            self.timer = None;
            cx.notify();
            return;
        }
        if !ready && swap.cap.is_some_and(|at| at <= now) {
            observation::release(&mut swap, now, "expired", true);
            swap.fallbacks += 1;
            swap.cap = None;
            swap.holding = false;
        }
        let due = swap
            .cap
            .filter(|at| *at > now)
            .or(minimum.filter(|at| *at > now));
        drop(swap);
        self.arm(due, cx);
    }
}
