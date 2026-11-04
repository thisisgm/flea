use super::*;

// ui/js/PreviewKeys.js pdfAction and ui/PdfViewer.qml: the strip's controls, its focus walk and the zoom ladder.
#[test]
fn viewer_opens_fitted_with_no_control_picked() {
    let mut viewer = Viewer {
        zoom: 2.5,
        expanded: true,
        control: Some(3),
    };
    viewer.subject_changed();
    assert_eq!(
        (viewer.zoom, viewer.control),
        (ZOOM_MIN, None),
        "a new document opens fitted with no control picked"
    );
    assert!(
        viewer.expanded,
        "a new document keeps the window's expand state"
    );
}

#[test]
fn viewer_zoom_walks_the_ladder() {
    let mut viewer = Viewer::default();
    assert_eq!(
        viewer.zoom_by(-1),
        Effect::Zoom(1.),
        "the bottom rung always fits the frame"
    );
    assert_eq!(viewer.zoom_by(1), Effect::Zoom(1.25), "one step a press");
    assert_eq!(zoom_step(3.9, 1), ZOOM_MAX, "the top rung holds");
    assert_eq!(
        zoom_step(1.5, -10),
        ZOOM_MIN,
        "many presses down hold at the fit"
    );
}

#[test]
fn viewer_pager_shows_only_past_one_page() {
    assert!(
        !Viewer::visible(Control::Previous, 1),
        "a one-page document has no chevrons"
    );
    assert!(
        !Viewer::visible(Control::Next, 0),
        "nor does a document with no pages"
    );
    assert!(
        Viewer::visible(Control::Next, 3),
        "three pages draw the chevrons"
    );
    let viewer = Viewer::default();
    assert!(
        !viewer.enabled(Control::Previous, 0, 3),
        "page 1 cannot turn back"
    );
    assert!(
        viewer.enabled(Control::Next, 0, 3),
        "page 1 of 3 turns forward"
    );
    assert!(
        !viewer.enabled(Control::Next, 2, 3),
        "the last page cannot turn forward"
    );
    assert!(
        !viewer.enabled(Control::ZoomOut, 0, 3),
        "the fit cannot zoom out"
    );
    assert!(
        !viewer.enabled(Control::ZoomIn, 0, 0),
        "nothing loaded cannot zoom"
    );
}

#[test]
fn viewer_tab_walks_drawn_enabled_controls() {
    let mut viewer = Viewer::default();
    assert_eq!(
        viewer.action(Action::FocusNext, 0, 3),
        Effect::Focus(Some(1)),
        "Tab from none skips the dead previous"
    );
    assert_eq!(
        viewer.action(Action::FocusNext, 0, 3),
        Effect::Focus(Some(3)),
        "and the dead zoom out"
    );
    assert_eq!(
        viewer.action(Action::FocusPrevious, 0, 3),
        Effect::Focus(Some(1)),
        "Shift+Tab walks back"
    );
    let mut viewer = Viewer::default();
    assert_eq!(
        viewer.action(Action::FocusPrevious, 0, 1),
        Effect::Focus(Some(5)),
        "Shift+Tab from none lands on close"
    );
    assert_eq!(
        viewer.action(Action::FocusNext, 0, 1),
        Effect::Focus(Some(3)),
        "a one-page document skips the hidden pager"
    );
}

#[test]
fn viewer_enter_presses_the_focused_control() {
    let mut viewer = Viewer::default();
    assert_eq!(
        viewer.action(Action::Open, 0, 3),
        Effect::None,
        "Enter with no control picked does nothing"
    );
    viewer.control = Some(1);
    assert_eq!(
        viewer.action(Action::Open, 0, 3),
        Effect::Turn(1),
        "Enter on next turns forward"
    );
    viewer.control = Some(0);
    assert_eq!(
        viewer.action(Action::Preview, 0, 3),
        Effect::None,
        "a dead control takes no press"
    );
    viewer.control = Some(4);
    assert_eq!(
        viewer.action(Action::Open, 0, 3),
        Effect::ToggleExpand,
        "Enter on expand toggles it"
    );
    assert!(viewer.expanded, "and the viewer is expanded");
    viewer.control = Some(5);
    assert_eq!(
        viewer.action(Action::Open, 0, 3),
        Effect::Close,
        "Enter on close closes"
    );
}

#[test]
fn viewer_keys_turn_zoom_and_scroll() {
    let mut viewer = Viewer::default();
    assert_eq!(
        viewer.action(Action::SeekBack, 1, 3),
        Effect::Turn(-1),
        "seek back turns back"
    );
    assert_eq!(
        viewer.action(Action::Parent, 1, 3),
        Effect::Turn(-1),
        "parent turns back"
    );
    assert_eq!(
        viewer.action(Action::PageForward, 1, 3),
        Effect::Turn(1),
        "page forward turns forward"
    );
    assert_eq!(
        viewer.action(Action::ZoomIn, 1, 3),
        Effect::Zoom(1.25),
        "zoom in steps up"
    );
    assert_eq!(
        viewer.action(Action::CursorDown, 1, 3),
        Effect::Scroll(1),
        "j scrolls down"
    );
    assert_eq!(
        viewer.action(Action::CursorUp, 1, 3),
        Effect::Scroll(-1),
        "k scrolls up"
    );
    assert_eq!(
        Action::from_name("zoomIn"),
        Some(Action::ZoomIn),
        "the key table's name maps"
    );
    assert_eq!(Action::from_name("escape"), None, "escape is the host's");
}

#[test]
fn viewer_column_has_no_strip() {
    assert_eq!(
        column_action(Action::Open),
        Effect::ToggleExpand,
        "Return opens Quick Look at the page in view"
    );
    assert_eq!(
        column_action(Action::Expand),
        Effect::ToggleExpand,
        "e does too"
    );
    assert_eq!(
        column_action(Action::SeekForward),
        Effect::Turn(1),
        "the column turns forward"
    );
    assert_eq!(
        column_action(Action::ZoomIn),
        Effect::None,
        "the column has no zoom"
    );
    assert_eq!(
        column_action(Action::FocusNext),
        Effect::None,
        "nor a strip to walk"
    );
}

#[test]
fn viewer_scroll_is_held_inside_the_content() {
    assert_eq!(scroll_by(0., 1, 28., 1000., 400.), 28., "one row down");
    assert_eq!(
        scroll_by(590., 1, 28., 1000., 400.),
        600.,
        "the bottom holds"
    );
    assert_eq!(scroll_by(10., -1, 28., 1000., 400.), 0., "the top holds");
    assert_eq!(
        scroll_by(0., 1, 28., 300., 400.),
        0.,
        "content shorter than the frame never scrolls"
    );
}

#[test]
fn viewer_glyphs_and_names_match_the_strip() {
    let glyphs: Vec<&str> = CONTROLS.iter().map(|c| c.glyph()).collect();
    assert_eq!(
        glyphs,
        [
            "chevron-left",
            "chevron-right",
            "minus",
            "plus",
            "maximize",
            "x"
        ],
        "the strip's glyphs in order"
    );
    assert!(
        CONTROLS
            .iter()
            .all(|c| c.name_key().starts_with("document.pdf_")),
        "every name is a catalog key"
    );
}

#[test]
fn viewer_strip_matches_the_boards() {
    let mut viewer = Viewer::default();
    let one = viewer.strip(0, 1);
    let drawn: Vec<Control> = one
        .iter()
        .filter(|item| item.visible)
        .map(|item| item.control)
        .collect();
    assert_eq!(
        drawn,
        vec![
            Control::ZoomOut,
            Control::ZoomIn,
            Control::Expand,
            Control::Close
        ],
        "ql-pdf: one page draws no chevrons"
    );
    assert!(
        !one[2].enabled && one[3].enabled,
        "zoom out rests disabled at the fitted zoom"
    );
    let three = viewer.strip(0, 3);
    assert!(
        three[0].visible && !three[0].enabled && three[1].enabled,
        "ql-pdf3: Previous dims on page 1, Next is live"
    );
    assert!(
        three.iter().all(|item| !item.focused),
        "no control is focused as the document opens"
    );
    viewer.action(Action::FocusNext, 0, 3);
    assert_eq!(
        viewer.strip(0, 3).iter().position(|item| item.focused),
        Some(1),
        "Tab lands on the first enabled control"
    );
    assert_eq!(counter(0, 3), Some((1, 3)), "the count reads 1 / 3");
    assert_eq!(counter(0, 0), None, "no count before the pages are known");
}

#[test]
fn bfx_keys_pdf_aliases_turn_pages_in_both_viewers() {
    use super::*;
    for (name, delta) in [
        ("previewPageForward", 1),
        ("previewPageBack", -1),
        ("pageDown", 1),
        ("pageUp", -1),
    ] {
        let action = Action::from_name(name).expect("document paging action");
        assert_eq!(Viewer::default().action(action, 1, 3), Effect::Turn(delta));
        assert_eq!(column_action(action), Effect::Turn(delta));
    }
    assert_eq!(Action::from_name("seekBack"), Some(Action::SeekBack));
    assert_eq!(Action::from_name("seekForward"), Some(Action::SeekForward));
    assert_eq!(Action::from_name("nextSheet"), None);
}
