//! Every operation-card owner mounts as production does: op1 and op4 retain a palette, the archive layer reads one each render.
use super::{frames, mounted};
use crate::{
    g2::{archive_flash, file_ops, print_convert},
    state::Shell,
};
use flea::{
    ui_logic_file_ops::{collide, trash},
    ui_logic_print_convert::{
        output::Kind,
        selection::{Selection, Source},
    },
};
use gpui::{prelude::*, px, size, AppContext, Context, Entity, Render, TestAppContext, Window};

// Each card mounts rounded, and frames then checks the live update to square and back.
const ROUNDED: f32 = 8.;
const CARD_WINDOW: (f32, f32) = (1100., 800.);
// The output cards cap their scrolling body by the window height.
const OUTPUT_WINDOW: (f32, f32) = (1100., 900.);

/// Mounts each op1 dialog over the production overlay host and frames it at the rounded corner
fn operations(cx: &mut TestAppContext) {
    use file_ops::model::{CopyFocus, Dialog};
    let cards = [
        (
            "new-file",
            Dialog::NewFile {
                id: 1,
                folder: "/files".into(),
                name: "notes.txt".into(),
                suggested: String::new(),
                focus: CopyFocus::Field,
                busy: false,
                error: String::new(),
            },
        ),
        (
            "move-to",
            Dialog::CopyTo {
                menu: 1,
                moving: true,
                path: "/files".into(),
                focus: CopyFocus::Field,
                busy: false,
                error: String::new(),
            },
        ),
        (
            "collision",
            Dialog::Collide {
                title: "File exists".into(),
                names: vec![flea::ui_logic_file_ops::Collision {
                    name: "notes.txt".into(),
                    dir: false,
                    icon: "text-plain".into(),
                }],
                more: String::new(),
                merge: false,
                focus: collide::Choice::Keep,
            },
        ),
        (
            "fat-names",
            Dialog::Fat {
                title: "Names on this drive".into(),
                renames: vec![flea::ui_logic_file_ops::Rename {
                    old: "a:b".into(),
                    new: "a_b".into(),
                    dir: false,
                    icon: "file".into(),
                }],
                note: String::new(),
                focus: collide::Fat::Rename,
            },
        ),
        (
            "delete-permanently",
            Dialog::Delete {
                session: 1,
                review: trash::Review {
                    count: Some(2),
                    bytes: 1024,
                    all: false,
                    token: "1".into(),
                    stale: false,
                },
                scope: trash::Scope::Items,
                focus: trash::Button::Cancel,
                busy: false,
            },
        ),
        (
            "empty-trash",
            Dialog::Delete {
                session: 1,
                review: trash::Review {
                    count: Some(2),
                    bytes: 1024,
                    all: true,
                    token: "1".into(),
                    stale: false,
                },
                scope: trash::Scope::TrashItems,
                focus: trash::Button::Cancel,
                busy: false,
            },
        ),
    ];
    for (name, dialog) in cards {
        let (shell, view, cx) = mounted(cx, ROUNDED, |shell, window, cx| {
            let theme = super::theme();
            let palette = file_ops::palette::CardPalette::from_theme(
                &window.text_style(),
                theme,
                shell.corner,
            );
            let catalog = shell.locale.catalog.clone();
            let view = cx.new(|_| file_ops::host::Overlay::new(catalog, palette, "default"));
            shell.overlay = Some(view.clone());
            view
        });
        cx.simulate_resize(size(px(CARD_WINDOW.0), px(CARD_WINDOW.1)));
        view.update(cx, |view, cx| {
            view.model.dialog = Some(dialog);
            cx.notify();
        });
        frames(name, &shell, cx, ROUNDED, &[]);
    }
}

// The production owner builds the card, palette, room and corner, as the root view asks it to.
struct ArchiveLayer(Entity<Shell>, &'static str);
impl Render for ArchiveLayer {
    /// Draws the shell's archive card, panicking when the shell opened none
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let name = self.1;
        self.0
            .update(cx, |shell, cx| shell.archive_layer(window, cx))
            .unwrap_or_else(|| panic!("{name} must open its archive card"))
    }
}

/// Opens the card through `open`, mounts it as production draws it and frames it
fn archive_card(cx: &mut TestAppContext, name: &'static str, open: impl FnOnce(&mut Shell)) {
    let (shell, _, cx) = mounted(cx, ROUNDED, |shell, _, cx| {
        open(shell);
        let shell = cx.entity();
        cx.new(|_| ArchiveLayer(shell, name))
    });
    cx.simulate_resize(size(px(CARD_WINDOW.0), px(CARD_WINDOW.1)));
    frames(name, &shell, cx, ROUNDED, &[]);
}

/// Frames the compress and extract password cards
fn archives(cx: &mut TestAppContext) {
    archive_card(cx, "compress-password", |shell| {
        let (compress, _) = archive_flash::compress::Compress::open(
            vec!["/files/a".into()],
            "/files",
            "files",
            1,
            &["7z", "zip"],
        );
        shell.operations.archive.compress = Some(compress);
    });
    archive_card(cx, "extract-password", |shell| {
        let catalog = shell.locale.catalog.clone();
        let archive = &mut shell.operations.archive;
        archive
            .extract
            .open("/files/archive.7z", "/files", "", &catalog);
    });
}

/// Opens each op4 output action on a two-image selection and frames the card
fn outputs(cx: &mut TestAppContext) {
    for (name, action) in [
        ("print", print_convert::model::Action::Print),
        ("convert", print_convert::model::Action::Convert),
        ("combine", print_convert::model::Action::Combine),
    ] {
        let (shell, view, cx) = mounted(cx, ROUNDED, |shell, window, cx| {
            let palette = file_ops::palette::CardPalette::from_theme(
                &window.text_style(),
                super::theme(),
                shell.corner,
            );
            let catalog = shell.locale.catalog.clone();
            let view = cx.new(|cx| print_convert::host::OutputView::new(catalog, palette, cx));
            shell.output.view = Some(view.clone());
            view
        });
        cx.simulate_resize(size(px(OUTPUT_WINDOW.0), px(OUTPUT_WINDOW.1)));
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                let selection = Selection {
                    generation: 1,
                    menu: 1,
                    files: ["a.png", "b.png"]
                        .into_iter()
                        .map(|name| Source {
                            path: format!("/files/{name}"),
                            kind: Kind::Image,
                        })
                        .collect(),
                };
                assert!(view.open(action, selection, window, cx));
            })
        });
        frames(name, &shell, cx, ROUNDED, &[]);
    }
}

#[gpui::test]
/// Frames every operation dialog card
fn vf_frames_operation_cards(cx: &mut TestAppContext) {
    operations(cx);
}
#[gpui::test]
/// Frames the archive password cards
fn vf_frames_archive_cards(cx: &mut TestAppContext) {
    archives(cx);
}
#[gpui::test]
/// Frames the print, convert and combine output cards
fn vf_frames_output_cards(cx: &mut TestAppContext) {
    outputs(cx);
}
