//! Neighbour menu snapshots and choices must not reuse a coincident active row index.
use super::{
    join_tests::{receive, Fixture},
    target::PathMenu,
};
use crate::state::Shell;
use flea::protocol::RowFacts;
use gpui::{AppContext, Context, TestAppContext, Window};

fn open(cx: &mut TestAppContext) -> Fixture {
    let (backend, requests) = crate::backend::Backend::test_lines();
    let handle = cx.add_window(|window, cx| {
        let shell = cx.new(|cx| crate::state::tests::shell_with(backend, cx));
        shell.update(cx, |shell, cx| window.focus(&shell.focus, cx));
        crate::window::root::Root { shell }
    });
    Fixture {
        handle,
        requests: Box::new(requests),
    }
}

fn change<R>(
    cx: &mut TestAppContext,
    fixture: &Fixture,
    run: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> R,
) -> R {
    fixture
        .handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| run(shell, window, cx))
        })
        .unwrap()
}

fn external(cx: &mut TestAppContext, fixture: &Fixture) -> u64 {
    change(cx, fixture, |shell, window, cx| {
        shell.launch.settings.data = flea::jsondoc::parse(r#"{"menu":{"hidden":[]}}"#).unwrap();
        shell.begin_menu(
            0,
            Some(0),
            (30., 50.),
            (
                false,
                Some(PathMenu {
                    base: "/parent".into(),
                    row: Some(RowFacts {
                        name: "other.txt".into(),
                        mode: 0o100644,
                        icon: "text-plain".into(),
                        ..Default::default()
                    }),
                }),
            ),
            window,
            cx,
        );
        shell.menu.captured.as_ref().unwrap().id
    })
}

fn ready(cx: &mut TestAppContext, fixture: &Fixture, id: u64) {
    receive(
        cx,
        fixture.handle,
        &format!(
            r#"{{"t":"menuaction","op":"snapshot","id":{id},"ok":true,"kinds":[32768],"modes":[33188]}}"#
        ),
    );
    receive(
        cx,
        fixture.handle,
        &format!(
            r#"{{"t":"menuaction","op":"applications","id":{id},"ok":true,"applications":[{{"id":"text.desktop","label":"Text","icon":"text","default":true}}]}}"#
        ),
    );
}

fn choose(cx: &mut TestAppContext, fixture: &Fixture, id: u64, action: &str, leaf: &str) {
    change(cx, fixture, |shell, window, cx| {
        shell.menu.pending = Some((action.into(), leaf.into()));
        shell.activate_menu(window, cx);
    });
    let action = if leaf.is_empty() {
        action.into()
    } else {
        format!("{action}:{leaf}")
    };
    receive(
        cx,
        fixture.handle,
        &format!(
            r#"{{"t":"menuaction","op":"validate","id":{id},"ok":true,"action":"{action}","paths":["/parent/other.txt"]}}"#
        ),
    );
    cx.run_until_parked();
}

fn requests(fixture: &mut Fixture) -> Vec<String> {
    std::iter::from_fn(|| (fixture.requests)()).collect()
}

#[gpui::test]
fn ey379_snapshot_and_properties_name_the_neighbour_path(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    let id = external(cx, &fixture);
    let lines = requests(&mut fixture);
    let snapshot = lines
        .iter()
        .find(|line| line.contains(r#""op":"snapshot""#))
        .unwrap();
    assert_eq!(
        flea::json::field_str_array(snapshot, "paths"),
        ["/parent/other.txt"]
    );
    assert!(flea::json::field_usize_array(snapshot, "rows").is_empty());
    ready(cx, &fixture, id);
    choose(cx, &fixture, id, "properties", "");
    let lines = requests(&mut fixture);
    let snapshot = lines
        .iter()
        .find(|line| line.contains(r#""op":"snapshot""#))
        .expect("the Properties card captures its own paths");
    assert_eq!(
        flea::json::field_str_array(snapshot, "paths"),
        ["/parent/other.txt"]
    );
}

#[gpui::test]
fn ey379_copy_as_and_trash_use_the_neighbour_not_active_row_zero(cx: &mut TestAppContext) {
    for (action, leaf) in [("copyAs", "copyName"), ("trash", "")] {
        let mut fixture = open(cx);
        let before = change(cx, &fixture, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            (host.path.clone(), host.pane.cursor, host.selected.clone())
        });
        let id = external(cx, &fixture);
        ready(cx, &fixture, id);
        choose(cx, &fixture, id, action, leaf);
        if action == "copyAs" {
            assert_eq!(
                cx.update(|cx| cx.read_from_clipboard().and_then(|item| item.text()))
                    .as_deref(),
                Some("other.txt")
            );
        } else {
            let trash = requests(&mut fixture)
                .into_iter()
                .find(|line| line.contains(r#""c":"trash""#))
                .expect("trash is sent on the captured side");
            assert_eq!(
                flea::json::field_str_array(&trash, "paths"),
                ["/parent/other.txt"]
            );
        }
        change(cx, &fixture, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(
                (host.path.clone(), host.pane.cursor, host.selected.clone()),
                before
            );
        });
    }
}

#[gpui::test]
fn ey379_tags_and_open_with_another_application_retain_the_path_snapshot(cx: &mut TestAppContext) {
    for (action, leaf) in [("tags", "tag1"), ("openWith", "__another__")] {
        let mut fixture = open(cx);
        let id = external(cx, &fixture);
        ready(cx, &fixture, id);
        change(cx, &fixture, |shell, _, _| {
            shell.menu.context.tag_submenu = Some(super::tags::entries(
                &[flea::ui_logic_rail_places::tags::State::Absent; 7],
            ));
        });
        choose(cx, &fixture, id, action, leaf);
        let lines = requests(&mut fixture);
        if action == "tags" {
            let line = lines
                .iter()
                .find(|line| line.contains(r#""op":"settags""#))
                .expect("tagging names the menu snapshot");
            assert_eq!(flea::json::field_usize(line, "id"), Some(id as usize));
            assert!(flea::json::field_usize_array(line, "rows").is_empty());
        } else {
            let line = lines
                .iter()
                .rev()
                .find(|line| line.contains(r#""op":"snapshot""#))
                .unwrap();
            assert_eq!(
                flea::json::field_str_array(line, "paths"),
                ["/parent/other.txt"]
            );
            change(cx, &fixture, |shell, _, _| {
                assert!(shell.open_with.is_some())
            });
        }
    }
}

#[gpui::test]
fn ey379_a_changed_listing_refuses_external_validation(cx: &mut TestAppContext) {
    let fixture = open(cx);
    let id = external(cx, &fixture);
    ready(cx, &fixture, id);
    change(cx, &fixture, |shell, window, cx| {
        shell.panes[0].as_mut().unwrap().pane.cursor += 1;
        shell.menu.pending = Some(("trash".into(), String::new()));
        shell.activate_menu(window, cx);
        assert!(shell.menu.captured.is_none());
        assert_eq!(shell.menu.context.row_name, "other.txt");
    });
}

#[gpui::test]
fn a_peeked_pdf_or_office_document_offers_convert(cx: &mut TestAppContext) {
    let rows = [
        ("garden-report.pdf", true),
        ("garden-report.docx", true),
        ("notes.txt", false),
    ];
    for (name, document) in rows {
        let fixture = open(cx);
        let id = change(cx, &fixture, |shell, window, cx| {
            shell.launch.settings.data = flea::jsondoc::parse(r#"{"menu":{"hidden":[]}}"#).unwrap();
            let row = RowFacts {
                name: name.into(),
                mode: 0o100644,
                ..Default::default()
            };
            let target = PathMenu {
                base: "/parent".into(),
                row: Some(row),
            };
            shell.begin_menu(0, Some(0), (30., 50.), (false, Some(target)), window, cx);
            shell.menu.captured.as_ref().unwrap().id
        });
        ready(cx, &fixture, id);
        change(cx, &fixture, |shell, _, _| {
            let context = &shell.menu.context;
            assert_eq!(context.row_name, name);
            assert_eq!(
                (context.row_is_document, context.row_is_image),
                (document, false)
            );
            let entries = flea::ui_logic_context_menus::build::listing_entries(context);
            let convert = entries.iter().any(|e| e.action == "convert");
            assert_eq!(convert, document, "{name}");
        });
    }
}
