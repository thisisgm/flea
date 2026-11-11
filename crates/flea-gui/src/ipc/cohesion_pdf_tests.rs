//! The capture reader follows the real strip's hover handler.
use super::*;

#[gpui::test]
fn cohesion_pdf_reports_the_hovered_painted_strip(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (handle, _requests) = open(cx);
    let doc = document(cx, handle, "book.pdf", "application-pdf");
    let Document::Pdf(pdf) = doc.view else {
        panic!("Quick Look must mount the PDF view")
    };
    pdf.update(cx, |pdf, _| pdf.surface.sizes = vec![(612., 792.); 3]);
    reveal(cx, handle);
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    draw(&mut visual);
    let read = |cx: &mut VisualTestContext| {
        jsondoc::parse(&value(cx, handle, "pdfState", &["true"])).unwrap()
    };
    let state = read(&mut visual);
    let next = &state.get("controls").unwrap().as_array().unwrap()[1];
    assert_eq!(next.get("hovered").and_then(Json::as_bool), Some(false));
    let centre: Vec<f32> = next
        .get("centre")
        .unwrap()
        .as_str()
        .unwrap()
        .split_whitespace()
        .map(|n| n.parse().unwrap())
        .collect();
    assert_eq!(centre.len(), 2, "the actual next-page box was painted");
    visual.simulate_mouse_move(
        gpui::point(gpui::px(centre[0]), gpui::px(centre[1])),
        None,
        gpui::Modifiers::none(),
    );
    draw(&mut visual);
    let model_hover = edit(&mut visual, handle, |shell, _, cx| {
        shell.quick_look.as_ref().unwrap().view.read(cx).hovered()
    });
    assert_eq!(
        model_hover, None,
        "only the painted strip handler saw this hover"
    );
    let state = read(&mut visual);
    assert_eq!(
        state.get("controls").unwrap().as_array().unwrap()[1]
            .get("hovered")
            .and_then(Json::as_bool),
        Some(true)
    );
    let name = next.get("name").and_then(Json::as_str).unwrap();
    let cohesion = jsondoc::parse(&value(&mut visual, handle, "cohesionControls", &[])).unwrap();
    let hovered: Vec<_> = cohesion
        .get("controls")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row.get("hovered").and_then(Json::as_bool) == Some(true))
        .map(|row| row.get("name").and_then(Json::as_str).unwrap_or(""))
        .collect();
    assert_eq!(
        hovered,
        [name],
        "the cohesion receipt carries the same painted hover"
    );
}
