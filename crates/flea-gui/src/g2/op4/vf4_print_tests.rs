// The print card names the printer by its advertised name and falls back to the queue ID.
use super::*;

fn printer_name(field: &str) -> (String, String) {
    let c = crate::test_catalogs::english();
    let mut m = draft(Action::Print, Kind::Pdf, &["garden-report.pdf"]);
    let id = printer_ticket(&m).request;
    let line = format!(
        r#"{{"t":"printoptions","id":{id},"ok":true,"destinations":[{{"name":"flea-private","default":true}}],"printer":"flea-private"{field},"duplex":false,"twoSided":false,"color":false,"grayscale":false}}"#
    );
    assert!(m.line(&line, &c));
    let Some(Card::Print(p)) = &m.card else {
        panic!()
    };
    (p.printer.to_string(), p.printer_name.to_string())
}

#[test]
fn vf4_printer_query_keeps_human_name_separate_from_queue_id() {
    assert_eq!(
        printer_name(r#","printerName":"Brother HL-L3270CDW""#),
        ("flea-private".into(), "Brother HL-L3270CDW".into())
    );
}

#[test]
fn vf4_r1_a_missing_or_blank_printer_name_falls_back_to_the_queue_id() {
    for field in ["", r#","printerName":"""#, r#","printerName":"   ""#] {
        assert_eq!(
            printer_name(field),
            ("flea-private".into(), "flea-private".into()),
            "{field}"
        );
    }
}
