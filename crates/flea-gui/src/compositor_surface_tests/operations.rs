//! Every operation-card owner mounts as production does: op1 and op4 retain a palette, the archive layer reads one each render.
use super::{frames, mounted};
use crate::{
    g2::{op1, op2, op4},
    state::Shell,
};
use flea::{
    ui_logic_op1::{collide, trash},
    ui_logic_op4::{
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

fn operations(cx: &mut TestAppContext) {
    use op1::model::{CopyFocus, Dialog};
    let cards = [
        (
            "new-file",
            Dialog::NewFile {
                id: 1,
                folder: "/files".into(),
                name: "notes.txt".into(),
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
                names: vec![flea::ui_logic_op1::Collision {
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
                renames: vec![flea::ui_logic_op1::Rename {
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
            let palette =
                op1::palette::CardPalette::from_theme(&window.text_style(), theme, shell.corner);
            let catalog = shell.locale.catalog.clone();
            let view = cx.new(|_| op1::host::Overlay::new(catalog, palette, "default"));
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let name = self.1;
        self.0
            .update(cx, |shell, cx| shell.archive_layer(window, cx))
            .unwrap_or_else(|| panic!("{name} must open its archive card"))
    }
}

fn archive_card(cx: &mut TestAppContext, name: &'static str, open: impl FnOnce(&mut Shell)) {
    let (shell, _, cx) = mounted(cx, ROUNDED, |shell, _, cx| {
        open(shell);
        let shell = cx.entity();
        cx.new(|_| ArchiveLayer(shell, name))
    });
    cx.simulate_resize(size(px(CARD_WINDOW.0), px(CARD_WINDOW.1)));
    frames(name, &shell, cx, ROUNDED, &[]);
}

fn archives(cx: &mut TestAppContext) {
    archive_card(cx, "compress-password", |shell| {
        let (compress, _) = op2::compress::Compress::open(
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

fn outputs(cx: &mut TestAppContext) {
    for (name, action) in [
        ("print", op4::model::Action::Print),
        ("convert", op4::model::Action::Convert),
        ("combine", op4::model::Action::Combine),
    ] {
        let (shell, view, cx) = mounted(cx, ROUNDED, |shell, window, cx| {
            let palette = op1::palette::CardPalette::from_theme(
                &window.text_style(),
                super::theme(),
                shell.corner,
            );
            let catalog = shell.locale.catalog.clone();
            let view = cx.new(|cx| op4::host::OutputView::new(catalog, palette, cx));
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
fn vf_frames_operation_cards(cx: &mut TestAppContext) {
    operations(cx);
}
#[gpui::test]
fn vf_frames_archive_cards(cx: &mut TestAppContext) {
    archives(cx);
}
#[gpui::test]
fn vf_frames_output_cards(cx: &mut TestAppContext) {
    outputs(cx);
}
