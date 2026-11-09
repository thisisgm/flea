//! An app row still reading its devices or account draws at rest and settles once, red or plain, never grey before red.
use super::{
    lines::{Env, Ink},
    menu::{Face, Menu},
};
use crate::list::Typeface;
use flea::{
    i18n::{I18n, Paths},
    ui_logic_context_menus::{
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

#[derive(Clone, Copy, Debug, PartialEq)]
enum Look {
    Rest,
    Grey,
    Red,
}

fn context(reading: bool, answering: bool) -> Context {
    let peers = || {
        if answering {
            vec![Peer {
                id: "box".into(),
                label: "Box".into(),
            }]
        } else {
            Vec::new()
        }
    };
    Context {
        row_mode: 0o100644,
        row_is_file: true,
        taildrop_installed: true,
        taildrop_peers: if reading { Vec::new() } else { peers() },
        taildrop_refreshing: reading,
        localsend_installed: true,
        localsend_peers: if reading { Vec::new() } else { peers() },
        localsend_checking: reading,
        dropbox_installed: true,
        dropbox_path: if answering && !reading {
            "/home/test/Dropbox".into()
        } else {
            String::new()
        },
        dropbox_refreshing: reading,
        ..Context::default()
    }
}

#[gpui::test]
fn s11j4_an_app_row_reading_draws_at_rest_and_settles_once(cx: &mut TestAppContext) {
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
            let rows = |reading: bool, answering: bool| {
                let mut menu = Menu::default();
                menu.open(
                    Face::File,
                    listing_entries(&context(reading, answering)),
                    (40., 40.),
                    work,
                    "a",
                    &env,
                    window,
                );
                let card = menu.card.unwrap();
                // Every row the pointer can land on, probed down the card a pixel at a time.
                let pointed: Vec<usize> = (0..card.frame.h as i32)
                    .filter_map(|dy| card.row_at(card.frame.x + 1., card.frame.y + dy as f32))
                    .collect();
                card.entries
                    .iter()
                    .zip(&card.built.lines)
                    .enumerate()
                    .filter(|(_, (entry, _))| entry.brand.is_some())
                    .map(|(index, (entry, line))| {
                        let look = match (line.ink == Ink::Errored, line.disabled) {
                            (true, _) => Look::Red,
                            (false, true) => Look::Grey,
                            (false, false) => Look::Rest,
                        };
                        let reach = (entry.steppable(), pointed.contains(&index));
                        (entry.brand, look, reach, line.wanted)
                    })
                    .collect::<Vec<(Option<Brand>, Look, (bool, bool), f32)>>()
            };
            let (reading, gone, live) = (rows(true, false), rows(false, false), rows(false, true));
            assert_eq!(
                reading.len(),
                3,
                "Taildrop, LocalSend and Dropbox: {reading:?}"
            );
            for ((read, down), up) in reading.iter().zip(&gone).zip(&live) {
                assert_eq!(
                    read.1,
                    Look::Rest,
                    "{:?} draws at rest while it reads",
                    read.0
                );
                assert_eq!(
                    read.2,
                    (false, false),
                    "{:?} stays out of the keys' and the pointer's way while it reads",
                    read.0
                );
                assert_eq!(
                    down.1,
                    Look::Red,
                    "{:?} settles red with no devices",
                    down.0
                );
                assert_eq!(
                    down.2,
                    (false, false),
                    "{:?} with no devices stays skipped",
                    down.0
                );
                assert_eq!(up.1, Look::Rest, "{:?} settles plain with a device", up.0);
                assert_eq!(
                    up.2,
                    (true, true),
                    "{:?} with a device takes keys and pointer",
                    up.0
                );
                assert_eq!(read.3, down.3, "{:?} keeps its width as it settles", read.0);
            }
        })
        .unwrap();
}
