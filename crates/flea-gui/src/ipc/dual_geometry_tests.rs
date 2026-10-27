//! The drag reader follows the mounted pane's painted geometry through the real IPC dispatcher.
use crate::{
    backend::Backend,
    ipc::server::Event,
    state::{tests::shell_with, Shell},
    window::startup::View,
};
use flea::{
    ipc::wire::{Reply, Request},
    jsondoc::Json,
    protocol::{Reply as BackendReply, RowFacts},
};
use gpui::{px, size, AppContext, TestAppContext, TestDispatcher, WindowHandle};
use std::rc::Rc;

fn ask(
    cx: &mut TestAppContext,
    window: WindowHandle<Shell>,
    name: &str,
    arguments: &[&str],
) -> Reply {
    window
        .update(cx, |shell, window, cx| {
            let (answer, receive) = std::sync::mpsc::sync_channel(1);
            shell.ipc(
                Event::Call {
                    request: Request::Call {
                        target: "flea".into(),
                        function: name.into(),
                        arguments: arguments.iter().map(|value| (*value).into()).collect(),
                    },
                    answer,
                },
                window,
                cx,
            );
            receive
                .try_recv()
                .expect("the mounted reader answers")
                .reply
        })
        .unwrap()
}

fn field<'a>(value: &'a Json, key: &str) -> &'a str {
    value.get(key).and_then(Json::as_str).unwrap()
}

fn geometry(view: View, filtered: bool) {
    let mut cx = TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        gpui_platform::current_platform(true).text_system(),
    );
    let (backend, mut requests) = Backend::test_channel();
    let window = cx.open_window(size(px(1100.), px(700.)), |_, cx| {
        let mut shell = shell_with(backend, cx);
        shell.view = view;
        let host = shell.panes[0].as_mut().unwrap();
        host.pane = crate::pane::Pane::new("/".into(), 646.);
        host.pane
            .receive(BackendReply::Listed {
                path: "/".into(),
                total: 6,
            })
            .unwrap();
        host.pane
            .receive(BackendReply::Rows {
                listing: 2,
                start: 0,
                rows: (0..6)
                    .map(|index| RowFacts {
                        name: format!("row-\"{index}\\"),
                        dir: index == 1,
                        icon: "text-plain".into(),
                        ..RowFacts::default()
                    })
                    .collect(),
            })
            .unwrap();
        host.changed(&shell.locale);
        host.pane.cursor = if filtered { 5 } else { 0 };
        if filtered {
            host.filter.shown = Some(Rc::from([1, 5]));
        }
        shell
    });
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    let reply = ask(&mut cx, window, "dragPaneGeometry", &["0", "1"]);
    assert_eq!(reply.code, 0, "{view:?}: {reply:?}");
    let value = flea::jsondoc::parse(&reply.stdout).unwrap();
    assert_eq!(value.get("side").and_then(Json::as_f64), Some(0.));
    assert_eq!(
        value.get("active").and_then(Json::as_bool),
        Some(view == View::Dual)
    );
    assert_eq!(value.get("focused").and_then(Json::as_bool), Some(true));
    assert_eq!(value.get("loading").and_then(Json::as_bool), Some(false));
    assert_eq!(value.get("total").and_then(Json::as_f64), Some(6.));
    assert_eq!(field(&value, "path"), "/");
    let expected_view = match view {
        View::List | View::Dual => "list",
        View::Grid => "grid",
        View::Columns => "columns",
    };
    assert_eq!(field(&value, "view"), expected_view);
    let area = field(&value, "area");
    assert!(!area.is_empty(), "{view:?}: the mounted area is not empty");
    let folder = value.get("folder").unwrap();
    assert_eq!(folder.get("index").and_then(Json::as_f64), Some(1.));
    assert_eq!(field(folder, "name"), "row-\"1\\");
    assert_eq!(folder.get("directory").and_then(Json::as_bool), Some(true));
    let rect = field(folder, "rect");
    assert!(
        !rect.is_empty(),
        "{view:?}: the painted folder is not empty"
    );
    let numbers = |text: &str| {
        text.split_whitespace()
            .map(|number| number.parse::<i64>().unwrap())
            .collect::<Vec<_>>()
    };
    let area = numbers(area);
    let rect = numbers(rect);
    assert_eq!(area.len(), 4);
    assert_eq!(rect.len(), 4);
    assert!(rect[2] > 0 && rect[3] > 0, "{view:?}: {rect:?}");
    assert!(rect[0] >= area[0] && rect[0] + rect[2] <= area[0] + area[2]);
    if filtered && matches!(view, View::List | View::Dual) {
        assert_eq!(rect[1], area[1], "filtered row 1 draws at position 0");
    }
    let last = value.get("last").unwrap();
    assert_eq!(last.get("index").and_then(Json::as_f64), Some(5.));
    assert_eq!(field(last, "name"), "row-\"5\\");
    assert!(
        !field(last, "rect").is_empty(),
        "{view:?}: last row is painted"
    );
    for args in [["-1", "0"], ["1", "0"], ["2", "0"], ["0", "-1"], ["0", "6"]] {
        assert_eq!(
            ask(&mut cx, window, "dragPaneGeometry", &args),
            Reply::value("{}")
        );
    }
    while requests().is_some() {}
    assert_eq!(ask(&mut cx, window, "dragPaneGeometry", &["0", "1"]), reply);
    assert!(requests().is_none(), "the reader starts no backend work");
    if filtered {
        let hidden = ask(&mut cx, window, "dragPaneGeometry", &["0", "0"]);
        let hidden = flea::jsondoc::parse(&hidden.stdout).unwrap();
        let folder = hidden.get("folder").unwrap();
        assert_eq!(field(folder, "name"), "row-\"0\\");
        assert_eq!(field(folder, "rect"), "", "a filtered row has no item");
    }
}

#[test]
fn drag_pane_geometry_matches_the_oracle_in_every_painted_view() {
    for view in [View::List, View::Dual, View::Grid, View::Columns] {
        geometry(view, false);
    }
}

#[test]
fn drag_pane_geometry_maps_filtered_listing_indices_to_painted_positions() {
    for view in [View::List, View::Dual, View::Grid, View::Columns] {
        geometry(view, true);
    }
}
