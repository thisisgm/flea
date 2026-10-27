//! Readers project only the retained frame actually mounted by the production Shell.
use super::*;
use crate::ipc::Adapter;
use flea::ipc::wire::Reply;
use gpui::{App, Bounds, Pixels};

pub(crate) struct Reader<'a> {
    pub view: Option<&'a SelectionPreview>,
    pub active: bool,
    pub cx: &'a App,
    pub window: &'a gpui::Window,
}
fn rect(bounds: Option<Bounds<Pixels>>) -> String {
    bounds
        .map(|bounds| {
            format!(
                "{} {} {} {}",
                f32::from(bounds.left()).round(),
                f32::from(bounds.top()).round(),
                f32::from(bounds.size.width).round(),
                f32::from(bounds.size.height).round()
            )
        })
        .unwrap_or_default()
}
fn centre(bounds: Bounds<Pixels>) -> String {
    let centre = bounds.center();
    format!(
        "{} {}",
        f32::from(centre.x).round(),
        f32::from(centre.y).round()
    )
}
impl Reader<'_> {
    pub(crate) fn value(&self, function: &str, arguments: &[String]) -> Option<String> {
        let view = self.view.filter(|v| self.active && v.visible);
        let loading = view.is_some_and(|v| {
            v.pending.is_some() && (!v.port.0.borrow().holding || v.drawn.is_none())
        });
        let frame = view.and_then(|v| (!loading).then_some(v.drawn.as_ref()).flatten());
        let pdf = frame.and_then(|f| match &f.mounted {
            Mounted::Pdf(pdf) => Some(pdf.read(self.cx)),
            _ => None,
        });
        let player = frame.and_then(|f| match &f.mounted {
            Mounted::Media(Some(player)) => Some(player.read(self.cx)),
            _ => None,
        });
        Some(match function {
            "pdfState" => {
                if arguments.first().is_some_and(|overlay| overlay == "true") {
                    return None;
                }
                view.zip(pdf)
                    .map_or("null".into(), |(v, p)| pdf_state(v, p, self.window))
            }
            "columnPdfPages" => pdf.map_or(0, |p| p.surface.page_count()).to_string(),
            "columnPdfPage" => pdf.map_or(-1, |p| p.surface.page as i64).to_string(),
            "columnPdfLoaded" => pdf
                .is_some_and(|p| p.surface.shown_page().is_some())
                .to_string(),
            "columnPlayerLoaded" | "previewMediaLoaded" => player.is_some().to_string(),
            "columnMediaPlaying" => player
                .is_some_and(|p| p.state.playback == flea::ui_logic_pv3::player::Playback::Playing)
                .to_string(),
            // PreviewColumn.qml mediaPosition answers 0 while no player is mounted; -1 is only no column at all.
            "columnMediaPosition" => view
                .map_or(-1, |_| player.map_or(0, |p| p.position))
                .to_string(),
            "columnManualHold" => frame.is_some_and(|f| f.manual).to_string(),
            "columnThumbShown" => frame.is_some_and(|f| f.image.is_some()).to_string(),
            "columnFrameReady" => frame
                .is_some_and(|f| {
                    !f.failed
                        && match f.kind {
                            Kind::Image | Kind::Video => {
                                f.image.is_some() || player.is_some_and(|p| p.frame.is_some())
                            }
                            Kind::Pdf => pdf.is_some_and(|p| p.surface.shown_page().is_some()),
                            _ => !loading,
                        }
                })
                .to_string(),
            "previewColumnState" => {
                if loading {
                    "loading".into()
                } else {
                    frame.map_or(String::new(), |f| {
                        if self.view.is_some_and(|v| v.manual_empty(f)) {
                            return Kind::Facts.name().into();
                        }
                        // PreviewColumn.qml previewState: Error only where Facts.js draws it, never for a failed picture.
                        if self.view.is_some_and(|v| v.state(f, self.cx) == flea::ui_logic_pv1::kinds::State::Error) {
                            "error".into()
                        } else {
                            f.kind.name().into()
                        }
                    })
                }
            }
            "columnFailure" => frame
                .zip(view)
                .and_then(|(f, v)| v.failure(f, self.cx))
                .unwrap_or_default(),
            "columnTextLines" => frame.map_or(String::new(), |f| doc_lines(f, self.cx).join("|")),
            "previewText" => frame.map_or(String::new(), |f| doc_lines(f, self.cx).join("\n")),
            "columnArchiveNames" => frame
                .and_then(|f| f.facts.as_ref())
                .map_or(String::new(), |m| {
                    m.names.iter().map(|e| e.name.as_str()).collect::<Vec<_>>().join("|")
                }),
            "previewSurfaceRect" => rect(view.and_then(|v| *v.rect.borrow())),
            "columnFrameRect" => rect(view.and_then(|v| *v.frame_rect.borrow())),
            "columnPictureRect" | "previewPictureRect" => rect(
                frame
                    .zip(view)
                    .and_then(|(frame, view)| picture_rect(view, frame, self.cx)),
            ),
            "columnLinesRect" => rect(
                frame
                    .filter(|f| matches!(f.mounted, Mounted::Doc(_)))
                    .and(view)
                    .and_then(|v| *v.frame_rect.borrow()),
            ),
            "columnStripCentre" => view
                .and_then(|v| *v.play_rect.borrow())
                .map_or(String::new(), centre),
            // The play mark's own box, painted by the strip, and only while the strip is drawn.
            "columnPlayCentre" => view
                .and_then(|v| *v.play_rect.borrow())
                .and(view.and_then(|v| v.caches.strip.borrow().play))
                .map_or(String::new(), centre),
            // ColumnsArea.qml factsLine: "Kind=MPEG-4 video|Duration=1:12|...".
            "previewFacts" => frame.zip(view).map_or(String::new(), |(f, v)| {
                v.facts(f, self.cx)
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join("|")
            }),
            "previewTable" => frame.zip(view).map_or(String::new(), |(f, v)| {
                v.facts(f, self.cx)
                    .iter()
                    .map(|(k, v)| format!("{k}: {v}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            }),
            // Without a preview class the pane's own fsinfo class answers, so the reader falls through.
            "storageClass" => view.and_then(|v| v.class.as_ref()).map(|(_, class)| class.clone())?,
            "previewSwapState" => view.map_or(String::new(), swap_state),
            _ => return None,
        })
    }
}
impl Adapter for Reader<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
        let value = self.value(function, arguments)?;
        if let Some(view) = self.view {
            view.port
                .0
                .borrow_mut()
                .readers
                .insert(function.into(), value.clone());
        }
        Some(Reply::value(value))
    }
}

/// What a mounted document draws, as plain lines.
fn doc_lines(frame: &Frame, cx: &App) -> Vec<String> {
    match &frame.mounted {
        Mounted::Doc(doc) => doc.lines(cx),
        _ => Vec::new(),
    }
}

fn pdf_state(view: &SelectionPreview, pdf: &PdfView, window: &gpui::Window) -> String {
    use flea::jsondoc::Json;
    let s = &pdf.surface;
    let number = |n: f64| Json::Num(n.to_string());
    let last = s.sizes.len().checked_sub(1);
    let last_rect = last
        .and_then(|page| {
            s.image(page)?;
            let frame = (*view.frame_rect.borrow())?;
            let top = *s.layout.tops.get(page)? - s.view_top;
            let height = *s.layout.heights.get(page)?;
            Some(format!(
                "{} {} {} {}",
                (f64::from(frame.left()) + 1.).round(),
                (f64::from(frame.top()) + 1. + top).round(),
                s.width.round(),
                height.round()
            ))
        })
        .unwrap_or_default();
    let state = Json::Obj(vec![
        ("pages".into(), number(s.page_count() as f64)),
        ("page".into(), number(s.page as f64)),
        ("scrollY".into(), number(s.view_top)),
        ("focused".into(), Json::Bool(view.focus.is_focused(window))),
        ("loading".into(), Json::Bool(s.render_busy())),
        ("frame".into(), Json::Str(rect(*view.frame_rect.borrow()))),
        ("lastPage".into(), Json::Str(last_rect)),
        (
            "landed".into(),
            Json::Arr(
                pdf.drawn_pages()
                    .map(|(page, _, _)| number(page as f64))
                    .collect(),
            ),
        ),
    ]);
    flea::jsondoc::render(&state)
}

fn swap_state(view: &SelectionPreview) -> String {
    use flea::jsondoc::Json;
    let swap = view.port.0.borrow();
    let frames = view.frames.borrow();
    let number = |n: u64| Json::Num(n.to_string());
    let observed = |n| {
        if frames.available {
            number(n)
        } else {
            Json::Null
        }
    };
    let column = Json::Obj(vec![
        ("holding".into(), Json::Bool(swap.holding)),
        ("holds".into(), number(swap.holds)),
        ("fallbacks".into(), number(swap.fallbacks)),
        ("heldFrames".into(), observed(frames.held)),
        ("midFrames".into(), observed(frames.blank)),
        ("loadingFrames".into(), observed(frames.loading)),
        ("blankFrames".into(), observed(frames.blank)),
        ("blinkBacks".into(), observed(frames.blink_backs)),
        ("presentedFrames".into(), observed(frames.presented)),
        ("presentationAvailable".into(), Json::Bool(frames.available)),
        ("lostFeedback".into(), number(frames.lost)),
    ]);
    flea::jsondoc::render(&Json::Obj(vec![
        ("column".into(), column),
        ("look".into(), Json::Null),
        ("lookVisible".into(), Json::Bool(false)),
    ]))
}

fn picture_rect(view: &SelectionPreview, frame: &Frame, cx: &App) -> Option<Bounds<Pixels>> {
    let bounds = (*view.frame_rect.borrow())?;
    let player = match &frame.mounted {
        Mounted::Media(Some(player)) => player.read(cx).frame.as_ref(),
        _ => None,
    };
    let image = player.or(frame.image.as_ref())?;
    let size = image.size(0);
    let [x, y, width, height] = flea::ui_logic_pv3::picture::fit(
        size.width.0 as f32,
        size.height.0 as f32,
        (f32::from(bounds.size.width) - 2.).max(0.),
        (f32::from(bounds.size.height) - 2.).max(0.),
    );
    Some(Bounds::new(
        bounds.origin + gpui::point(gpui::px(1. + x), gpui::px(1. + y)),
        gpui::size(gpui::px(width), gpui::px(height)),
    ))
}
