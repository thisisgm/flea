//! I50-26: a control row's value and caption yield to the label's longest word, so the label never breaks inside one.
use super::draw::Line;
use super::geometry::{control_room, Scale, VALUE_SHARE};
use super::wrap::unbroken;
use flea::ui_logic_settings_pages::search::Run;
use flea::ui_logic_settings_pages::{
    rows::{Kind, Row},
    Text,
};

// A pane narrow enough that the value's whole share would reach into the label's word.
const PANE: f32 = 300.;

/// Eight pixels a character, for data text only.
fn width(text: &Text) -> f32 {
    match text {
        Text::Data(value) => value.chars().count() as f32 * 8.,
        _ => 0.,
    }
}

fn action(value: &str) -> Row {
    Row {
        kind: Kind::Action,
        label: Text::data("Spalten"),
        value: Text::data(value),
        ..Row::default()
    }
}

#[test]
fn i50_fit_room_value_yields_to_the_longest_word() {
    let s = Scale::default();
    let word = width(&Text::data("Spalten"));
    // A value as wide as its whole share would leave the label less than its one word.
    let long = action(&"x".repeat(60));
    assert!(control_room(&long, false, PANE, s, 0., width, width).label < word);
    let room = control_room(&long, false, PANE, s, word, width, width);
    assert!(room.label >= word, "{room:?} cuts the word {word}px");
    assert!(room.value < width(&long.value).min(PANE * VALUE_SHARE));
    // A short value keeps its whole width, and the label takes the rest.
    let short = action("Name");
    let room = control_room(&short, false, PANE, s, word, width, width);
    assert!(room.value >= width(&short.value));
    assert!(room.label > word);
    // A caption yields the same way.
    let captioned = Row {
        caption: Text::data("y".repeat(60)),
        ..action("Name")
    };
    let room = control_room(&captioned, false, PANE, s, word, width, width);
    assert!(room.label >= word, "{room:?} cuts the word {word}px");
}

#[test]
fn i50_fit_room_pieces_keep_words_whole() {
    // A hyphenated word, a Greek one and an Arabic one each stay one piece, as the room must hold each whole.
    assert_eq!(
        unbroken("E-Mail-Adresse anzeigen"),
        ["E-Mail-Adresse", "anzeigen"]
    );
    assert_eq!(
        unbroken("\u{3a0}\u{3c1}\u{3bf}\u{3b2}\u{3bf}\u{3bb}\u{3ae}"),
        ["\u{3a0}\u{3c1}\u{3bf}\u{3b2}\u{3bf}\u{3bb}\u{3ae}"]
    );
    assert_eq!(
        unbroken(
            "\u{627}\u{644}\u{639}\u{631}\u{636} \u{627}\u{644}\u{645}\u{644}\u{641}\u{627}\u{62a}"
        )
        .len(),
        2
    );
    // Kana and Han break between every character, and a Latin run after one rides on it.
    assert_eq!(
        unbroken("\u{30d7}\u{30ec}\u{30d3}\u{30e5}\u{30fc} Git"),
        ["\u{30d7}", "\u{30ec}", "\u{30d3}", "\u{30e5}", "\u{30fc}", "Git"]
    );
    assert_eq!(unbroken("\u{8868}\u{793a}Git"), ["\u{8868}", "\u{793a}Git"]);
    assert_eq!(
        unbroken("\u{8bbe}\u{7f6e}\u{ff08}\u{9ad8}\u{7ea7}\u{ff09}"),
        ["\u{8bbe}", "\u{7f6e}", "\u{ff08}", "\u{9ad8}", "\u{7ea7}", "\u{ff09}"]
    );
    // GPUI breaks before an opening mark or a slash, and never at a no-break space.
    assert_eq!(unbroken("Datei (neu)! c/d"), ["Datei", "(neu)!", "c", "/d"]);
    assert_eq!(unbroken("\u{201e}Neu\u{201c}"), ["\u{201e}Neu", "\u{201c}"]);
    // Every mark GPUI keeps in a word stays inside its piece, as l’écran does.
    for mark in "\u{2019}\u{2018}\u{201d}\u{bb}\u{2026}\u{22ef}\u{2011}\u{202f}".chars() {
        let word = format!("l{mark}cran");
        assert_eq!(unbroken(&word), [word.as_str()], "{mark:?}");
    }
    assert_eq!(
        unbroken("aper\u{e7}u\u{a0}complet"),
        ["aper\u{e7}u\u{a0}complet"]
    );
    assert!(unbroken("  ").is_empty());
}

#[test]
fn i50_fit_room_bold_run_is_the_lifted_hits() {
    let row = action("Name");
    let run = Some(Run {
        start: 1,
        length: 2,
    });
    assert_eq!(
        Line::result(&row, run, Some(0), true).bold_run(),
        Some((1, 2))
    );
    assert_eq!(Line::result(&row, run, Some(1), false).bold_run(), None);
    assert_eq!(Line::result(&row, None, Some(2), true).bold_run(), None);
}

#[gpui::test]
fn i50_fit_room_gpui_breaks_only_between_pieces(cx: &mut gpui::TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|_, cx| super::view::SettingsView::new(super::tests::panel("{}"), cx));
    // Latin and Han: GPUI also breaks between any two Greek, Arabic, Hebrew or Thai letters, which no room prevents.
    let labels = [
        "E-Mail-Adresse beim Start anzeigen",
        "Spalten in der Spaltenansicht",
        "プレビューを表示する Git",
        "Datei (neu)! a:b c/d „Neu“ öffnen",
        "表示Git und mehr",
        "设置（高级）显示全部文件",
        "l\u{2019}\u{e9}cran d\u{2018}ici\u{201d} \u{ab}mehr\u{bb} Ende\u{2026} a\u{22ef}b E\u{2011}Mail 10\u{202f}km",
    ];
    let breaks = cx.update(|window, cx| {
        let font = view.read(cx).typeface.font.clone();
        let size = view.read(cx).typeface.body;
        let mut breaks = 0;
        for label in labels {
            let pieces = unbroken(label);
            let starts: Vec<usize> = pieces
                .iter()
                .map(|piece| piece.as_ptr() as usize - label.as_ptr() as usize)
                .collect();
            let run = gpui::TextRun {
                len: label.len(),
                font: font.clone(),
                color: Default::default(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let widest = pieces
                .iter()
                .map(|piece| view.read(cx).shaped((*piece).to_owned(), size, window))
                .fold(0., f32::max);
            let full = view.read(cx).shaped(label.to_owned(), size, window);
            // Every whole pixel from the widest piece's width to the label's: each break GPUI makes lands where a piece starts.
            for wrap in widest.ceil() as usize..=full.ceil() as usize {
                let lines = window
                    .text_system()
                    .shape_text(
                        label.into(),
                        size,
                        std::slice::from_ref(&run),
                        Some(gpui::px(wrap as f32)),
                        None,
                    )
                    .expect("the label shapes");
                for line in &lines {
                    for boundary in line.wrap_boundaries() {
                        let at = line.unwrapped_layout.runs[boundary.run_ix].glyphs
                            [boundary.glyph_ix]
                            .index;
                        // A break on the space before a piece is a break between pieces too.
                        assert!(
                            starts.contains(&at) || label[at..].starts_with(' '),
                            "{label:?} breaks at byte {at} at {wrap}px"
                        );
                        breaks += 1;
                    }
                }
            }
        }
        breaks
    });
    assert!(breaks > 0, "no label wrapped, so this test proves nothing");
}
