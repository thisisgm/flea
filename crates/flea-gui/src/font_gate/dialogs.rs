use super::*;
use crate::g2::mn1;
use flea::ui_logic_op1::{
    confirm::Source,
    trash::{Review, Scope},
};

pub(super) fn check() -> Vec<String> {
    let mut failed = Vec::new();
    surfaces::case("bulk-rename", &mut failed, |shell, window, cx| {
        shell.rename.paths = Some(0);
        shell.receive(
            0,
            crate::backend::Event::Paths(vec![
                "/files/first.txt".into(),
                "/files/second.txt".into(),
            ]),
            window,
            cx,
        );
        assert!(shell.rename.bulk.is_some());
    });
    surfaces::case("unlock", &mut failed, |shell, window, cx| {
        shell.mount_rail(cx);
        shell.open_unlock(
            0,
            "/dev/font-gate",
            "Font gate encrypted drive",
            0,
            "crypto_LUKS",
            window,
            cx,
        );
        assert!(shell.rail.as_ref().unwrap().unlock.is_some());
    });
    surfaces::case("properties", &mut failed, |shell, window, cx| {
        assert!(shell.properties_listing(window, cx));
    });
    surfaces::case("run-file", &mut failed, |shell, _, cx| {
        shell.open_run_card(0, "/files/run.sh".into(), r#"{"ask":"run","kind":"script","size":21,"handlerId":"","handlerName":"","dev":"8","ino":"9","mtimeS":1,"mtimeNs":2,"target":"/files/run.sh"}"#.into(), false, cx);
        assert!(shell.run_card.is_some());
    });
    for empty in [false, true] {
        surfaces::case(
            if empty {
                "confirm-empty-trash"
            } else {
                "confirm-delete"
            },
            &mut failed,
            |shell, window, cx| {
                shell.open_overlay_copy_to(false, "/files", window, cx);
                shell.overlay.as_ref().unwrap().update(cx, |view, cx| {
                    view.confirm_delete(
                        Review {
                            count: Some(2),
                            token: "5".into(),
                            all: empty,
                            ..Default::default()
                        },
                        if empty {
                            Scope::TrashItems
                        } else {
                            Scope::Items
                        },
                        Source::Menu(5),
                        cx,
                    );
                });
            },
        );
    }
    surfaces::case("new-file", &mut failed, |shell, window, cx| {
        shell.open_overlay_copy_to(false, "/files", window, cx);
        shell
            .overlay
            .as_ref()
            .unwrap()
            .update(cx, |view, cx| view.new_file(1, "/files", cx));
    });
    for fat in [false, true] {
        surfaces::case(
            if fat {
                "fat-name-confirm"
            } else {
                "collision-confirm"
            },
            &mut failed,
            |shell, window, cx| {
                shell.open_overlay_copy_to(false, "/files", window, cx);
                shell.overlay.as_ref().unwrap().update(cx, |view, cx| {
                    view.model.dialog = Some(if fat {
                        crate::g2::op1::model::Dialog::Fat {
                            title: "Rename incompatible names".into(),
                            renames: vec![flea::ui_logic_op1::Rename {
                                old: "bad:name.txt".into(),
                                new: "bad_name.txt".into(),
                                dir: false,
                                icon: "file".into(),
                            }],
                            note: "Font gate FAT names".into(),
                            focus: flea::ui_logic_op1::collide::Fat::Rename,
                        }
                    } else {
                        crate::g2::op1::model::Dialog::Collide {
                            title: "Replace existing files".into(),
                            names: vec![flea::ui_logic_op1::Collision {
                                name: "existing.txt".into(),
                                dir: false,
                                icon: "file".into(),
                            }],
                            more: String::new(),
                            merge: false,
                            focus: flea::ui_logic_op1::collide::Choice::Replace,
                        }
                    });
                    cx.notify();
                });
            },
        );
    }
    surfaces::case("compress-password", &mut failed, |shell, _, _| {
        shell.operations.archive.compress = Some(
            crate::g2::op2::compress::Compress::open(
                vec!["/files/file-0.txt".into()],
                "/files",
                "files",
                1,
                &["7z", "zip"],
            )
            .0,
        );
    });
    surfaces::case("extract-password", &mut failed, |shell, _, _| {
        shell.operations.archive.extract.open(
            "/files/archive.7z",
            "/files",
            "",
            &shell.locale.catalog,
        );
    });
    surfaces::case("flash", &mut failed, |shell, _, _| {
        shell.operations.archive.flash.open();
    });
    surfaces::case("open-with", &mut failed, |shell, _, cx| {
        shell.open_open_with(0, "/files/file-0.txt".into(), cx);
        assert!(shell.open_with.is_some());
    });
    surfaces::case("permissions", &mut failed, |shell, _, cx| {
        let model = crate::g2::op3::tests::permissions();
        let catalog = shell.locale.catalog.clone();
        shell.permissions.view =
            Some(cx.new(|cx| crate::g2::op3::permissions::host::View::new(model, catalog, 0., cx)));
    });
    for action in [
        crate::g2::op4::model::Action::Print,
        crate::g2::op4::model::Action::Convert,
        crate::g2::op4::model::Action::Combine,
    ] {
        surfaces::case(
            &format!("output-{action:?}"),
            &mut failed,
            |shell, window, cx| {
                let files = ["image-a.png", "image-b.png"]
                    .map(|name| flea::ui_logic_op4::selection::Source {
                        path: format!("/files/{name}"),
                        kind: flea::ui_logic_op4::output::Kind::Image,
                    })
                    .into();
                assert!(shell.open_output(0, action, 1, files, window, cx));
            },
        );
    }
    surfaces::case("add-network-share", &mut failed, |shell, window, cx| {
        shell.mount_rail(cx);
        shell.network_add(0, window, cx)
    });
    surfaces::case(
        "context-menu-and-submenu",
        &mut failed,
        |shell, window, _| {
            // The Env refresh_menu builds; context-menu-background covers the real opener.
            let face = shell.panes[0]
                .as_ref()
                .unwrap()
                .list
                .borrow()
                .typeface
                .clone();
            let env = shell.menu_env(&face);
            let entries = vec![flea::ui_logic_mn1::Entry {
                submenu: Some(vec![flea::ui_logic_mn1::Entry::leaf(
                    "copy",
                    flea::ui_logic_mn1::Text::Data("Font submenu".into()),
                )]),
                ..flea::ui_logic_mn1::Entry::row(
                    "copyAs",
                    flea::ui_logic_mn1::Text::Data("Font menu".into()),
                    "file",
                )
            }];
            shell.menu.menu.borrow_mut().open(
                mn1::menu::Face::File,
                entries,
                (100., 100.),
                flea::ui_logic_mn1::fit::Rect {
                    x: 0.,
                    y: 0.,
                    w: 1100.,
                    h: 700.,
                },
                "file",
                &env,
                window,
            );
            shell
                .menu
                .menu
                .borrow_mut()
                .open_submenu_for("copyAs", true, &env, window);
            assert!(shell.menu.menu.borrow().flyout.is_some());
        },
    );
    surfaces::case(
        "context-menu-background",
        &mut failed,
        |shell, window, cx| {
            shell.open_menu(0, None, (100., 100.), window, cx);
            assert!(
                shell.menu.menu.borrow().opened(),
                "the pane opened its menu"
            );
        },
    );
    surfaces::case("find", &mut failed, |shell, window, cx| {
        assert!(shell.search_action("search", window, cx));
        shell.search_update(
            0,
            None,
            |search, host| {
                for ch in "Font gate query".chars() {
                    search.key(crate::g2::sr1::StripKey::Text(ch.to_string()), host);
                }
            },
            window,
            cx,
        );
    });
    surfaces::case("explore-filter", &mut failed, |shell, window, cx| {
        assert!(shell.filter_action("filter", window, cx));
    });
    failed
}
