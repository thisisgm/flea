//! Mounted PDF fixture using the renderer's ordinary replies and the production column.
use super::previewtypes_tests::{file, local, meta, open, pump};
use crate::{
    g2::pv2::{render::Reply, surface::Surface, PdfView},
    state::Shell,
    window::selection_preview::Mounted,
};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::{sync::Arc, time::Duration};

pub(super) fn pdf(
    cx: &mut TestAppContext,
) -> (Entity<Shell>, &mut VisualTestContext, Entity<PdfView>) {
    let (shell, cx) = open(cx, "/files");
    super::test_shell::land(
        &shell,
        cx,
        "/files",
        vec![file(
            "pages.pdf",
            "application-pdf",
            "PDF document",
            0o100644,
        )],
        1,
    );
    let lines = local(&shell, cx, "/files");
    meta(&shell, cx, &lines, r#","pages":3"#);
    let pdf = shell.read_with(cx, |shell, cx| {
        let preview = shell.selection_preview(0).unwrap();
        let preview = preview.read(cx);
        let frame = preview.pending.as_ref().or(preview.drawn.as_ref()).unwrap();
        match &frame.mounted {
            Mounted::Pdf(pdf) => pdf.clone(),
            _ => panic!("PDF mounted"),
        }
    });
    cx.update(|_, cx| {
        pdf.update(cx, |pdf, cx| {
            seed(pdf, true);
            cx.notify();
        })
    });
    pump(cx);
    (shell, cx, pdf)
}

pub(super) fn seed(pdf: &mut PdfView, continuous: bool) {
    let mut surface = Surface::new(continuous, 66, 100);
    surface.set_active(true, 0);
    surface.set_path("/files/pages.pdf", 0);
    surface.frame();
    surface.opening(1);
    surface.set_frame(pdf.frame.0.max(274.), pdf.frame.1.max(560.), 1., 9.);
    surface.reply(
        Reply::Opened {
            doc: 1,
            sizes: vec![(612., 792.); 3],
            read: 3,
        },
        0,
    );
    for page in 0..3 {
        surface.reply(
            Reply::Rendered {
                doc: 1,
                page,
                width: surface.render_width,
                image: Arc::new(gpui::RenderImage::from_bgra(1, 1, vec![255; 4]).unwrap()),
                took: Duration::ZERO,
            },
            0,
        );
    }
    pdf.surface = surface;
}
