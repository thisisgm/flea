use super::*;

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
// Fingerprints of the scenes below rendered at base 053117489, before the Mono attributes existed.
const BASE_TRUECOLOR: u64 = 0x0f11_ab91_4283_8d45;
const BASE_INDEXED: u64 = 0x228c_b001_dd7b_f2ff;
const BASE_SIXTEEN: u64 = 0x56e3_41c8_8a41_6ca1;

#[test]
fn mono_colour_ansi_snapshots() {
    for depth in [Depth::Truecolor, Depth::Indexed, Depth::Sixteen] {
        let theme = Theme::with_depth(PALETTE, depth);
        let mut scenes = Vec::new();
        for sub in [
            menu_layout::Sub::Root,
            menu_layout::Sub::OpenWith,
            menu_layout::Sub::Compress,
            menu_layout::Sub::Taildrop,
        ] {
            let mut m = menu_model();
            m.menu_view.sub = sub;
            for cursor in 0..2 {
                m.menu_cursor = cursor;
                scenes.push((format!("menu-{sub:?}-{cursor}"), frame(&m, &theme)));
            }
        }
        let mut m = model();
        m.tabs.push(Tab {
            path: "/fx/Docs".into(),
            cursor: 0,
            back: Vec::new(),
            forward: Vec::new(),
        });
        for tab in 0..2 {
            m.tab = tab;
            scenes.push((format!("tabs-{tab}"), frame(&m, &theme)));
        }
        m.deletion = Some(Deletion {
            token: 1,
            count: 1,
            ..Default::default()
        });
        for focused in [false, true] {
            m.deletion.as_mut().unwrap().destructive = focused;
            scenes.push((format!("buttons-{focused}"), frame(&m, &theme)));
        }
        for kind in ["print", "convert", "combine"] {
            let mut m = opened(kind);
            for choice in 0..2 {
                match kind {
                    "print" => m.printing.card.as_mut().unwrap().choice = choice,
                    "convert" => {
                        m.converting.card.as_mut().unwrap().choice = choice;
                        m.converting.card.as_mut().unwrap().focus = 1;
                    }
                    _ => m.combining.card.as_mut().unwrap().image = choice,
                }
                scenes.push((format!("{kind}-{choice}"), frame(&m, &theme)));
            }
        }
        let mut m = model();
        m.rows.get_mut(&7).unwrap().name = "plates.3mf".into();
        m.model3d.data = Some(Box::new(flea::ui_logic_model_preview::wire::Data {
            plates: vec![Default::default(), Default::default()],
            ..Default::default()
        }));
        for plate in 0..2 {
            m.model3d.controls.plate = plate;
            scenes.push((format!("plates-{plate}"), look::card(&m, &theme, m.columns)));
        }
        // FNV-1a pins the base's complete ANSI streams, including resets and spacing.
        let mut fingerprint = FNV_OFFSET_BASIS;
        for (_, stream) in &scenes {
            for byte in stream.bytes() {
                fingerprint = (fingerprint ^ u64::from(byte)).wrapping_mul(FNV_PRIME);
            }
        }
        let expected = match depth {
            Depth::Truecolor => BASE_TRUECOLOR,
            Depth::Indexed => BASE_INDEXED,
            Depth::Sixteen => BASE_SIXTEEN,
            Depth::Mono => unreachable!(),
        };
        let names: Vec<_> = scenes.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(
            fingerprint, expected,
            "{depth:?}: coloured ANSI changed from the base in one of {names:?}"
        );
    }
}
