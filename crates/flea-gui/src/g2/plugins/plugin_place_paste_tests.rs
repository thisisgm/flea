use super::{place_model::*, place_ops::Action, rail_place_fixture::*};
use crate::{state::Shell, window::navigate::Step};
use flea::jsondoc::Json;
use gpui::{Entity, TestAppContext, VisualTestContext};

const ROOT: &str = "plugin://example.virtual-place/";
const PHOTOS: &str = "plugin://example.virtual-place/Photos";

fn open(cx: &mut TestAppContext) -> (Fixture, Entity<Shell>, &mut VisualTestContext) {
    let (mut fixture, shell, cx) = Fixture::open("virtual-place", cx);
    navigate(&shell, Step::OpenPlace(ROOT.into()), cx);
    settle(&mut fixture, cx);
    (fixture, shell, cx)
}

fn settle(fixture: &mut Fixture, cx: &mut VisualTestContext) {
    fixture.pump(cx);
    frame(cx);
    fixture.pump(cx);
    frame(cx);
}

fn verb(shell: &Entity<Shell>, action: &str, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        shell.update(cx, |s, cx| {
            assert!(s.clipboard_action(action, window, cx));
        })
    });
}

fn names(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Vec<String> {
    shell.read_with(cx, |s, _| {
        s.panes[0]
            .as_ref()
            .unwrap()
            .pane
            .held
            .rows
            .iter()
            .map(|r| r.name.clone())
            .collect()
    })
}

fn paste(cx: &mut TestAppContext, cutting: bool, read_only: bool) {
    let (mut fixture, shell, cx) = open(cx);
    fixture.read_only = read_only;
    shell.update(cx, |s, _| s.panes[0].as_mut().unwrap().pane.cursor = 0);
    verb(&shell, if cutting { "cut" } else { "copy" }, cx);
    settle(&mut fixture, cx);
    navigate(&shell, Step::OpenPlace(PHOTOS.into()), cx);
    settle(&mut fixture, cx);
    verb(&shell, "paste", cx);
    settle(&mut fixture, cx);
    let op = if cutting { "move" } else { "copy" };
    assert_eq!(
        fixture.count(op),
        1,
        "clipboard must reach {op}; commands: {:?}",
        fixture.pane_calls
    );
    let request = fixture.calls.iter().find(|r| text(r, "op") == op).unwrap();
    assert_eq!(text(request, "dest"), "/Photos");
    assert_eq!(request.get("sourceLocal"), Some(&Json::Bool(false)));
    assert_eq!(request.get("destinationLocal"), Some(&Json::Bool(false)));
    assert_eq!(request.get("overwrite"), Some(&Json::Bool(false)));
    if read_only {
        assert_eq!(names(&shell, cx), ["lake.png"]);
        shell.read_with(cx, |s, cx| {
            assert!(s.panes[0]
                .as_ref()
                .unwrap()
                .status
                .contains("This operation is unavailable in a plugin place."));
            let operations = &s.operation_view(0).unwrap().read(cx).model;
            assert!(!operations.wire.transfer.running);
            assert_eq!(operations.collide.state(), "idle");
        });
        fixture.read_only = false;
        verb(&shell, "paste", cx);
        settle(&mut fixture, cx);
        assert_eq!(
            fixture.count(op),
            2,
            "a refusal must leave the next paste usable"
        );
        assert!(names(&shell, cx).contains(&"notes.txt".into()));
    } else {
        assert!(names(&shell, cx).contains(&"notes.txt".into()));
    }
    navigate(&shell, Step::OpenPlace(ROOT.into()), cx);
    settle(&mut fixture, cx);
    assert_eq!(
        names(&shell, cx).contains(&"notes.txt".into()),
        !cutting || read_only
    );
}

#[gpui::test]
fn plugin_place_paste_copy_reaches_writable_place(cx: &mut TestAppContext) {
    paste(cx, false, false);
}

#[gpui::test]
fn plugin_place_paste_cut_reaches_writable_place(cx: &mut TestAppContext) {
    paste(cx, true, false);
}

#[gpui::test]
fn plugin_place_paste_non_writable_keeps_refusal(cx: &mut TestAppContext) {
    paste(cx, false, true);
}

fn copy_to(cx: &mut TestAppContext, moving: bool) {
    let (mut fixture, shell, cx) = open(cx);
    let listing = shell.read_with(cx, |s, _| s.panes[0].as_ref().unwrap().pane.held.listing);
    shell.update(cx, |s, _| {
        s.panes[0]
            .as_ref()
            .unwrap()
            .backend
            .as_ref()
            .unwrap()
            .operation(format!(
                r#"{{"c":"menuaction","op":"snapshot","id":71,"listing":{listing},"rows":[0]}}"#
            ))
            .unwrap()
    });
    settle(&mut fixture, cx);
    cx.update(|window, cx| {
        shell.update(cx, |s, cx| {
            s.copy_to_operation(0, 71, moving, PHOTOS, window, cx);
            s.operation_view(0).unwrap().update(cx, |view, _| {
                view.model
                    .key(&crate::test_catalogs::english(), "enter", false);
            });
            s.flush_operations(0, cx);
        })
    });
    settle(&mut fixture, cx);
    let op = if moving { "move" } else { "copy" };
    assert_eq!(
        fixture.count(op),
        1,
        "Copy To must reach {op} for a plugin URI"
    );
    navigate(&shell, Step::OpenPlace(PHOTOS.into()), cx);
    settle(&mut fixture, cx);
    assert!(names(&shell, cx).contains(&"notes.txt".into()));
    navigate(&shell, Step::OpenPlace(ROOT.into()), cx);
    settle(&mut fixture, cx);
    assert_eq!(names(&shell, cx).contains(&"notes.txt".into()), !moving);
}

#[gpui::test]
fn plugin_place_paste_copy_to_plugin_destination(cx: &mut TestAppContext) {
    copy_to(cx, false);
}

#[gpui::test]
fn plugin_place_paste_move_to_plugin_destination(cx: &mut TestAppContext) {
    copy_to(cx, true);
}

#[test]
fn plugin_place_paste_copy_to_carries_validated_paths_across_owners() {
    use crate::g2::op1::model::Operations;
    use flea::jsondoc;
    let cat = crate::test_catalogs::english();
    for source in [
        "/sandbox/notes.txt",
        "plugin://example.virtual-place/notes.txt",
    ] {
        let mut ops = Operations::new("default");
        ops.copy_to(71, false, PHOTOS);
        ops.key(&cat, "enter", false);
        assert_eq!(ops.outbox.len(), 1, "plugin destination must validate");
        let reply = format!(
            r#"{{"t":"menuaction","op":"validate","id":71,"ok":true,"paths":["{source}"],"dest":"{PHOTOS}"}}"#
        );
        ops.line(&cat, &reply, 0);
        let question = jsondoc::parse(&ops.outbox[1]).unwrap();
        assert_eq!(
            question.get("paths").unwrap(),
            &Json::Arr(vec![string(source)])
        );
        assert!(
            question.get("menuId").is_none(),
            "the destination never owns the source menu"
        );
    }
}

#[test]
fn plugin_place_paste_wire_trace_resolves_rows_before_transfer() {
    use flea::ui_logic_dg1::{self as clip, Clip, Out, PendingPaste, Stamp};
    let mut model = Model {
        path: ROOT.into(),
        ..Default::default()
    };
    model
        .accept(
            &flea::jsondoc::parse(
                r#"{"op":"list","entries":[{"name":"notes.txt","kind":"file"}]}"#,
            )
            .unwrap(),
            &flea::jsondoc::parse("{}").unwrap(),
        )
        .unwrap();
    let read = flea::jsondoc::parse(r#"{"c":"paths","rows":[0]}"#).unwrap();
    let paths = match model.command(&read) {
        Ok(Action::Replies(rows)) => rows[0]
            .get("paths")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap().into())
            .collect(),
        Err(error) => {
            println!("TRACE paths refusal: {error}");
            Vec::new()
        }
        _ => panic!("paths is a read"),
    };
    let current = Clip {
        paths,
        ..Default::default()
    };
    let effects = clip::plan(
        &current,
        &crate::test_catalogs::english(),
        &PendingPaste::transfer(PHOTOS),
        PHOTOS,
        false,
        false,
        Stamp {
            year: 2026,
            month: 11,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
        },
    );
    let wire = match &effects[0] {
        Out::Send(line) => line.clone(),
        Out::Ask(ask) => crate::g2::dg1::join::request(ask).unwrap().0.json(),
        other => panic!("unexpected effect: {other:?}"),
    };
    println!("TRACE paste wire: {wire}");
    let command = flea::jsondoc::parse(&wire).unwrap();
    let result = model.command(&command);
    if let Err(error) = &result {
        println!("TRACE paste refusal: {error}");
    }
    assert!(
        matches!(result, Ok(Action::Runtime { .. })),
        "file paste must reach the place protocol"
    );
}
