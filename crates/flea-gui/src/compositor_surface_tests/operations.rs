//! All operation-card owners mount with the compositor update and retained-palette path.
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
use std::sync::Arc;

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
        let (shell, view, cx) = mounted(cx, 8., |shell, window, cx| {
            let theme = super::theme();
            let palette =
                op1::palette::CardPalette::from_theme(&window.text_style(), theme, shell.corner);
            let view = cx.new(|_| op1::host::Overlay::new(Arc::default(), palette, "default"));
            shell.overlay = Some(view.clone());
            view
        });
        cx.simulate_resize(size(px(1100.), px(800.)));
        view.update(cx, |view, cx| {
            view.model.dialog = Some(dialog);
            cx.notify();
        });
        frames(name, &shell, cx, 8., &[]);
    }
}

struct Archive(op2::cards::Card, Entity<Shell>);
impl Render for Archive {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use op2::cards::DialogShell;
        let palette = crate::list::Palette::inherited(&window.text_style(), super::theme());
        op2::draw::StandIn.present(
            &self.0,
            &palette,
            &crate::list::Typeface::omarchy_default(),
            1100.,
            self.1.read(cx).corner,
        )
    }
}

fn archives(cx: &mut TestAppContext) {
    let catalog = crate::test_catalogs::english();
    let (compress, _) = op2::compress::Compress::open(
        vec!["/files/a".into()],
        "/files",
        "files",
        1,
        &["7z", "zip"],
    );
    let mut extract = op2::extract::Extract::closed();
    extract.open("/files/archive.7z", "/files", "", &catalog);
    for (name, card) in [
        ("compress-password", compress.card(&catalog)),
        ("extract-password", extract.card(&catalog)),
    ] {
        let (shell, _, cx) = mounted(cx, 8., |_, _, cx| {
            let shell = cx.entity();
            cx.new(|_| Archive(card, shell))
        });
        frames(name, &shell, cx, 8., &[18.]);
    }
}

fn outputs(cx: &mut TestAppContext) {
    for (name, action) in [
        ("print", op4::model::Action::Print),
        ("convert", op4::model::Action::Convert),
        ("combine", op4::model::Action::Combine),
    ] {
        let (shell, view, cx) = mounted(cx, 8., |shell, window, cx| {
            let palette = op1::palette::CardPalette::from_theme(
                &window.text_style(),
                super::theme(),
                shell.corner,
            );
            let view = cx.new(|cx| op4::host::OutputView::new(Arc::default(), palette, cx));
            shell.output.view = Some(view.clone());
            view
        });
        cx.simulate_resize(size(px(1100.), px(900.)));
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
        frames(name, &shell, cx, 8., &[18.]);
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
