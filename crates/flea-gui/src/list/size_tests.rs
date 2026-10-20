use super::bench::fixture;
use crate::columns::{cache, sets};
use crate::navigation::sort::Size;
use crate::row::format::{size_text, Words};
use flea::i18n::{I18n, Paths};
use std::path::PathBuf;

/// German words: a decimal comma from the language file, and a translated link word.
pub(crate) fn german_words() -> Words {
    let fixtures =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../src/ui_logic_op3/fixtures");
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
