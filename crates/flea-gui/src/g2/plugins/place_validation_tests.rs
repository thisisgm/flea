use super::{
    place_model::*,
    place_ops::{Action, Completion},
};
use flea::jsondoc::{self, Json};
fn doc(line: &str) -> Json {
    jsondoc::parse(line).unwrap()
}
fn model() -> Model {
    let mut model = Model {
        path: "plugin://example.virtual-place/".into(),
        ..Model::default()
    };
    model.accept(&doc(r#"{"op":"list","entries":[{"name":"Photos","kind":"dir","size":0},{"name":"notes.txt","kind":"file","size":99}]}"#),&doc(r#"{"first":350,"foldersFirst":true}"#)).unwrap();
    model
}
#[test]
fn plugin_runtime_place_rejects_traversal_controls_duplicate_names_and_bad_metadata() {
    for uri in [
        "plugin://example.virtual-place/../x",
        "plugin://example.virtual-place/./x",
        "plugin://example.virtual-place/hi\n",
        "plugin://invalid/",
    ] {
        assert!(path(uri).is_none(), "{uri:?}")
    }
    for entries in [
        r#"[{"name":"../x","kind":"file"}]"#,
        r#"[{"name":"x","kind":"exec"}]"#,
        r#"[{"name":"x","kind":"file","size":-1}]"#,
        r#"[{"name":"x\u202e","kind":"file"}]"#,
        r#"[{"name":"x","kind":"file"},{"name":"x","kind":"file"}]"#,
    ] {
        let answer = doc(&format!(r#"{{"op":"list","entries":{entries}}}"#));
        assert!(model().accept(&answer, &doc("{}")).is_err(), "{entries}");
    }
}
#[test]
fn plugin_runtime_place_routes_copy_move_and_delete_only_the_captured_paths() {
    let mut model = model();
    for op in ["copy", "move"] {
        let command = doc(&format!(
            r#"{{"c":"transfer","op":"{op}","paths":["plugin://example.virtual-place/notes.txt"],"dest":"/sandbox/destination","id":5}}"#
        ));
        let Action::Runtime {
            plugin, request, ..
        } = model.command(&command).unwrap()
        else {
            panic!("runtime route required")
        };
        assert_eq!(plugin, "example.virtual-place");
        assert_eq!(text(&request, "op"), op);
        assert_eq!(text(&request, "dest"), "/sandbox/destination");
        assert_eq!(request.get("paths").unwrap(), &doc(r#"["/notes.txt"]"#));
        assert_eq!(request.get("overwrite"), Some(&Json::Bool(false)));
    }
    let listing = model.listing;
    model
        .command(&doc(&format!(
            r#"{{"c":"menuaction","op":"snapshot","id":17,"listing":{listing},"rows":[1]}}"#
        )))
        .unwrap();
    assert!(model
        .command(&doc(
            r#"{"c":"menuaction","op":"delete","id":17,"token":17}"#
        ))
        .is_err());
    model
        .command(&doc(r#"{"c":"menuaction","op":"prepareDelete","id":17}"#))
        .unwrap();
    let Action::Runtime {
        request,
        completion,
        ..
    } = model
        .command(&doc(
            r#"{"c":"menuaction","op":"delete","id":17,"token":17}"#,
        ))
        .unwrap()
    else {
        panic!("delete runtime route required")
    };
    assert_eq!(request.get("paths").unwrap(), &doc(r#"["/notes.txt"]"#));
    assert!(matches!(completion, Completion::Delete(17, 1)));
    assert!(model
        .command(&doc(
            r#"{"c":"menuaction","op":"delete","id":17,"token":17}"#
        ))
        .is_err());
}
#[test]
fn plugin_runtime_place_stale_rows_cross_plugin_transfer_and_overwrite_are_refused() {
    let mut model = model();
    let stale = doc(r#"{"c":"paths","listing":999,"rows":[0]}"#);
    assert!(model.command(&stale).is_err());
    assert!(model.command(&doc(r#"{"c":"transfer","op":"move","paths":["plugin://example.virtual-place/notes.txt"],"dest":"plugin://other.owner/"}"#)).is_err());
    assert!(model
        .command(&doc(
            r#"{"c":"rename","path":"plugin://example.virtual-place/notes.txt","to":"Photos"}"#
        ))
        .is_err());
    assert!(model
        .command(&doc(
            r#"{"c":"rename","path":"plugin://example.virtual-place/notes.txt","to":"../outside"}"#
        ))
        .is_err());
    let listing = model.listing;
    model
        .command(&doc(&format!(
            r#"{{"c":"menuaction","op":"snapshot","id":3,"listing":{listing},"rows":[0]}}"#
        )))
        .unwrap();
    model.publish(&doc("{}")).unwrap();
    assert!(model
        .command(&doc(r#"{"c":"menuaction","op":"prepareDelete","id":3}"#))
        .is_err());
}
#[test]
fn plugin_runtime_place_delete_names_only_its_own_items_and_move_to_says_moving() {
    let mut model = model();
    model
        .command(&doc(
            r#"{"c":"menuaction","op":"snapshot","id":4,"paths":["plugin://other.owner/notes.txt"]}"#,
        ))
        .unwrap();
    model
        .command(&doc(r#"{"c":"menuaction","op":"prepareDelete","id":4}"#))
        .unwrap();
    assert!(
        model
            .command(&doc(r#"{"c":"menuaction","op":"delete","id":4,"token":4}"#))
            .is_err(),
        "another place's item is never deleted here"
    );
    for (c, op, moving) in [
        ("menuaction", "copyTo", false),
        ("menuaction", "moveTo", true),
        ("transfer", "move", true),
    ] {
        let command = doc(&format!(
            r#"{{"c":"{c}","op":"{op}","paths":["plugin://example.virtual-place/notes.txt"],"dest":"/sandbox/destination","id":6}}"#
        ));
        let Action::Runtime { completion, .. } = model.command(&command).unwrap() else {
            panic!("transfer runtime route required")
        };
        let replies = model.complete(&command, &doc(r#"{"op":"done"}"#), completion);
        assert_eq!(replies[0].get("moving"), Some(&Json::Bool(moving)), "{op}");
    }
}
#[test]
fn plugin_runtime_place_literal_text_window_bound_and_sort_use_the_normal_wire() {
    let mut model = model();
    model
        .entries
        .extend((0..240).map(|i| flea::protocol::RowFacts {
            name: format!("{i:03}<b>.txt"),
            ..Default::default()
        }));
    let list = model.publish(&doc(r#"{"first":5}"#)).unwrap();
    assert_eq!(list[1].get("rows").unwrap().as_array().unwrap().len(), 5);
    let window = model.window(20, 7);
    assert_eq!(window.get("rows").unwrap().as_array().unwrap().len(), 7);
    assert_eq!(num(&window, "start"), 20);
    model.entries.extend(
        (0..super::limits::WINDOW_ROWS).map(|i| flea::protocol::RowFacts {
            name: format!("more {i:04}.txt"),
            ..Default::default()
        }),
    );
    model.publish(&doc("{}")).unwrap();
    let whole = model.window(0, super::limits::WINDOW_ROWS * 2);
    assert_eq!(
        whole.get("rows").unwrap().as_array().unwrap().len(),
        super::limits::WINDOW_ROWS,
        "one window answers at most its bound"
    );
    let Action::Replies(sorted) = model
        .command(&doc(
            r#"{"c":"sort","by":"name","desc":true,"first":4,"foldersFirst":false}"#,
        ))
        .unwrap()
    else {
        panic!("sort replies")
    };
    assert!(matches!(
        flea::protocol::parse_reply(&flea::ui_logic_plugin_settings::wire::compact(&sorted[1]))
            .unwrap(),
        flea::protocol::Reply::Rows { .. }
    ));
    assert_eq!(model.rows[0].name, "Photos");
    assert!(model.rows.iter().any(|r| r.name.contains("<b>")));
}
