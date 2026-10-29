//! Every document reader receives the paging matrix through the production root key listener.
use super::{
    document_keys_fixture as fixture,
    previewtypes_tests::{file, local, meta, open, pump},
};
use crate::{state::Shell, window::u07_tests::read};
use flea::jsondoc::Json;
use gpui::{Entity, TestAppContext, VisualTestContext};

const PAIRS: [(&str, &str); 3] = [
    ("shift-f->F", "shift-b->B"),
    ("ctrl-d", "ctrl-u"),
    ("pagedown", "pageup"),
];

fn preset(shell: &Entity<Shell>, preset: &str, cx: &mut VisualTestContext) {
    shell.update(cx, |shell, _| {
        let Json::Obj(fields) = &mut shell.launch.settings.data else {
            panic!("settings object")
        };
        fields.retain(|(key, _)| key != "keys");
        fields.push(("keys".into(), Json::Str(preset.into())));
    });
}

#[track_caller]
fn exercise(shell: &Entity<Shell>, view: &crate::g2::pv1::doc::View, cx: &mut VisualTestContext) {
    for preset in flea::keys::PRESETS {
        self::preset(shell, preset, cx);
        let expected = fixture::step(view, cx);
        assert!(expected > 0., "{preset} laid-out document viewport");
        for (forward, back) in PAIRS {
            assert_eq!(fixture::position(view, cx), 0.);
            cx.simulate_keystrokes(forward);
            pump(cx);
            assert!(
                (fixture::position(view, cx) - expected).abs() < 0.01,
                "{preset} {forward} pages by {expected}, got {}",
                fixture::position(view, cx)
            );
            assert_eq!(
                read(shell, cx, "cursor", &[]),
                "0",
                "document paging keeps the listing cursor"
            );
            cx.simulate_keystrokes(back);
            pump(cx);
            assert_eq!(
                fixture::position(view, cx),
                0.,
                "{preset} {back} reverses paging"
            );
            cx.simulate_keystrokes(back);
            pump(cx);
            assert_eq!(
                fixture::position(view, cx),
                0.,
                "{preset} {back} clamps at the top"
            );
        }
    }
}

// A standing filter sends the listing's Page Down and Ctrl+D through dispatch, past the shown document's family.
fn filtered_listing_pages(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    preset(shell, "default", cx);
    shell.update(cx, |shell, cx| {
        shell.filter_update(
            0,
            |filter, pane| {
                filter.close(pane);
                filter.start();
                None
            },
            cx,
        );
        shell.filter_update(0, |filter, pane| filter.typed("e", pane), cx);
        let host = shell.panes[0].as_ref().unwrap();
        let id = host.filter.owner.asked().expect("the key asked").id;
        let listing = host.pane.held.listing;
        let line = format!(
            r#"{{"t":"filtered","id":{id},"query":"e","n":2,"rows":[0,1],"ms":0.010,"listing":{listing}}}"#
        );
        assert!(shell.filter_line(0, &line, cx));
    });
    pump(cx);
    for (down, up) in [("pagedown", "pageup"), ("ctrl-d", "ctrl-u")] {
        cx.simulate_keystrokes(down);
        pump(cx);
        assert_eq!(
            read(shell, cx, "cursor", &[]),
            "1",
            "filtered listing {down} moves the cursor"
        );
        cx.simulate_keystrokes(up);
        pump(cx);
        assert_eq!(
            read(shell, cx, "cursor", &[]),
            "0",
            "filtered listing {up} moves the cursor"
        );
    }
}

// The column frame height a host passes while a Source reader has no frame of its own.
const HOST: f32 = 600.;

// `laid` is false where this harness never lays the reader out inside the card; the fixture gives it layout's geometry.
fn document(cx: &mut TestAppContext, name: &str, icon: &str, source: bool, laid: bool) {
    crate::g2::pv2::render::without_threads(|| {
        cx.executor().allow_parking();
        let root = fixture::Root::new();
        let dir = root.0.to_string_lossy().into_owned();
        let text = (0..400)
            .map(|n| format!("line {n},value {n}\n\n"))
            .collect::<String>();
        std::fs::write(root.0.join(name), &text).unwrap();
        let (shell, cx) = open(cx, &dir);
        super::test_shell::land(
            &shell,
            cx,
            &dir,
            vec![
                file(name, icon, "Document", 0o100644),
                file("sentinel.txt", "text-plain", "Text", 0o100644),
            ],
            1,
        );
        let lines = local(&shell, cx, &dir);
        meta(&shell, cx, &lines, r#","lines":800"#);
        let column = fixture::view(&shell, cx, false);
        fixture::seed(&column, &text, cx);
        fixture::ready(&column, cx);
        if source {
            fixture::source(&column, &text, cx);
        }
        let expected = fixture::step(&column, cx);
        cx.simulate_keystrokes("shift-f->F");
        pump(cx);
        assert!(
            (fixture::position(&column, cx) - expected).abs() < 0.01,
            "listing F pages the column"
        );
        cx.simulate_keystrokes("shift-b->B ctrl-tab");
        pump(cx);
        exercise(&shell, &column, cx);
        if source {
            let (moved, expected) = fixture::unlaid_page(&column, HOST, cx);
            assert!(
                (moved - expected).abs() < 0.01,
                "Source before layout pages by the host frame, {expected}, got {moved}"
            );
        }
        cx.simulate_keystrokes("escape space");
        pump(cx);
        let look = fixture::view(&shell, cx, true);
        fixture::seed(&look, &text, cx);
        if laid {
            fixture::ready(&look, cx);
        }
        if source {
            fixture::source(&look, &text, cx);
        }
        if !laid {
            fixture::lay(&look, cx);
        }
        exercise(&shell, &look, cx);
        assert_eq!(
            read(&shell, cx, "previewOpen", &[]),
            "true",
            "paging keeps the card"
        );
        cx.simulate_keystrokes("escape");
        pump(cx);
        assert_eq!(read(&shell, cx, "previewOpen", &[]), "false");
        filtered_listing_pages(&shell, cx);
        shell.update(cx, |shell, _| shell.stop_for_app_quit());
    });
}

macro_rules! document {
    ($test:ident, $name:literal, $icon:literal, $source:literal) => {
        document!($test, $name, $icon, $source, true);
    };
    ($test:ident, $name:literal, $icon:literal, $source:literal, $laid:literal) => {
        #[gpui::test]
        fn $test(cx: &mut TestAppContext) {
            document(cx, $name, $icon, $source, $laid);
        }
    };
}

document!(bfx_keys_text, "notes.txt", "text-plain", false);
document!(bfx_keys_code, "main.rs", "text-x-rust", false);
document!(
    bfx_keys_markdown_rendered,
    "notes.md",
    "text-markdown",
    false
);
document!(
    bfx_keys_markdown_source,
    "notes.md",
    "text-markdown",
    true,
    false
);
document!(
    bfx_keys_html_rendered,
    "page.html",
    "text-html",
    false,
    false
);
document!(bfx_keys_html_source, "page.html", "text-html", true);
document!(bfx_keys_csv, "data.csv", "text-csv", false, false);
document!(bfx_keys_pdf, "pages.pdf", "application-pdf", false);
document!(
    bfx_keys_office_document,
    "report.docx",
    "x-office-document",
    false
);
document!(
    bfx_keys_office_slides,
    "talk.odp",
    "x-office-presentation",
    false
);
document!(
    bfx_keys_office_sheet,
    "budget.xlsx",
    "x-office-spreadsheet",
    false,
    false
);
