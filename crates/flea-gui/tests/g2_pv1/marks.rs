//! The bar's mark: models wear the cube, code its enabled language logo, and other files their kind mark.
use flea::ui_logic_preview_core::{kinds::State, rect::Shape};
use flea_gui::g2::quick_look_overlay::{Body, Subject};
use flea_gui::g2::{
    model_preview::{
        seam::{NoReader, Wire},
        view::Model3dView,
    },
    quick_look_overlay::doc::{Doc, View},
};
use gpui::{AppContext, TestAppContext};
use std::{rc::Rc, sync::Arc};

fn file(name: &str, icon: &str) -> Subject {
    Subject {
        name: name.into(),
        icon: icon.into(),
        mode: 0o100644,
        state: State::Code,
        shape: Shape::unknown(State::Code),
        body: Body::Text {
            lines: Vec::new(),
            count: String::new(),
        },
    }
}

/// The key of the mark the bar draws for a subject under the card's live langMarks.
fn key(subject: &Subject, languages: bool) -> String {
    subject.mark_with_languages(languages).svg().0.to_string()
}

#[test]
fn the_bar_wears_each_code_files_language_logo() {
    for (name, icon, logo) in [
        ("main.rs", "text-x-rust", "rust"),
        ("app.py", "text-x-python", "python"),
        ("app.js", "text-x-javascript", "javascript"),
        ("run.sh", "text-x-script", "gnubash"),
    ] {
        assert_eq!(
            key(&file(name, icon), true),
            format!("flea-lang:{logo}"),
            "{name} wears its language logo"
        );
    }
}

#[test]
fn a_file_without_a_language_keeps_its_kind_mark() {
    assert_eq!(
        key(&file("notes.txt", "text-plain"), true),
        "flea-mark:file-text"
    );
}

#[test]
fn language_marks_off_keeps_the_kind_mark() {
    assert_eq!(
        key(&file("main.rs", "text-x-rust"), false),
        "flea-mark:file"
    );
}

struct NoWire;

impl Wire for NoWire {
    fn send(&self, _: String) -> bool {
        false
    }
}

fn model_bar(cx: &mut TestAppContext, name: &str, icon: &str) {
    let view = cx.new(|cx| {
        Model3dView::new(
            flea::ui_logic_model_preview::control::Surface::QuickLook,
            Rc::new(NoWire),
            Arc::new(NoReader),
            Arc::new(crate::test_catalogs::english()),
            cx,
        )
    });
    let mut subject = file(name, icon);
    subject.state = State::Image;
    subject.shape = Shape::unknown(State::Image);
    subject.body = Body::Doc(Doc {
        view: View::Model(view),
        path: format!("/files/{name}"),
        page: 0,
    });
    for languages in [true, false] {
        assert_eq!(
            key(&subject, languages),
            "pv1-mark:cube",
            "{name}, language marks {languages}"
        );
        let svg = std::str::from_utf8(subject.mark_with_languages(languages).svg().1).unwrap();
        assert!(svg.contains(r#"d="M12 2l9 5v10l-9 5-9-5V7z M3 7l9 5 9-5 M12 12v10""#));
    }
}

#[gpui::test]
fn the_stl_model_bar_wears_the_cube_mark(cx: &mut TestAppContext) {
    model_bar(cx, "model-stl.stl", "image-x-generic");
}

#[gpui::test]
fn the_obj_model_bar_wears_the_cube_mark(cx: &mut TestAppContext) {
    model_bar(cx, "model-obj.obj", "application-x-generic");
}

#[gpui::test]
fn the_3mf_model_bar_wears_the_cube_mark(cx: &mut TestAppContext) {
    model_bar(cx, "model-3mf.3mf", "image-x-generic");
}

#[gpui::test]
fn every_other_model_view_bar_wears_the_cube_mark(cx: &mut TestAppContext) {
    for name in [
        "model.ply",
        "model.amf",
        "model.gcode",
        "model.gco",
        "model.bgcode",
        "MODEL.STL",
    ] {
        model_bar(cx, name, "text-plain");
    }
}

#[test]
fn a_model_filename_without_a_model_view_keeps_its_kind_mark() {
    assert_eq!(
        key(&file("model.stl", "image-x-generic"), true),
        "flea-mark:image"
    );
    assert_eq!(
        key(&file("model.obj", "application-x-generic"), true),
        "flea-mark:file"
    );
    assert_eq!(
        key(&file("model.3mf", "image-x-generic"), true),
        "flea-mark:image"
    );
}

#[gpui::test]
fn a_model_name_in_another_doc_view_keeps_its_kind_mark(cx: &mut TestAppContext) {
    let view = cx.new(|_| {
        let theme = flea_gui::g2::doc_preview::font::Theme {
            foreground: "#c0caf5".into(),
            surface: "#1a1b26".into(),
            muted: flea_gui::palette::Theme::default().muted,
            size_ratio: 1.,
        };
        let metrics = flea_gui::g2::doc_preview::font::Metrics {
            font: gpui::font("sans-serif"),
            caption: gpui::px(12.),
        };
        flea_gui::g2::doc_preview::font::FontView::new(theme, metrics, ("font".into(), &[]))
    });
    let mut subject = file("model.stl", "image-x-generic");
    subject.body = Body::Doc(Doc {
        view: View::Font(view),
        path: "/files/model.stl".into(),
        page: 0,
    });
    assert_eq!(
        key(&subject, true),
        "flea-mark:image",
        "only the model view wears the cube"
    );
}
