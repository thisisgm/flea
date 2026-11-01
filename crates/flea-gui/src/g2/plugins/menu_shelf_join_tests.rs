//! EY-195: the menu's file and folder choices reach the production Shelf provider.
use super::host_tests::{attach, stop, wait};
use super::*;

#[gpui::test]
fn add_to_shelf_adds_a_file_and_a_folder(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = scratch();
    let (plugin, runtime) = plugin("checksums", &root);
    runtime.shutdown();
    let mut fixture = open(cx, root.clone(), plugin.clone());
    fixture
        .plugins
        .iter_mut()
        .find(|plugin| plugin.manifest.id == "flea.shelf")
        .unwrap()
        .enabled = true;
    attach(cx, &mut fixture, &root, "checksums", &plugin);
    update(cx, fixture.handle, |shell, _, cx| {
        shell.launch.settings.data = flea::uistate::from_file(r#"{"shelf":{"enabled":true}}"#);
    });
    wait(cx, |cx| {
        update(cx, fixture.handle, |shell, _, cx| {
            let host = shell.plugins.host.as_ref().unwrap().read(cx);
            host.plugins
                .iter()
                .any(|p| p.manifest.id == "flea.shelf" && host.healthy(p))
        })
    });
    for (row, name) in [(0, "notes.txt"), (4, "folder")] {
        fixture.menu(cx, row, vec![]);
        wait(cx, |cx| {
            fixture.entries(cx).iter().any(|entry| entry.id == "shelf")
        });
        let validate = fixture.choose(cx, "addToShelf", "").unwrap();
        fixture.validated(cx, &validate, &[root.join(name)]);
        wait(cx, |cx| {
            update(cx, fixture.handle, |shell, _, _| {
                shell.panes[0].as_ref().unwrap().status.as_str() == "Added it to the shelf"
            })
        });
        let pile =
            std::fs::read_to_string(root.join("state/omarchy/flea-shelf/shelf.json")).unwrap();
        let pile = jsondoc::parse(&pile).unwrap();
        let paths = pile
            .get("items")
            .and_then(Json::as_array)
            .unwrap()
            .iter()
            .filter_map(|item| item.get("path").and_then(Json::as_str))
            .collect::<Vec<_>>();
        assert!(
            paths.contains(&root.join(name).to_str().unwrap()),
            "{paths:?}"
        );
    }
    stop(cx, &fixture);
}
