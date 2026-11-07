//! Office documents as a GPUI view: documents and presentations open in the PDF surface from the officepdf cache,
//! spreadsheets in the CSV table one sheet at a time, the embedded thumbnail stands in while the file converts, and
//! a failure, a password or a box with no converter projects one approved sentence for the host's failure recipe.
use super::csv::{CodeHost, CsvView, TableFace, TablePalette};
use super::{Bar, Metrics, Palette, PdfView};
use flea::i18n::I18n;
use flea::ui_logic_pv2::office::{self, Applied, Group, Office, State};
use flea::ui_logic_pv2::{office_sniff, pdf, viewer};
use gpui::{
    canvas, div, fill, point, prelude::*, px, size, App, Bounds, Context, Corners, Entity, Pixels,
    Render, RenderImage, Task, Window,
};
use std::{os::unix::fs::FileExt, path::Path, sync::Arc};

/// The host's backend: sends `office::request_line` under an id it returns; replies come back through `reply`.
pub trait OfficeHost {
    fn office(&self, path: &str, sheets: bool) -> u64;
}

/// Quick Look's bar for an office document as the host's chrome bar draws it, every string through the catalog.
#[derive(Clone, Debug, PartialEq)]
pub struct OfficeBar {
    /// The row's own kind mark: the lined page, the table or the screen.
    pub glyph: &'static str,
    /// A sheet's count beside the name, "13 rows".
    pub count: Option<String>,
    /// The sheet names as bare words, the current one marked.
    pub sheets: Vec<(String, bool)>,
    /// The PDF viewer's strip for pages and for every state with no pages; None for a sheet's table.
    pub strip: Option<Bar>,
}

/// The failure recipe's parts: its mark, its tone, its one sentence and the Open With button's label.
#[derive(Clone, Debug, PartialEq)]
pub struct Failure {
    pub glyph: &'static str,
    pub error_tone: bool,
    pub sentence: String,
    pub open_with: Option<String>,
}

pub struct OfficeView {
    pub office: Office,
    pub pdf: Entity<PdfView>,
    pub table: Entity<CsvView>,
    host: Option<Box<dyn OfficeHost>>,
    /// The document's embedded thumbnail, from the host's thumbnail service, shown until the pages land.
    pub thumb: Option<Arc<RenderImage>>,
    pub palette: Palette,
    pub metrics: Metrics,
    /// The document's own Title and Author, from the host's metadata reply.
    pub meta: (Option<String>, Option<String>),
    sniff: Option<Task<()>>,
    check: Option<Task<()>>,
    /// Bytes the last header sniff read.
    pub sniffed_bytes: usize,
}

impl OfficeView {
    /// Quick Look's card when `look`, the preview column's media frame otherwise; both draw one fitted page.
    pub fn new(
        look: bool,
        palette: Palette,
        metrics: Metrics,
        face: TableFace,
        table_palette: TablePalette,
        cx: &mut Context<Self>,
    ) -> Self {
        let pdf = cx.new(|cx| PdfView::new(false, palette, metrics, cx));
        let table = cx.new(|_| CsvView::new(look, face, table_palette));
        cx.observe(&pdf, |view, _, cx| view.drawable(cx)).detach();
        cx.observe(&table, |view, _, cx| view.drawable(cx)).detach();
        OfficeView {
            office: Office::default(),
            pdf,
            table,
            host: None,
            thumb: None,
            palette,
            metrics,
            meta: (None, None),
            sniff: None,
            check: None,
            sniffed_bytes: 0,
        }
    }

    pub fn set_host(
        &mut self,
        host: Box<dyn OfficeHost>,
        code: Box<dyn CodeHost>,
        cx: &mut Context<Self>,
    ) {
        self.host = Some(host);
        self.table.update(cx, |table, _| table.set_host(code));
    }

    // A ready subject whose PDF did not open or whose sheet the engine refused ends as a failure the swap shows.
    fn drawable(&mut self, cx: &mut Context<Self>) {
        if self.office.state != State::Ready {
            return;
        }
        let lost = if self.office.wants_sheets() {
            self.table
                .read(cx)
                .feed
                .answer
                .refused()
                .then_some(office::SHEET_REFUSED)
        } else {
            self.pdf
                .read(cx)
                .surface
                .failed()
                .then_some(office::PDF_UNOPENED)
        };
        if let Some(diagnostic) = lost {
            self.office.undrawable(diagnostic);
            cx.notify();
        }
    }

    /// The subject left: a conversion still out becomes cancelled, its late line is dropped, and no toast is raised.
    pub fn leave(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.office.leave();
        self.sniff = None;
        self.check = None;
        self.pdf
            .update(cx, |pdf, cx| pdf.set_active(false, window, cx));
        cx.notify();
    }

    /// The embedded thumbnail for the subject on screen.
    pub fn set_thumbnail(&mut self, path: &str, image: Arc<RenderImage>, cx: &mut Context<Self>) {
        if path == self.office.path {
            self.thumb = Some(image);
            cx.notify();
        }
    }

    pub fn set_meta(
        &mut self,
        path: &str,
        title: Option<String>,
        author: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if path == self.office.path {
            self.meta = (title, author);
            cx.notify();
        }
    }

    /// One officepdf or officesheets line from the host; only the request in flight takes it.
    pub fn reply(&mut self, line: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(line) = office::parse(line) else {
            return false;
        };
        match self.office.reply(&line) {
            Applied::Stale => return false,
            Applied::Converting | Applied::Settled => {}
            Applied::Pdf(path) => self.pdf.update(cx, |pdf, cx| {
                pdf.set_active(true, window, cx);
                pdf.set_path(&path, window, cx);
                pdf.first = self.office.first;
            }),
            Applied::Sheet(path) => self.table.update(cx, |table, cx| table.set_path(&path, cx)),
            Applied::Sniff => self.sniff(cx),
            Applied::Ask => {
                let (path, sheets) = (self.office.path.clone(), self.office.wants_sheets());
                let id = self
                    .host
                    .as_ref()
                    .map_or(0, |host| host.office(&path, sheets));
                self.office.asked(id);
            }
        }
        cx.notify();
        true
    }

    // A failed conversion reads the file's own header off this thread, so a password is told apart from a failure.
    fn sniff(&mut self, cx: &mut Context<Self>) {
        let path = self.office.path.clone();
        self.sniff = Some(cx.spawn(async move |this, cx| {
            let read = path.clone();
            let (locked, bytes) = cx
                .background_executor()
                .spawn(async move { sniff_file(&read) })
                .await;
            let _ = this.update(cx, |view, cx| {
                if view.office.path == path {
                    view.office.sniffed(locked);
                    view.sniffed_bytes = bytes;
                }
                view.sniff = None;
                cx.notify();
            });
        }));
    }

    /// One key-table action: `[` and `]` turn sheets, j and k scroll a sheet, pages take the PDF viewer's actions.
    pub fn action(&mut self, name: &str, cx: &mut Context<Self>) -> bool {
        if let Some(direction) = viewer::page_direction(name) {
            return self.page(direction, cx);
        }
        if self.office.state != State::Ready {
            return false;
        }
        if self.office.wants_sheets() {
            let path = match name {
                "previousSheet" => self.office.turn_sheet(-1),
                "nextSheet" => self.office.turn_sheet(1),
                "cursorDown" | "cursorUp" => {
                    let rows = if name == "cursorDown" { 1 } else { -1 };
                    self.table
                        .update(cx, |table, cx| table.scroll_rows(rows, cx));
                    return true;
                }
                _ => return false,
            };
            if let Some(path) = path {
                self.table.update(cx, |table, cx| table.set_path(&path, cx));
                cx.notify();
            }
            return true;
        }
        let Some(action) = viewer::Action::from_name(name) else {
            return false;
        };
        self.pdf.update(cx, |pdf, cx| pdf.action(action, cx));
        true
    }

    /// The bar Quick Look's chrome draws for this document.
    pub fn bar(&self, i18n: &I18n, cx: &App) -> OfficeBar {
        let glyph = self.office.group.map_or("file-text", Group::glyph);
        let ready = self.office.state == State::Ready;
        if ready && self.office.wants_sheets() {
            return OfficeBar {
                glyph,
                count: self.table.read(cx).count_text(i18n),
                sheets: self.office.sheet_words(),
                strip: None,
            };
        }
        let strip = if ready {
            self.pdf.read(cx).bar(i18n)
        } else {
            Some(idle_strip(i18n))
        };
        OfficeBar {
            glyph,
            count: None,
            sheets: Vec::new(),
            strip,
        }
    }

    /// The failure recipe the host draws in place of pages, None while pages, a table or the thumbnail stand.
    pub fn failure(&self, i18n: &I18n) -> Option<Failure> {
        let p = self.office.projection()?;
        Some(Failure {
            glyph: p.glyph,
            error_tone: p.error_tone,
            sentence: i18n.tr(p.key),
            open_with: p.open_with.then(|| i18n.tr("document.office_open_with")),
        })
    }

    /// The facts the column adds under Kind: Pages, Sheets or Slides, then the Title and Author the document sets.
    pub fn facts(&self, i18n: &I18n, cx: &App) -> Vec<(String, String)> {
        let pages = self.pdf.read(cx).surface.page_count();
        let mut out = Vec::new();
        if let Some((key, count)) = self.office.count(pages) {
            out.push((i18n.tr(key), viewer::count_text(count, self.office.first)));
        }
        let (title, author) = &self.meta;
        for (key, value) in [
            ("document.office_title", title),
            ("document.office_author", author),
        ] {
            if let Some(value) = value.as_ref().filter(|v| !v.is_empty()) {
                out.push((i18n.tr(key), value.clone()));
            }
        }
        out
    }

    /// The swap may show this subject: first page or first screen drawn, thumbnail while converting, or a failure.
    pub fn shown(&self, cx: &App) -> bool {
        match self.office.state {
            State::Ready if self.office.wants_sheets() => self.table.read(cx).first_screen(),
            State::Ready => self.pdf.read(cx).surface.shown_page().is_some(),
            _ => true,
        }
    }

    /// Where the thumbnail draws inside the frame: fitted inside the page margin, centred, where the first page lands.
    pub fn thumb_rect(&self, frame: (f64, f64)) -> Option<(f64, f64, f64, f64)> {
        let image = self.thumb.as_ref()?;
        let size = image.size(0);
        let aspect = pdf::aspect(f64::from(size.width.0), f64::from(size.height.0));
        let margin = f64::from(self.metrics.margin);
        let inner = (
            (frame.0 - 2. * margin).max(0.),
            (frame.1 - 2. * margin).max(0.),
        );
        let (w, h) = pdf::fit(inner.0, inner.1, aspect);
        Some((
            margin + (inner.0 - w) / 2.,
            margin + (inner.1 - h) / 2.,
            w,
            h,
        ))
    }

    fn paint_thumb(&self, bounds: Bounds<Pixels>, window: &mut Window, cx: &App) {
        if self.office.state == State::Ready
            && (self.office.wants_sheets() || self.pdf.read(cx).surface.shown_page().is_some())
        {
            return;
        }
        let frame = (f64::from(bounds.size.width), f64::from(bounds.size.height));
        let (Some(image), Some((x, y, w, h))) = (&self.thumb, self.thumb_rect(frame)) else {
            return;
        };
        let rect = Bounds::new(
            bounds.origin + point(px(x as f32), px(y as f32)),
            size(px(w as f32), px(h as f32)),
        );
        let _ = window.paint_image(rect, rect, Corners::default(), Arc::clone(image), 0, false);
        if let Some(veil) = self.palette.veil() {
            window.paint_quad(fill(rect, veil));
        }
    }
}

// The strip a document with no pages draws: no pager, the zoom marks dimmed, expand and close live.
fn idle_strip(i18n: &I18n) -> Bar {
    let items = viewer::Viewer::default()
        .strip(0, 0)
        .into_iter()
        .map(|item| (item, item.control.glyph(), i18n.tr(item.control.name_key())))
        .collect();
    Bar {
        counter: None,
        items,
    }
}

// The sniff over a regular file only: its head, its last 64 KiB and one local header, read at their offsets.
fn sniff_file(path: &str) -> (bool, usize) {
    let Some(file) = flea::backend::regfile::open_regular(Path::new(path)) else {
        return (false, 0);
    };
    let size = file.metadata().map_or(0, |m| m.len());
    office_sniff::sniff(path, size, &mut |at, len| {
        let mut buf = vec![0; len];
        let read = file.read_at(&mut buf, at).ok()?;
        buf.truncate(read);
        Some(buf)
    })
}

impl Render for OfficeView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let painted = cx.entity();
        let thumb = canvas(
            |_, _, _| (),
            move |bounds, _, window, cx| painted.read(cx).paint_thumb(bounds, window, cx),
        )
        .absolute()
        .size_full();
        let content = match self.office.state {
            State::Ready if self.office.wants_sheets() => {
                Some(self.table.clone().into_any_element())
            }
            State::Ready => Some(self.pdf.clone().into_any_element()),
            _ => None,
        };
        div()
            .size_full()
            .relative()
            .child(thumb)
            .children(content.map(|c| div().absolute().size_full().child(c)))
    }
}

mod container;

#[cfg(test)]
mod m9_g2rows_14;
