use flea::protocol::RowFacts;
use flea_gui::row::marks::{Language, Mark, EXTS, FILES};
use std::path::PathBuf;

// The shipped QML's own tables, read from the tree so a drift on either side fails here.
fn oracle(file: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../ui/js")
        .join(file);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

// The "key": "value" pairs inside one `var NAME = { ... }` block.
fn object(text: &str, name: &str) -> Vec<(String, String)> {
    let start = text
        .find(&format!("var {name} = {{"))
        .unwrap_or_else(|| panic!("no {name} table"));
    let body = &text[start..start + text[start..].find("\n}").unwrap()];
    let quoted: Vec<&str> = body.split('"').skip(1).step_by(2).collect();
    quoted
        .chunks(2)
        .map(|pair| (pair[0].to_owned(), pair[1].to_owned()))
        .collect()
}

fn array(text: &str, name: &str) -> Vec<String> {
    let start = text
        .find(&format!("var {name} = ["))
        .unwrap_or_else(|| panic!("no {name} list"));
    let body = &text[start..start + text[start..].find(']').unwrap()];
    body.split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect()
}

#[test]
fn language_tables_match_lang_marks_js_entry_for_entry() {
    let js = oracle("LangMarks.js");
    let pairs = |table: &[(&str, &str)]| {
        table
            .iter()
            .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
            .collect::<Vec<_>>()
    };
    assert_eq!(pairs(&EXTS), object(&js, "EXTS"));
    assert_eq!(pairs(&FILES), object(&js, "FILES"));
    assert_eq!(Language::slugs().collect::<Vec<_>>(), array(&js, "KEYS"));
    let paths = object(&oracle("LangMarkPaths.js"), "PATHS");
    assert_eq!(paths.len(), Language::slugs().count());
    for (slug, d) in paths {
        let language = Language::for_slug(&slug).unwrap_or_else(|| panic!("no logo for {slug}"));
        let (key, svg) = Mark::Language(language).svg();
        assert_eq!(key.as_ref(), format!("flea-lang:{slug}"));
        let svg = std::str::from_utf8(svg).unwrap();
        assert!(
            svg.contains(&format!("d=\"{d}\"")),
            "the {slug} logo is not LangMarkPaths.js's"
        );
        assert!(
            svg.contains("d=\"M4 7V2h10l6 6v14H17 M14 2v6h6\""),
            "the {slug} mark lost its page"
        );
    }
}

#[test]
fn a_language_is_found_by_basename_then_last_extension_case_blind() {
    let slug = |name: &str| Language::for_file(name).map(Language::slug);
    assert_eq!(slug("main.rs"), Some("rust"));
    assert_eq!(slug("Dockerfile"), Some("docker"));
    assert_eq!(slug("CMakeLists.txt"), Some("cmake"));
    assert_eq!(slug(".gitignore"), Some("git"));
    assert_eq!(slug("deep/path/App.TSX"), Some("react"));
    assert_eq!(slug("archive.tar.json"), Some("json"));
    assert_eq!(slug("notes.txt"), None);
    assert_eq!(slug(".rs"), None, "a leading dot is not an extension");
    let file = |name: &str, mode: u32, dir: bool| RowFacts {
        name: name.into(),
        mode,
        dir,
        icon: "text-x-script".into(),
        ..RowFacts::default()
    };
    assert!(matches!(
        Mark::for_facts(&file("main.rs", 0o100644, false), true),
        Mark::Language(_)
    ));
    assert_eq!(
        Mark::for_facts(&file("main.rs", 0o120777, false), true),
        Mark::Symlink,
        "a link never wears a logo"
    );
    assert_eq!(
        Mark::for_facts(&file("src.rs", 0o040755, true), true),
        Mark::Code,
        "a folder never wears a logo"
    );
    assert_eq!(
        Mark::for_facts(&file("main.rs", 0o100644, false), false),
        Mark::Code
    );
}

#[test]
fn glyph_names_follow_icons_js() {
    let js = oracle("Icons.js");
    let glyphs = object(&js, "GLYPHS");
    assert!(glyphs.len() >= 17, "the GLYPHS table did not parse");
    for (icon, glyph) in glyphs {
        assert_eq!(Mark::for_row(&icon, 0o100644).name(), glyph, "icon {icon}");
        assert_eq!(
            Mark::for_row(&icon, 0o120777).name(),
            "symlink",
            "a link to {icon} keeps the link mark"
        );
    }
    assert_eq!(
        Mark::for_row("application-x-unknown", 0o100644).name(),
        "file"
    );
}
