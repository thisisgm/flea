//! Office workbook proofs: a workbook opens its first sheet in the table, and the sheet keys switch between sheets.
use super::*;

// ql-office-sheet: a workbook opens its first sheet's cached CSV in Quick Look's table, [ and ] switch sheets.
#[gpui::test]
fn pv2_office_sheets_open_in_the_table_and_switch(cx: &mut TestAppContext) {
    let (view, asked, code) = office(cx, true);
    open(cx, view, "/d/budget.xlsx");
    assert!(
        asked.borrow()[0].starts_with(r#"{"c":"officesheets","id":1"#),
        "a workbook asks for its sheets"
    );
    assert!(reply(
        cx,
        view,
        r#"{"t":"officesheets","id":1,"state":"ready","names":["Monthly","Savings"],"paths":["/c/m.csv","/c/s.csv"]}"#
    ));
    let table = view.update(cx, |v, _, _| v.table.clone()).unwrap();
    let mut answered = 0;
    for _ in 0..20 {
        draw(cx, view);
        let pending: Vec<String> = code.borrow()[answered..].to_vec();
        if pending.is_empty() {
            break;
        }
        for ask in pending {
            answered += 1;
            assert!(
                ask.contains(r#""path":"/c/m.csv""#),
                "the first sheet's CSV is read"
            );
            let line = answer(&ask, &budget());
            cx.update(|cx| table.update(cx, |t, cx| t.answered(&line, cx)));
            cx.run_until_parked();
        }
    }
    let i18n = crate::test_catalogs::english();
    assert!(
        view.update(cx, |v, _, cx| v.shown(cx)).unwrap(),
        "the sheet's first screen is whole"
    );
    let bar = view.update(cx, |v, _, cx| v.bar(&i18n, cx)).unwrap();
    assert_eq!(bar.glyph, "table", "the bar leads with the table mark");
    assert_eq!(
        bar.count.as_deref(),
        Some("13 rows"),
        "and counts the sheet's rows"
    );
    assert_eq!(
        bar.sheets,
        vec![("Monthly".into(), true), ("Savings".into(), false)],
        "Monthly in foreground, Savings muted"
    );
    assert!(bar.strip.is_none(), "the text card's bar has no pager");
    let facts = view.update(cx, |v, _, cx| v.facts(&i18n, cx)).unwrap();
    assert_eq!(
        facts[0],
        ("Sheets".into(), "2".into()),
        "the facts count the sheets"
    );
    assert!(view
        .update(cx, |v, _, cx| v.action("nextSheet", cx))
        .unwrap());
    draw(cx, view);
    assert!(
        code.borrow()
            .last()
            .unwrap()
            .contains(r#""path":"/c/s.csv""#),
        "] reads the next sheet"
    );
    assert!(
        view.update(cx, |v, _, cx| v.bar(&i18n, cx).sheets[1].1)
            .unwrap(),
        "Savings is current"
    );
    assert!(view
        .update(cx, |v, _, cx| v.action("nextSheet", cx))
        .unwrap());
    draw(cx, view);
    assert!(
        code.borrow()
            .last()
            .unwrap()
            .contains(r#""path":"/c/m.csv""#),
        "] on the last sheet wraps to the first, as the 3MF plate steps wrap"
    );
    assert!(
        view.update(cx, |v, _, cx| v.bar(&i18n, cx).sheets[0].1)
            .unwrap(),
        "Monthly is current again"
    );
    view.update(cx, |v, _, cx| {
        v.set_meta("/d/budget.xlsx", Some("Budget".into()), None, cx)
    })
    .unwrap();
    let facts = view.update(cx, |v, _, cx| v.facts(&i18n, cx)).unwrap();
    assert_eq!(
        facts[1],
        ("Title".into(), "Budget".into()),
        "the document's Title follows the count"
    );
}
