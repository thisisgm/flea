//! The frozen QML files are the oracle for every filename rule and every original logo contour.
use super::marks::{Language, Mark, EXTS, FILES};
use flea::{
    jsondoc::{self, Json},
    protocol::RowFacts,
};

const RULES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../ui/js/LangMarks.js"
));
const PATHS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../ui/js/LangMarkPaths.js"
));
const PAGE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../ui/LangMark.qml"
));

fn object(source: &str, name: &str) -> Vec<(String, Json)> {
    let start = source
        .split_once(&format!("var {name} = {{"))
        .expect("reference object")
        .1;
    let body = start.split_once("\n}").expect("object end").0;
    jsondoc::parse(&format!("{{{body}\n}}"))
        .unwrap()
        .as_object()
        .unwrap()
        .to_vec()
}

#[test]
fn language_filename_rules_match_every_reference_extension_and_basename() {
    for (name, actual, extension) in [
        ("EXTS", EXTS.as_slice(), true),
        ("FILES", FILES.as_slice(), false),
    ] {
        let expected = object(RULES, name);
        assert_eq!(actual.len(), expected.len(), "{name} gained or lost a rule");
        for (file, value) in expected {
            let slug = value.as_str().unwrap();
            assert!(actual.contains(&(file.as_str(), slug)), "{name}: {file}");
            let file = if extension {
                format!("main{file}")
            } else {
                file
            };
            for spelling in [
                file.clone(),
                file.to_uppercase(),
                format!("parent.rs/nested/{file}"),
            ] {
                assert_eq!(
                    Language::for_file(&spelling).map(Language::slug),
                    Some(slug),
                    "{spelling}"
                );
            }
        }
    }
}

#[test]
fn language_logos_keep_every_original_contour_and_the_cut_page_in_one_ink() {
    let paths = object(PATHS, "PATHS");
    let slugs: Vec<_> = Language::slugs().collect();
    assert_eq!(slugs.len(), 35);
    assert_eq!(paths.len(), slugs.len());
    let page = PAGE
        .split_once("readonly property string page: \"")
        .unwrap()
        .1
        .split_once('"')
        .unwrap()
        .0;
    for (slug, path) in paths {
        assert!(slugs.contains(&slug.as_str()), "missing logo {slug}");
        let mark = Mark::Language(Language::for_slug(&slug).expect("mapped language"));
        let (key, data) = mark.svg();
        assert_eq!(key.as_ref(), format!("flea-lang:{slug}"));
        let svg = std::str::from_utf8(data).unwrap();
        assert!(
            svg.contains(r#"viewBox="0 0 24 24""#),
            "{slug}: shared geometry"
        );
        assert!(
            svg.contains(&format!(r#"<path d="{page}"/>"#)),
            "{slug}: cut page"
        );
        assert!(
            svg.contains(&format!(
                r#"<path fill="currentColor" stroke="none" d="{}"/>"#,
                path.as_str().unwrap()
            )),
            "{slug}: original contour"
        );
        assert!(
            svg.contains(r#"fill="none" stroke="currentColor""#),
            "{slug}: page ink"
        );
        assert!(
            svg.contains(r#"stroke-width="2" stroke-linecap="square" stroke-linejoin="miter""#),
            "{slug}: Omarchy page cut"
        );
        assert_eq!(
            svg.matches("<path ").count(),
            2,
            "{slug}: page and one filled logo only"
        );
    }
}

#[test]
fn language_logos_rasterize_like_the_reference_page_at_row_and_tile_sizes() {
    let renderer = gpui::SvgRenderer::new(std::sync::Arc::new(()));
    let page = PAGE
        .split_once("readonly property string page: \"")
        .unwrap()
        .1
        .split_once('"')
        .unwrap()
        .0;
    for (slug, path) in object(PATHS, "PATHS") {
        let expected = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="{page}" fill="none" stroke="black" stroke-width="2" stroke-linecap="square" stroke-linejoin="miter"/><path d="{}" fill="black"/></svg>"#,
            path.as_str().unwrap()
        );
        let (_, actual) = Mark::Language(Language::for_slug(&slug).unwrap()).svg();
        let actual = renderer.parse_svg(actual).expect("production SVG parses");
        let expected = renderer
            .parse_svg(expected.as_bytes())
            .expect("reference SVG parses");
        for side in [20, 48, 96] {
            let size = gpui::size(gpui::DevicePixels(side), gpui::DevicePixels(side));
            let actual = renderer
                .render_parsed(&actual, gpui::SvgSize::ExactSize(size))
                .unwrap();
            let expected = renderer
                .render_parsed(&expected, gpui::SvgSize::ExactSize(size))
                .unwrap();
            assert_eq!(actual.size(0), size, "{slug}: geometry at {side}");
            assert_eq!(
                actual.as_bytes(0),
                expected.as_bytes(0),
                "{slug}: contour at {side}"
            );
            assert!(
                actual
                    .as_bytes(0)
                    .unwrap()
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|pixel| pixel[3] > 0),
                "{slug}: blank mask"
            );
        }
    }
}

#[test]
fn language_marks_off_directories_links_and_unmarked_code_keep_their_kind() {
    for (extension, _) in EXTS {
        let row = RowFacts {
            name: format!("file{extension}"),
            icon: "text-x-script".into(),
            mode: 0o100644,
            ..RowFacts::default()
        };
        assert!(matches!(Mark::for_facts(&row, true), Mark::Language(_)));
        assert_eq!(Mark::for_facts(&row, false), Mark::Code);
        assert_eq!(
            Mark::for_facts(
                &RowFacts {
                    dir: true,
                    icon: "folder".into(),
                    ..row.clone()
                },
                true
            ),
            Mark::Folder
        );
        assert_eq!(
            Mark::for_facts(
                &RowFacts {
                    mode: 0o120777,
                    ..row
                },
                true
            ),
            Mark::Symlink
        );
    }
    for file in [
        "constructor",
        "__proto__",
        "toString",
        "main.cs",
        "main.ini",
        "main.conf",
        "main.vim",
        "main.log",
        "main.txt",
        "main.o",
        "main",
        ".bashrc",
        "Makefile.am",
    ] {
        assert_eq!(Language::for_file(file), None, "{file} must keep its kind");
    }
    assert_eq!(Language::for_slug("unknown"), None);
}
