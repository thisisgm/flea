    use super::*;
    #[test]
    fn counted_lines_read_singular_for_one() {
        let (mut wire, reader) = crate::tui::wire::echo_wire();
        let mut model = Model::new(PathBuf::from("/listing"), &Json::Null);
        model.pending_clipboard = true;
        model.receive(crate::jsondoc::parse(r#"{"t":"paths","paths":["/listing/a"]}"#).unwrap(), &mut wire).unwrap();
        assert_eq!(model.message, "Copied 1 item");
        model.search = "Search: stale".into();
        model.receive(crate::jsondoc::parse(r#"{"t":"searched","n":1,"scanned":5}"#).unwrap(), &mut wire).unwrap();
        assert_eq!(model.search, "1 found");
        model.total = 1;
        crate::tui::prompts::trash(&mut model, &mut wire).unwrap();
        model.receive(crate::jsondoc::parse(r#"{"t":"trashed","id":1,"ok":1,"failed":0,"moved":["/listing/a"]}"#).unwrap(), &mut wire).unwrap();
        assert_eq!(model.message, "Moved 1 item to Trash", "no undo key in the hints, so none is named");
        model.transfer_id = 7;
        model.receive(crate::jsondoc::parse(r#"{"t":"transferdone","id":7,"ok":1,"failed":0,"skipped":0,"cancelled":false,"retryPaths":[],"durable":false,"note":""}"#).unwrap(), &mut wire).unwrap();
        assert_eq!(model.message, "Copied 1 item");
        model.menu_action = "deleteRunning".into();
        model.menu_count = 1;
        model.receive(crate::jsondoc::parse(r#"{"t":"menuaction","op":"delete","id":0,"ok":true,"deleted":1,"failed":0,"remaining":[]}"#).unwrap(), &mut wire).unwrap();
        assert_eq!(model.message, "Deleted 1 of 1 item");
        assert_eq!(bulk_done_line(1, false), "Renamed 1 item · Undo reverts one");
        crate::tui::wire::finish(wire, reader);
    }
    #[test]
    fn counted_lines_read_plural_for_many() {
        let (mut wire, reader) = crate::tui::wire::echo_wire();
        let mut model = Model::new(PathBuf::from("/listing"), &Json::Null);
        model.pending_clipboard = true;
        model.cut = true;
        model.receive(crate::jsondoc::parse(r#"{"t":"paths","paths":["/listing/a","/listing/b"]}"#).unwrap(), &mut wire).unwrap();
        assert_eq!(model.message, "Cut 2 items");
        model.search = "Search: stale".into();
        model.receive(crate::jsondoc::parse(r#"{"t":"searched","n":2000,"scanned":3000}"#).unwrap(), &mut wire).unwrap();
        assert_eq!(model.search, "2,000 found");
        model.total = 3;
        crate::tui::prompts::trash(&mut model, &mut wire).unwrap();
        model.receive(crate::jsondoc::parse(r#"{"t":"trashed","id":1,"ok":3,"failed":0,"moved":["/listing/a","/listing/b","/listing/c"]}"#).unwrap(), &mut wire).unwrap();
        assert_eq!(model.message, "Moved 3 items to Trash");
        model.transfer_id = 7;
        model.receive(crate::jsondoc::parse(r#"{"t":"transferdone","id":7,"ok":1204,"failed":0,"skipped":0,"cancelled":false,"retryPaths":[],"durable":false,"note":""}"#).unwrap(), &mut wire).unwrap();
        assert_eq!(model.message, "Copied 1,204 items");
        model.menu_action = "deleteRunning".into();
        model.menu_count = 3;
        model.receive(crate::jsondoc::parse(r#"{"t":"menuaction","op":"delete","id":0,"ok":true,"deleted":2,"failed":0,"remaining":[]}"#).unwrap(), &mut wire).unwrap();
        assert_eq!(model.message, "Deleted 2 of 3 items");
        assert_eq!(bulk_done_line(3, true), "Stopped after renaming 3 items · Undo reverts one");
        crate::tui::wire::finish(wire, reader);
    }
    #[test]
    fn listing_invalidation_cancels_pending_taildrop_intent() {
        let mut model = Model::new(PathBuf::from("/"), &Json::Null);
        model.taildrop_target = Some(super::super::taildrop::Peer { id: "node".into(), label: "peer".into(), address: "peer.invalid".into() });
        model.menu_action = "taildropRefresh".into();
        model.message = "Checking Taildrop".into();
        model.invalidate_rows();
        assert!(model.taildrop_target.is_none());
        assert!(model.menu_action.is_empty());
        assert!(model.message.is_empty());
        assert_eq!(model.error, "Taildrop cancelled; selection changed");
        assert!(!model.cancel_taildrop(), "late invalidation must not create another error");
    }


    #[test]
    fn pdf_preview_uses_filename_not_generic_icon_or_localized_kind() {
        let mut row = Row::parse(&Json::Obj(vec![
            ("n".into(), word("pages.PDF")),
            ("i".into(), word("x-office-document")),
        ]), &[word("Document")]);
        assert!(row.is_pdf());
        row.directory = true;
        assert!(!row.is_pdf());
        row.directory = false;
        row.link = "original.pdf".into();
        assert!(!row.is_pdf());
        row.link.clear();
        for name in [".pdf", "pages.pdf.txt", "report.doc", "no-extension"] {
            row.name = name.into();
            assert!(!row.is_pdf(), "{name}");
        }
    }

    #[test]
    fn survivor_restore_requires_current_reply_and_original_membership() {
        let mut model = Model::new(PathBuf::from("/listing"), &Json::Null);
        model.menu_action = "deleteRestore".into();
        model.restore_id = 7;
        model.total = 100;
        model.restore_selection = [PathBuf::from("/listing/survivor"), PathBuf::from("/listing/replaced")].into_iter().collect();
        let reply = |id, directory| Json::Obj(vec![("id".into(), number(id)), ("directory".into(), word(directory)),
            ("ok".into(), Json::Bool(true)), ("matches".into(), Json::Arr(vec![
                Json::Obj(vec![("path".into(), word("/listing/survivor")), ("index".into(), number(75))]),
                Json::Obj(vec![("path".into(), word("/listing/unselected")), ("index".into(), number(3))])]))]);
        assert!(!model.restore_survivors(&reply(6, "/listing")));
        assert!(!model.restore_survivors(&reply(7, "/other")));
        model.pending = Some(PathBuf::from("/listing"));
        assert!(!model.restore_survivors(&reply(7, "/listing")));
        model.pending = None;
        assert!(model.restore_survivors(&reply(7, "/listing")));
        assert_eq!(model.selected, [75].into_iter().collect());
        assert_eq!(model.cursor, 75);
        assert!(model.selected_rows.is_empty(), "offscreen survivors remain selected without invented metadata");
        assert!(model.restore_selection.is_empty());
        assert!(model.menu_action.is_empty());
    }

    #[test]
    fn one_mark_keeps_its_identity_when_cursor_leaves_the_page() {
        let mut model = Model::new(PathBuf::from("/"), &Json::Null);
        let row = |name| Row::parse(&Json::Obj(vec![("n".into(), word(name))]), &[]);
        model.rows.insert(3, row("marked"));
        model.selected.insert(3);
        model.remember_selection();
        model.rows.clear();
        model.rows.insert(8, row("cursor"));
        model.cursor = 8;
        let (index, selected) = model.single_row().unwrap();
        assert_eq!(index, 3);
        assert_eq!(selected.name, "marked");
        model.selected.insert(8);
        assert!(model.single_row().is_none());
        model.selected.clear();
        assert_eq!(model.single_row().unwrap().1.name, "cursor");
    }

    #[test]
    fn errors_survive_until_each_is_acknowledged() {
        let mut model = Model::new(PathBuf::from("/"), &Json::Null);
        model.fail("Copy failed".into());
        model.fail("Preview failed".into());
        model.fail("Copy failed".into());
        model.fail("Preview failed".into());
        model.fail(String::new());
        assert_eq!(model.error, "Copy failed");
        assert_eq!(model.errors.len(), 1);
        model.dismiss_error();
        assert_eq!(model.error, "Preview failed");
        model.dismiss_error();
        assert!(model.error.is_empty());
    }

    #[test]
    fn menu_requires_a_ready_identity_snapshot() {
        let map = crate::tui::keymap::Map::load();
        let enabled = |model: &Model, id: &str| super::super::render::menu_layout::entries(model, &map).into_iter().flatten()
            .any(|row| row.id == id && row.enabled());
        let mut model = Model::new(PathBuf::from("/"), &Json::Null);
        model.menu_path = PathBuf::from("/selected");
        model.menu_count = 1;
        model.menu_view.taildrop = true;
        assert!(!enabled(&model, "open"));
        assert!(enabled(&model, "toggleHidden"));
        model.menu_ready = true;
        assert!(enabled(&model, "open"));
        let taildrop: Vec<_> = super::super::render::menu_layout::entries(&model, &map).into_iter().flatten().filter(|row| row.id == "taildrop").collect();
        assert_eq!(taildrop.len(), 1, "Taildrop with no machine is still a row");
        assert!(!taildrop[0].enabled(), "Taildrop with no machine cannot act");
        model.menu_path.clear();
        assert!(!enabled(&model, "open"));
    }
    #[test]
    fn a_transfer_that_partly_fails_keeps_its_failure_as_an_error() {
        let (mut wire, reader) = crate::tui::wire::echo_wire();
        let mut model = Model::new(PathBuf::from("/listing"), &Json::Null);
        model.transfer_id = 7;
        model.receive(crate::jsondoc::parse(r#"{"t":"transferdone","id":7,"ok":3,"failed":2,"skipped":0,"cancelled":false,"retryPaths":[],"durable":false,"note":""}"#).unwrap(), &mut wire).unwrap();
        assert!(model.error.contains('2'), "error {:?}", model.error);
        crate::tui::wire::finish(wire, reader);
    }
