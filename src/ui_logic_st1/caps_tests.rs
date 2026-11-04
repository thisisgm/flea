// Board lang-caps: each catalog string and the heading 0.4.0 draws for it.
use super::heading;

#[test]
fn every_row_of_the_caps_board() {
    let rows = [
        ("en", "Places", "PLACES"),
        ("de", "Geräte", "GERÄTE"),
        ("de", "Textgröße", "TEXTGRÖSSE"),
        ("tr", "Cihazlar", "CİHAZLAR"),
        ("el", "Τοποθεσίες", "ΤΟΠΟΘΕΣΙΕΣ"),
        ("ru", "Места", "МЕСТА"),
        ("ja", "場所", "場所"),
        ("ar", "الأماكن", "الأماكن"),
    ];
    for (language, catalog, drawn) in rows {
        assert_eq!(heading(catalog, language), drawn, "{language} {catalog}");
    }
}

#[test]
fn omarchys_own_headings_read_the_same_way() {
    assert_eq!(heading("Metin boyutu", "tr_TR"), "METİN BOYUTU");
    assert_eq!(heading("Μέγεθος κειμένου", "el-GR"), "ΜΕΓΕΘΟΣ ΚΕΙΜΕΝΟΥ");
    assert_eq!(heading("Размер текста", "ru"), "РАЗМЕР ТЕКСТА");
}

#[test]
fn turkish_dotless_i_and_greek_diaeresis_hold() {
    assert_eq!(heading("ılık", "tr"), "ILIK");
    assert_eq!(heading("Ϊ ΐ", "el"), "Ϊ Ϊ");
    // Outside Turkish an i takes the plain capital.
    assert_eq!(heading("i", "de"), "I");
    assert_eq!(heading("", "el"), "");
}
