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

#[gpui::test]
fn network_row_menu_owns_cohesion_over_an_unfolded_transfer(cx: &mut TestAppContext) {
    use gpui::VisualContext;
    let recorder = Rc::new(Recorder::default());
    let (shell, cx) = open(r#"{"places":{}}"#, recorder.clone(), cx);
    recorder.answer(recorder.listings()[0], SHARE_OUTPUT);
    // The transfer card paints under the window's Root, as in the native launch.
    cx.replace_root_view(|_, _| crate::window::root::Root {
        shell: shell.clone(),
    });
    frame(cx);
    let controls = |cx: &mut VisualTestContext| {
        jsondoc::parse(&crate::window::u07_tests::read(
            &shell,
            cx,
            "cohesionControls",
            &[],
        ))
        .unwrap()
    };
    let surfaces = |value: &jsondoc::Json| -> Vec<String> {
        value
            .get("controls")
            .and_then(jsondoc::Json::as_array)
            .unwrap()
            .iter()
            .filter_map(|row| {
                row.get("surface")
                    .and_then(jsondoc::Json::as_str)
                    .map(str::to_owned)
            })
            .collect()
    };
    cx.update(|window, cx| shell.update(cx, |shell, cx| {
        shell.open_overlay_copy_to(false, "/files", window, cx);
        shell.overlay.as_ref().unwrap().update(cx, |overlay, cx| {
            overlay.model.dialog = None;
            assert!(overlay.line(r#"{"t":"job","job":{"id":12,"kind":"copy","head":"Copying 0 of 2","file":"notes.txt","done":0,"total":2,"state":"running","output":[]},"unfinished":1}"#, cx));
        });
    }));
    cx.executor()
        .advance_clock(Duration::from_millis(crate::pane::swap::HOLD_MS));
    frame(cx);
    assert!(surfaces(&controls(cx)).iter().any(|s| s == "TransferCard"));
    shell.update_in(cx, |shell, window, cx| {
        let side = shell.focus_side;
        shell.network_menu(side, &share_row(), window, cx);
    });
    frame(cx);
    let observed = surfaces(&controls(cx));
    assert!(observed.iter().any(|s| s == "ContextMenu"), "{observed:?}");
    assert!(
        !observed.iter().any(|s| s == "TransferCard"),
        "{observed:?}"
    );
}
