//! The NETWORK row's keyboard menu opens in the list face, at the mark column the rail laid.
use super::*;

#[gpui::test]
fn trash_font_network_menu_opens_in_the_list_face(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx) = open(r#"{"places":{}}"#, recorder.clone(), cx);
    let listings = recorder.listings();
    recorder.answer(listings[0], SHARE_OUTPUT);
    frame(cx);
    shell.update_in(cx, |shell, window, cx| {
        let host = shell.panes[shell.focus_side]
            .as_ref()
            .expect("the focused pane is mounted");
        host.list.borrow_mut().typeface.font.family = "Noto Sans Mono".into();
        let side = shell.focus_side;
        shell.network_menu(side, &share_row(), window, cx);
        let network = shell.rail.as_ref().unwrap().network.as_ref().unwrap();
        let menu = network.menu.borrow();
        let card = menu.card.as_ref().expect("the NETWORK row's menu opened");
        assert_eq!(
            card.built.typeface.font.family.as_ref(),
            "Noto Sans Mono",
            "the NETWORK menu opens in the list's face"
        );
    });
}

#[gpui::test]
fn network_row_menu_answers_the_menu_readers_under_the_row(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx) = open(r#"{"places":{}}"#, recorder.clone(), cx);
    recorder.answer(recorder.listings()[0], SHARE_OUTPUT);
    frame(cx);
    let producer = shell.read_with(cx, |shell, _| shell.rail.as_ref().unwrap().places.clone());
    let index = producer.read_with(cx, |producer, _| {
        producer
            .entries()
            .iter()
            .position(|entry| entry.group == Group::Network)
            .expect("the NETWORK row")
    });
    shell.update_in(cx, |shell, window, cx| {
        producer.update(cx, |producer, cx| producer.set_cursor(index, cx));
        producer.read(cx).model.borrow_mut().pointer_focus = false;
        let side = shell.focus_side;
        shell.network_menu(side, &share_row(), window, cx);
    });
    frame(cx);
    let read = |name: &str, cx: &mut VisualTestContext| {
        crate::window::u07_tests::read(&shell, cx, name, &[])
    };
    assert_eq!(read("contextMenuVisible", cx), "true");
    let drawn = shell.read_with(cx, |shell, _| {
        let network = shell.rail.as_ref().unwrap().network.as_ref().unwrap();
        let menu = network.menu.borrow();
        let card = menu.card.as_ref().expect("the NETWORK row's menu opened");
        (
            card.built
                .lines
                .iter()
                .map(|line| {
                    line.label
                        .as_ref()
                        .map_or("-", |label| label.text.as_str())
                        .to_owned()
                })
                .collect::<Vec<_>>()
                .join("|"),
            card.frame.x,
        )
    });
    assert!(
        drawn.0.contains("Rename"),
        "the share's own rows: {}",
        drawn.0
    );
    assert_eq!(read("contextMenuEntries", cx), drawn.0);
    let inventory = flea::jsondoc::parse(&read("contextMenuModel", cx)).unwrap();
    let labels = inventory
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            row.get("label")
                .and_then(flea::jsondoc::Json::as_str)
                .unwrap_or("-")
        })
        .collect::<Vec<_>>()
        .join("|");
    assert_eq!(
        labels, drawn.0,
        "the menu traversal budget must describe this card"
    );
    assert_eq!(read("contextMenuCursor", cx), "0");
    let row = producer.read_with(cx, |producer, _| {
        producer.laid.borrow().row(index).unwrap().rect.x
    });
    assert_eq!(
        drawn.1,
        row + 14.,
        "the keyboard menu opens at the stock face's mark column, as rail-menu-network draws it"
    );
    // Until the next frame the rail's rows keep the face they were laid with, so the menu keeps their mark column.
    let (row, x) = shell.update_in(cx, |shell, window, cx| {
        let network = shell.rail.as_ref().unwrap().network.as_ref().unwrap();
        network.menu.borrow_mut().close();
        let host = shell.panes[shell.focus_side].as_ref().unwrap();
        host.list.borrow_mut().typeface.base = px(20.);
        let side = shell.focus_side;
        shell.network_menu(side, &share_row(), window, cx);
        let row = producer.read(cx).laid.borrow().row(index).unwrap().rect.x;
        let network = shell.rail.as_ref().unwrap().network.as_ref().unwrap();
        let menu = network.menu.borrow();
        (row, menu.card.as_ref().expect("the menu reopened").frame.x)
    });
    assert_eq!(
        x,
        row + 14.,
        "a face the rail has not laid yet leaves the keyboard menu at the laid mark column"
    );
    // A 20 px face lays the rail's mark 20 px in, so the menu must follow the face rather than a fixed offset.
    frame(cx);
    shell.update_in(cx, |shell, window, cx| {
        let side = shell.focus_side;
        shell.network_menu(side, &share_row(), window, cx);
    });
    frame(cx);
    let (row, x) = shell.read_with(cx, |shell, cx| {
        let row = producer.read(cx).laid.borrow().row(index).unwrap().rect.x;
        let network = shell.rail.as_ref().unwrap().network.as_ref().unwrap();
        let menu = network.menu.borrow();
        (row, menu.card.as_ref().expect("the menu reopened").frame.x)
    });
    assert_eq!(
        x,
        row + 20.,
        "the keyboard menu opens at a 20 px face's mark column"
    );
}
