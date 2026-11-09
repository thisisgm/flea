use super::{frames, mounted, painted};
use crate::g2::{device_rail, file_ops, open_with_properties::shell::*};
use gpui::{point, prelude::*, px, AppContext, Context, Render, TestAppContext, Window};
use std::{rc::Rc, sync::Arc};

struct Idle;
impl device_rail::process::Runner for Idle {
    fn start(&self, _: device_rail::process::Job) -> device_rail::process::Handle {
        device_rail::process::Handle::detached().0
    }
}

struct Operations(Card, gpui::Entity<crate::state::Shell>, f32);
impl Render for Operations {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = crate::list::Palette::inherited(&window.text_style(), super::theme());
        let face = crate::list::Typeface {
            body: px(self.2),
            ..crate::list::Typeface::omarchy_default()
        };
        StandIn.present(&self.0, &palette, &face, self.1.read(cx).corner, 1100.)
    }
}

fn controls() -> Card {
    let field = Field {
        input: None,
        label: None,
        value: "file".into(),
        placeholder: String::new(),
        glyph: None,
        focused: true,
    };
    Card {
        language: "en".into(),
        title: "Operations".into(),
        esc: "esc".into(),
        width: 640.,
        bleed: false,
        body: vec![
            Block::Fields(field.clone(), field),
            Block::CheckLine {
                check: Check {
                    value: Tri::On,
                    focused: true,
                },
                label: "Permission".into(),
            },
            Block::Names {
                old_label: "Old".into(),
                old: vec!["a".into()],
                new_label: "New".into(),
                new: vec![NameLine {
                    input: None,
                    text: "b".into(),
                    ink: Ink::Foreground,
                    reason: String::new(),
                }],
                focused: true,
                error: String::new(),
            },
        ],
        buttons: vec![Button {
            label: "Apply".into(),
            primary: true,
            focused: true,
            enabled: true,
        }],
    }
}

pub(super) fn check(cx: &mut TestAppContext, corner: f32) {
    let (shell, view, vcx) = mounted(cx, corner, |_, _, cx| {
        cx.new(|cx| device_rail::view::UnlockView::new(Arc::default(), Rc::new(Idle), cx))
    });
    vcx.update(|window, cx| {
        // The card grows with the text size (#310); this window has no Shell root, so it sits at the board's 14 px body.
        window.set_rem_size(px(14.));
        view.update(cx, |view, cx| {
            view.open_for("/dev/test", "Encrypted", 0, "crypto_LUKS", window, cx);
            let card = view.host.card.as_mut().unwrap();
            card.type_text("x");
            card.focus = Some(device_rail::unlock::Part::Unlock);
            cx.notify();
        })
    });
    frames("unlock-card-password-buttons", &shell, vcx, corner, &[]);
    let (scale, quads) = painted(vcx);
    let button = quads
        .iter()
        .filter(|quad| {
            quad.border_widths.top.0 > 0. && (quad.bounds.size.height.0 / scale - 30.).abs() < 0.1
        })
        .max_by(|a, b| {
            a.bounds
                .origin
                .y
                .0
                .total_cmp(&b.bounds.origin.y.0)
                .then(a.bounds.origin.x.0.total_cmp(&b.bounds.origin.x.0))
        })
        .unwrap();
    vcx.simulate_mouse_move(
        point(
            px(button.bounds.center().x.0 / scale),
            px(button.bounds.center().y.0 / scale),
        ),
        None,
        gpui::Modifiers::default(),
    );
    frames("unlock-pointer-wash", &shell, vcx, corner, &[]);
    let (_, quads) = painted(vcx);
    assert!(
        quads.iter().any(|quad| {
            let expected = (corner
                - if quad.bounds.size.height.0 / scale < 28. {
                    2.
                } else {
                    1.
                })
            .max(0.);
            quad.background
                .as_solid()
                .is_some_and(|color| (color.a - 0.08).abs() < 0.01)
                && (quad.corner_radii.top_left.0 / scale - expected).abs() < 0.01
        }),
        "unlock hover wash must inset and clamp the compositor radius"
    );
    let catalog = crate::g2::open_with_properties::tests::english();
    let (mut properties, _) =
        crate::g2::open_with_properties::properties::Properties::open(5, 1, &[2], 0);
    properties.reply(&catalog, &flea::jsondoc::parse(r#"{"op":"properties","id":5,"ok":true,"path":"/files/a","kind":"File","bytes":99,"modified":0,"mode":"0644","owner":"test","uid":1000,"gid":1000}"#).unwrap());
    let run = crate::g2::open_with_properties::runcard::RunCard::open(&catalog, "flea", "/files/run.sh", r#"{"ask":"run","kind":"script","size":21,"handlerId":"","handlerName":"","dev":"8","ino":"9","mtimeS":1,"mtimeNs":2,"target":"/files/run.sh"}"#).unwrap();
    let bulk = crate::g2::open_with_properties::bulkrename::BulkRename::open(vec![
        "/files/a".into(),
        "/files/b".into(),
    ]);
    let cards = [
        ("operation-fields-checkbox-ring-and-names", controls()),
        (
            "open-with",
            crate::g2::open_with_properties::tests::open_with().card(&catalog),
        ),
        (
            "permissions",
            crate::g2::open_with_properties::tests::permissions().card(&catalog),
        ),
        ("properties", properties.card(&catalog)),
        ("run-card", run.card(&catalog)),
        ("bulk-rename", bulk.card(&catalog)),
    ];
    for body in [14., 18.] {
        for (name, card) in &cards {
            let (shell, _, vcx) = mounted(cx, corner, |_, _, cx| {
                let shell = cx.entity();
                cx.new(|_| Operations(card.clone(), shell, body))
            });
            let side = CHECK * body / BOARD_BODY;
            frames(name, &shell, vcx, corner, &[side, side + 4.]);
        }
    }
    let (shell, view, vcx) = mounted(cx, corner, |shell, window, cx| {
        let palette = crate::list::Palette::inherited(&window.text_style(), super::theme());
        let view = cx.new(|_| {
            file_ops::host::Overlay::new(
                Arc::default(),
                file_ops::palette::CardPalette::from_list(&palette, shell.corner),
                "default",
            )
        });
        shell.overlay = Some(view.clone());
        view
    });
    view.update(vcx, |view, cx| view.copy_to(0, false, "/files", cx));
    frames("copy-to-card-field-buttons", &shell, vcx, corner, &[]);
    view.update(vcx, |view, cx| {
        view.model.dialog = None;
        view.model.jobs.update(flea::ui_logic_file_ops::JobRow {
            id: 1,
            state: "running".into(),
            ..Default::default()
        });
        view.model.refresh_blocks(&view.catalog);
        view.model.jobs.folded = false;
        cx.notify();
    });
    painted(vcx);
    vcx.executor()
        .advance_clock(std::time::Duration::from_millis(200));
    frames("operation-jobs-card", &shell, vcx, corner, &[]);
}
