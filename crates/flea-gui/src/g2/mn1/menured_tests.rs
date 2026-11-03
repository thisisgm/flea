//! GM 2026-10-29: an app row that cannot answer (LocalSend with no devices, Dropbox signed out, Taildrop) draws its label red and no state word, so the menu keeps its size.
use super::{
    lines::{Env, Ink},
    menu::{Card, Face, Menu},
};
use crate::list::Typeface;
use flea::{
    i18n::{I18n, Paths},
    ui_logic_mn1::{
        build::{listing_entries, Context, Peer},
        fit::Rect,
        Brand,
    },
};
use gpui::{div, prelude::*, Context as GpuiContext, Render, TestAppContext, Window};

struct Host;

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut GpuiContext<Self>) -> impl IntoElement {
        div().size_full()
    }
}

fn context(answering: bool) -> Context {
    let peer = || {
        vec![Peer {
            id: "box".into(),
            label: "Box".into(),
        }]
    };
    Context {
        row_mode: 0o100644,
        row_is_file: true,
        taildrop_installed: true,
        taildrop_peers: if answering { peer() } else { Vec::new() },
        taildrop_code: if answering {
            String::new()
        } else {
            "signedOut".into()
        },
        localsend_installed: true,
        localsend_peers: if answering { peer() } else { Vec::new() },
        dropbox_installed: true,
        dropbox_path: if answering {
            "/home/test/Dropbox".into()
        } else {
            String::new()
        },
        dropbox_code: if answering {
            String::new()
        } else {
            "signedOut".into()
        },
        ..Context::default()
    }
}

fn app_rows(card: &Card) -> Vec<(Option<Brand>, bool, bool, f32)> {
    card.entries
        .iter()
        .zip(&card.built.lines)
        .filter(|(entry, _)| entry.brand.is_some())
        .map(|(entry, line)| {
            (
                entry.brand,
                line.word.is_some(),
                line.ink == Ink::Errored,
                line.wanted,
            )
        })
        .collect()
}

#[gpui::test]
fn an_unavailable_app_row_reads_red_with_no_state_word_and_the_menu_keeps_its_size(
    cx: &mut TestAppContext,
) {
    let catalog = I18n::load(Paths {
        omarchy: "/nonexistent".into(),
        language_file: "/nonexistent".into(),
        catalogs: vec![crate::test_catalogs::english_only()],
    });
    let typeface = Typeface::omarchy_default();
    let env = Env {
        catalog: &catalog,
        preset: "default",
        key_hints: true,
        typeface: &typeface,
        inset: 4.,
    };
    let work = Rect {
        x: 0.,
        y: 0.,
        w: 1100.,
        h: 700.,
    };
    let handle = cx.add_window(|_, _| Host);
    handle
        .update(cx, |_, window, _| {
            let open = |answering: bool| {
                let mut menu = Menu::default();
                let rows = listing_entries(&context(answering));
                menu.open(Face::File, rows, (40., 40.), work, "a", &env, window);
                menu
            };
            let (down, up) = (open(false), open(true));
            let (down, up) = (down.card.as_ref().unwrap(), up.card.as_ref().unwrap());
            let (gone, live) = (app_rows(down), app_rows(up));
            assert_eq!(
                gone.len(),
                3,
                "Taildrop, LocalSend and Dropbox are drawn: {gone:?}"
            );
            for row in &gone {
                assert!(!row.1, "{:?} draws no state word", row.0);
                assert!(row.2, "{:?} draws its label in the theme's red", row.0);
            }
            assert_eq!(
                live.len(),
                gone.len(),
                "the answering menu draws the same app rows"
            );
            for (row, wide) in gone.iter().zip(&live) {
                assert_eq!(
                    row.3, wide.3,
                    "{:?} wants the width it has while it answers",
                    row.0
                );
            }
            assert!(
                down.entries
                    .iter()
                    .filter(|e| e.brand.is_some())
                    .all(|e| e.disabled),
                "the rows stay unavailable"
            );
            assert_eq!(down.frame.w, up.frame.w, "the menu keeps its original size");
        })
        .unwrap();
}
