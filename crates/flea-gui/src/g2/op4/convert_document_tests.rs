// The Convert card on one PDF (formats, PAGES, page outputs) and on one office document (PDF only), and the request each sends.
use super::{
    model::{Action, Card, Focus},
    seam::Ticket,
    tests::{checked, draft},
};
use flea::ui_logic_op4::output::Kind;

const TICKET: Ticket = Ticket {
    generation: 11,
    request: 99,
};

#[test]
fn op4_a_pdf_sends_its_pages_and_an_office_document_converts_to_pdf() {
    let c = crate::test_catalogs::english();
    let mut m = draft(Action::Convert, Kind::Pdf, &["garden-report.pdf"]);
    assert_eq!(m.focus_order()[..2], [Focus::Formats, Focus::Pages]);
    let ticket = match &m.card {
        Some(Card::Convert(s)) => s.count_pending.unwrap(),
        _ => panic!(),
    };
    m.document_pages(ticket, Ok(5), &c);
    if let Some(Card::Convert(s)) = &mut m.card {
        assert_eq!(s.format, "png");
        s.pages = "1-3, 5".into();
    }
    m.probe(&c);
    checked(&mut m);
    let Some(Card::Convert(s)) = &m.card else {
        panic!()
    };
    assert_eq!(
        (s.outputs.first().unwrap(), s.outputs.last().unwrap()),
        (
            &"/fixture/garden-report (page 1).png".to_string(),
            &"/fixture/garden-report (page 5).png".to_string()
        )
    );
    assert_eq!(s.outputs.len(), 4);
    assert!(m.can_submit());
    let request = super::wire::request(&m, TICKET).unwrap();
    assert!(request.contains(r#""c":"convertbatch""#), "{request}");
    assert!(request.contains(r#""format":"png""#), "{request}");
    assert!(request.contains(r#""pages":"1-3, 5""#), "{request}");

    let mut m = draft(Action::Convert, Kind::Office, &["garden-report.docx"]);
    assert_eq!(m.focus, Some(Focus::Submit));
    assert!(!m.focus_order().contains(&Focus::Formats));
    checked(&mut m);
    let Some(Card::Convert(s)) = &m.card else {
        panic!()
    };
    assert_eq!(s.outputs, ["/fixture/garden-report (converted).pdf"]);
    assert!(m.can_submit());
    let request = super::wire::request(&m, TICKET).unwrap();
    assert!(request.contains(r#""format":"pdf""#), "{request}");
}
