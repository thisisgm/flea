use super::*;

#[test]
fn tui_wave7_generated_isolates_keep_real_card_cells_and_inks_fixed() {
    let mut m = model();
    let reply = crate::jsondoc::parse(r#"{"path":"/fx/v/work/notes.txt","kind":"Plain text document","bytes":99,"modified":"2025-06-15T10:00:00","mode":"0644","owner":"gm","uid":1000,"gid":1000}"#).unwrap();
    m.properties = Some(render::cards::properties(&reply, &m.i18n));
    m.sheet = true;
    let theme = Theme::from_text(render::cards::tests::PALETTE);
    let card = render::cards::properties_card(&m);
    let (rows, ink, raw) = render::cards::tests::screen(&m, &theme);
    let controls: Vec<_> = raw
        .chars()
        .filter(|c| matches!(c, '\u{2066}'..='\u{2069}'))
        .collect();
    assert!(
        controls.len() >= 2,
        "real card must emit generated isolation"
    );
    let widths: Vec<_> = controls
        .iter()
        .map(|c| crate::tui::cells::width(*c))
        .collect();
    let (y, x) = (card.y + 3, card.left() + 13);
    let value: String = rows[y].chars().skip(x).take(20).collect();
    let border: String = rows[y]
        .chars()
        .skip(card.x + card.width + 1)
        .take(1)
        .collect();
    assert_eq!(
        (widths.clone(), value, border, ink[y][x].0.clone()),
        (
            vec![0; widths.len()],
            "/fx/v/work/notes.txt".into(),
            "│".into(),
            render::cards::tests::role(&theme.foreground)
        )
    );
}
