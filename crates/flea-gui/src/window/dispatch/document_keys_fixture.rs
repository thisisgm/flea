//! Bounded document data for the root keyboard regression, with no converter or browser process.
use super::previewtypes_tests::pump;
use crate::{
    g2::{
        pv1::doc::View,
        pv2::{code::CodeView, csv::CsvView},
    },
    state::Shell,
    window::selection_preview::{docs::Doc, Mounted},
};
use flea::ui_logic_pv2::{code_view::page_step, html, office};
use gpui::{App, Entity, RenderImage, VisualTestContext};
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

pub(super) struct Root(pub PathBuf);
impl Root {
    pub fn new() -> Self {
        let output = std::process::Command::new("mktemp")
            .args(["-d", "/tmp/flea-bfx-keys.XXXXXXXX"])
            .output()
            .unwrap();
        assert!(output.status.success());
        let path = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
        std::fs::write(path.join(".flea-test-sandbox"), b"BFX-keys\n").unwrap();
        Self(path)
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        assert!(self.0.starts_with("/tmp"));
        assert!(self
            .0
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("flea-bfx-keys."));
        assert_eq!(
            std::fs::read(self.0.join(".flea-test-sandbox")).unwrap(),
            b"BFX-keys\n"
        );
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

pub(super) fn view(shell: &Entity<Shell>, cx: &mut VisualTestContext, look: bool) -> View {
    shell.read_with(cx, |shell, cx| {
        if look {
            return shell
                .quick_look
                .as_ref()
                .unwrap()
                .view
                .read(cx)
                .subject()
                .unwrap()
                .body
                .doc()
                .unwrap()
                .view
                .clone();
        }
        let preview = shell.selection_preview(0).unwrap();
        let preview = preview.read(cx);
        match &preview
            .pending
            .as_ref()
            .or(preview.drawn.as_ref())
            .unwrap()
            .mounted
        {
            Mounted::Pdf(v) => View::Pdf(v.clone()),
            Mounted::Doc(Doc::Code(v)) => View::Code(v.clone()),
            Mounted::Doc(Doc::Markdown(v)) => View::Markdown(v.clone()),
            Mounted::Doc(Doc::Html(v)) => View::Html(v.clone()),
            Mounted::Doc(Doc::Office(v)) => View::Office(v.clone()),
            Mounted::Doc(Doc::Table(v)) => View::Table(v.clone()),
            _ => panic!("document mounted"),
        }
    })
}

fn seed_code(view: &Entity<CodeView>, text: &str, cx: &mut App) {
    view.update(cx, |view, cx| {
        view.lines.engine = false;
        view.set_subject(
            "fixture.txt",
            text.len() as u64,
            true,
            Some(text.into()),
            cx,
        );
        view.lines.answer_plain();
        view.flush(cx);
    });
}

pub(super) fn seed(view: &View, text: &str, cx: &mut VisualTestContext) {
    cx.update(|_, cx| match view {
        View::Code(view) => seed_code(view, text, cx),
        View::Html(view) => view.update(cx, |view, cx| {
            view.doc.pending = false;
            view.doc.picture = html::Picture::Ready;
            view.doc.has_drawn = true;
            view.doc.total = 400;
            view.image = Some((
                1,
                Arc::new(RenderImage::from_bgra(1, 1, vec![255; 4]).unwrap()),
                (100, 6000),
            ));
            cx.notify();
        }),
        View::Table(view) => view.update(cx, |view, cx| view.set_text("fixture.csv", text, cx)),
        View::Office(view) => view.update(cx, |view, cx| {
            view.office.id = 1;
            view.office.state = office::State::Ready;
            if view.office.wants_sheets() {
                view.office.sheet_names = vec!["Monthly".into(), "Savings".into()];
                view.office.sheet_paths = vec!["monthly.csv".into(), "savings.csv".into()];
                view.table
                    .update(cx, |table, cx| table.set_text("fixture.csv", text, cx));
            } else {
                view.pdf.update(cx, |pdf, cx| {
                    super::pb_preview_fixture::seed(pdf, false);
                    cx.notify();
                });
            }
            cx.notify();
        }),
        View::Pdf(view) => view.update(cx, |view, cx| {
            let continuous = view.surface.continuous;
            super::pb_preview_fixture::seed(view, continuous);
            cx.notify();
        }),
        View::Markdown(_) => {}
        _ => panic!("document fixture"),
    });
    pump(cx);
}

pub(super) fn source(view: &View, text: &str, cx: &mut VisualTestContext) {
    cx.update(|_, cx| match view {
        View::Markdown(view) => view.update(cx, |view, cx| {
            view.source.set_text(
                Arc::from(text),
                Arc::new(crate::g2::pv2::markdown::line_starts(text)),
            );
            view.source.answer = crate::g2::pv2::markdown::Answer::Plain;
            view.source.total = text.lines().count();
            view.set_view(crate::g2::pv2::markdown::View::Source, cx);
        }),
        View::Html(view) => view.update(cx, |view, cx| {
            view.set_view(html::View::Source, cx);
            seed_code(view.source.as_ref().unwrap(), text, cx);
        }),
        _ => panic!("reader with Source"),
    });
    pump(cx);
}

#[track_caller]
pub(super) fn ready(view: &View, cx: &mut VisualTestContext) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let ready = cx.read(|cx| match view {
            View::Code(v) => v.read(cx).first_screen(),
            View::Markdown(v) => {
                v.read(cx).doc.content_ready() && v.read(cx).content_height() > 2000.
            }
            View::Html(v) => v.read(cx).first_screen(cx),
            View::Office(v) => v.read(cx).shown(cx),
            View::Table(v) => v.read(cx).first_screen(),
            View::Pdf(v) => v.read(cx).surface.shown_page().is_some(),
            _ => false,
        });
        if ready {
            break;
        }
        assert!(Instant::now() < deadline, "document fixture becomes ready");
        pump(cx);
        std::thread::sleep(Duration::from_millis(5));
    }
    pump(cx);
}

// The card size and Source line height layout would give a reader inside Quick Look.
const CARD: (f32, f32) = (800., 600.);
const SOURCE_LINE: f32 = 20.;

// Quick Look's card never lays these readers out in this harness, so they take the geometry layout gives.
pub(super) fn lay(view: &View, cx: &mut VisualTestContext) {
    cx.update(|_, cx| match view {
        View::Table(v) => lay_table(v, cx),
        View::Office(v) => {
            let table = v.read(cx).table.clone();
            lay_table(&table, cx);
        }
        View::Html(v) => v.update(cx, |v, _| v.frame = CARD),
        View::Markdown(v) => v.update(cx, |v, _| {
            v.source.frame.height = CARD.1;
            v.source.frame.line = SOURCE_LINE;
        }),
        _ => panic!("reader the card leaves unlaid"),
    });
    pump(cx);
}

fn lay_table(view: &Entity<CsvView>, cx: &mut App) {
    view.update(cx, |view, _| {
        let table = view.feed.job().run();
        view.feed.apply(table);
        view.frame = (f64::from(CARD.0), f64::from(CARD.1));
    });
}

/// Pages a Source reader forward and back with neither frame laid out; returns (moved, the host's page).
pub(super) fn unlaid_page(view: &View, host: f32, cx: &mut VisualTestContext) -> (f64, f64) {
    cx.update(|_, cx| match view {
        View::Html(v) => v.update(cx, |html, cx| {
            let source = html.source.clone().unwrap();
            let kept = (html.frame, source.read(cx).frame);
            html.frame.1 = 0.;
            source.update(cx, |code, _| code.frame.1 = 0.);
            html.page(1, host, cx);
            let code = source.read(cx);
            let moved = (
                code_position(code),
                f64::from(page_step(host, code.metrics.row)),
            );
            html.page(-1, host, cx);
            html.frame = kept.0;
            source.update(cx, |code, _| code.frame = kept.1);
            moved
        }),
        View::Markdown(v) => v.update(cx, |md, cx| {
            let kept = (md.frame, md.source.frame.height);
            md.frame.1 = 0.;
            md.source.frame.height = 0.;
            md.page(1, host, cx);
            let line = md.source.frame.line;
            let moved = (
                f64::from(md.source.scroll_y()),
                f64::from(page_step(host, line)),
            );
            md.page(-1, host, cx);
            (md.frame, md.source.frame.height) = kept;
            moved
        }),
        _ => panic!("reader with Source"),
    })
}

pub(super) fn position(view: &View, cx: &mut VisualTestContext) -> f64 {
    cx.read(|cx| match view {
        View::Code(v) => code_position(v.read(cx)),
        View::Markdown(v) => {
            let v = v.read(cx);
            if v.shown_view() == crate::g2::pv2::markdown::View::Source {
                f64::from(v.source.scroll_y())
            } else {
                f64::from(v.scroll)
            }
        }
        View::Html(v) => {
            let v = v.read(cx);
            if v.doc.source_view() {
                code_position(v.source.as_ref().unwrap().read(cx))
            } else {
                f64::from(v.scroll_y)
            }
        }
        View::Office(v) => {
            let v = v.read(cx);
            if v.office.wants_sheets() {
                v.table.read(cx).scroll.1
            } else {
                v.pdf.read(cx).surface.page as f64
            }
        }
        View::Table(v) => v.read(cx).scroll.1,
        View::Pdf(v) => {
            let surface = &v.read(cx).surface;
            if surface.continuous {
                surface.view_top
            } else {
                surface.page as f64
            }
        }
        _ => panic!("document position"),
    })
}

fn code_position(view: &CodeView) -> f64 {
    f64::from(view.top as f32 * view.metrics.row + view.offset)
}

pub(super) fn step(view: &View, cx: &mut VisualTestContext) -> f64 {
    cx.read(|cx| match view {
        View::Code(v) => f64::from(page_step(v.read(cx).frame.1, v.read(cx).metrics.row)),
        View::Markdown(v) => {
            let v = v.read(cx);
            if v.shown_view() == crate::g2::pv2::markdown::View::Source {
                f64::from(page_step(v.source.frame.height, v.source.frame.line))
            } else {
                f64::from(v.frame.1)
            }
        }
        View::Html(v) => {
            let v = v.read(cx);
            if v.doc.source_view() {
                let source = v.source.as_ref().unwrap().read(cx);
                f64::from(page_step(source.frame.1, source.metrics.row))
            } else {
                f64::from(v.frame.1)
            }
        }
        View::Office(v) if !v.read(cx).office.wants_sheets() => 1.,
        View::Office(v) => table_step(v.read(cx).table.read(cx)),
        View::Table(v) => table_step(v.read(cx)),
        View::Pdf(v) => {
            let surface = &v.read(cx).surface;
            if surface.continuous {
                (surface.layout.height - surface.view_height).clamp(0., surface.view_height)
            } else {
                1.
            }
        }
        _ => panic!("document step"),
    })
}

fn table_step(view: &crate::g2::pv2::csv::CsvView) -> f64 {
    view.frame.1 - view.feed.geometry.head_height
}
