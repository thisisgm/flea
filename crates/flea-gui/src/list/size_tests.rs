use super::bench::fixture;
use crate::columns::{cache, sets};
use crate::navigation::sort::Size;
use crate::row::format::{size_text, Words};
use flea::i18n::{I18n, Paths};
use std::path::PathBuf;

/// German words: a decimal comma from the language file, and a translated link word.
pub(crate) fn german_words() -> Words {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../src/ui_logic_open_with_properties/fixtures");
    let catalog = I18n::load(Paths {
        omarchy: fixtures.clone(),
        language_file: fixtures.join("language-de"),
        catalogs: Vec::new(),
    });
    Words {
        link: "Verknüpfung".into(),
        ..Words::from_catalog(&catalog)
    }
}

#[test]
fn size_cells_print_one_localized_text_in_the_list_and_the_columns() {
    let words = german_words();
    let rows = fixture("2k");
    let file = rows
        .iter()
        .find(|row| !row.dir)
        .expect("a file row")
        .clone();
    let folder = rows
        .iter()
        .find(|row| row.dir)
        .expect("a folder row")
        .clone();
    let mut link = file.clone();
    link.mode = 0o120777;
    let walk = Some(Size {
        bytes: 22_100,
        partial: false,
    });
    let cases = [(&file, None), (&folder, walk), (&link, None)];
    for (facts, walked) in cases {
        let list = size_text(facts, walked, &words);
        let columns = cache::size_text(facts, walked, &words);
        let cell = sets::cell_text(
            "size",
            facts,
            None,
            None,
            walked.map(|walk| (walk.bytes, walk.partial)),
            &words,
        );
        assert_eq!(columns, list, "the columns cell of {} differs", facts.name);
        assert_eq!(
            cell, list,
            "the column autofit cell of {} differs",
            facts.name
        );
    }
    assert_eq!(size_text(&link, None, &words), "Verknüpfung");
    assert_eq!(
        size_text(&file, None, &words),
        words.locale.format_size(file.size)
    );
    assert!(words.locale.format_size(22_100).contains(','));
}

#[test]
fn bfx_172_2_partial_zero_size_cells_are_unknown() {
    let words = german_words();
    let folder = flea::protocol::RowFacts {
        dir: true,
        size: 4096,
        ..Default::default()
    };
    for (bytes, partial, expected) in [
        (0, true, "Unknown".to_string()),
        (0, false, words.locale.format_size(0)),
        (13, true, format!(">{}", words.locale.format_size(13))),
    ] {
        let walk = Some(Size { bytes, partial });
        assert_eq!(size_text(&folder, walk, &words), expected);
        assert_eq!(cache::size_text(&folder, walk, &words), expected);
        assert_eq!(
            sets::cell_text("size", &folder, None, None, Some((bytes, partial)), &words,),
            expected
        );
    }
}
