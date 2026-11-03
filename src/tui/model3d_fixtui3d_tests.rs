use super::{
    tests::{model, prime, settle, step, Fixture},
    Controls, Held, Key,
};
use crate::{
    backend::model3d::{self, testdata},
    tui::{
        graphics::{palette, Graphics, Protocol},
        stage,
        theme::{Depth, Theme, Watch},
        wire,
    },
};
use flea::ui_logic_pv4::color::Palette;
use std::{
    fs::File,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

// Sample input: the TUI battery driver's private colors.toml, written from the board's tui-list-text palette.
const BOARD_THEME: &str = "background = \"#1c120d\"\nforeground = \"#c6b395\"\n";
const GROUND: [u8; 3] = [0x1c, 0x12, 0x0d];
const FOREGROUND: [u8; 3] = [0xc6, 0xb3, 0x95];
// The theme's own fallback pair, what a TUI that read no colors.toml draws: the EY-284 capture's stage and parts.
const FALLBACK_GROUND: [u8; 3] = [0x10, 0x13, 0x15];
const FALLBACK_FOREGROUND: [u8; 3] = [0xca, 0xcc, 0xcc];
const THEME_FILE: &str = ".local/state/omarchy/current/theme/colors.toml";
const THEME_WAIT: Duration = Duration::from_secs(5);
// A foreground no theme above shares, so only faces drawn in the role ink change under it.
const PROBE_FOREGROUND: [f32; 3] = [0., 0., 1.];
const PIXELS: (usize, usize) = (160, 120);
// The terminal cells those pixels fill; a still's key carries them but the render reads only the pixels.
const STILL_CELLS: (usize, usize) = (20, 8);
// The tetrahedron covers about 400 pixels of the still; far fewer means the mask lost the part.
const MIN_PART_PIXELS: usize = 300;
// The board ground's red minus blue; a face no warmer than this is neutral, or a neutral edge over the ground.
const GROUND_WARMTH: i32 = 0x1c - 0x0d;
const TETRA_OBJ: &[u8] =
    b"v 0 0 0\nv 10 0 0\nv 0 10 0\nv 0 0 10\nf 1 3 2\nf 1 2 4\nf 1 4 3\nf 2 3 4\n";

fn bytes(color: [f32; 3]) -> [u8; 3] {
    color.map(|c| (c * 255.).round() as u8)
}
fn warmth(pixel: [u8; 3]) -> i32 {
    i32::from(pixel[0]) - i32::from(pixel[2])
}
fn board() -> Palette {
    palette(&Theme::from_text(BOARD_THEME))
}

// A part with its own material, as shelf-kit.3mf's knob: pid and pindex into basematerials group 1024, here #EEEEEC white PLA.
fn tetra_3mf(white: bool) -> Vec<u8> {
    let (group, material) = if white {
        (
            "<basematerials id='1024'><base name='white' displaycolor='#EEEEEC'/></basematerials>",
            " pid='1024' pindex='0'",
        )
    } else {
        ("", "")
    };
    let xml = format!("<model unit='millimeter'><resources>{group}<object id='1' name='bracket' type='model'{material}><mesh><vertices>\
<vertex x='0' y='0' z='0'/><vertex x='10' y='0' z='0'/><vertex x='0' y='10' z='0'/><vertex x='0' y='0' z='10'/></vertices><triangles>\
<triangle v1='0' v2='2' v3='1'/><triangle v1='0' v2='1' v3='3'/><triangle v1='0' v2='3' v3='2'/><triangle v1='1' v2='2' v3='3'/></triangles>\
</mesh></object></resources><build><item objectid='1'/></build></model>");
    testdata::archive(&[("3D/3dmodel.model", xml.as_bytes())])
}

// One Quick Look still of `source` as RGB pixels, through the same render_model the TUI calls.
fn still(root: &Fixture, format: &str, source: &[u8], palette: Palette) -> Vec<[u8; 3]> {
    std::fs::write(root.path("part.src"), source).unwrap();
    let parsed = model3d::parse(format, source).unwrap();
    model3d::wire::write(&parsed, File::create(root.path("cache.3d")).unwrap()).unwrap();
    let key = Key {
        path: root.path("part.src"),
        generation: 1,
        controls: Controls::default(),
        pixels: PIXELS,
        cells: STILL_CELLS,
        palette,
        quicklook: true,
    };
    let held = Arc::new(Held::open(root.path("part.src")).unwrap());
    let still = crate::tui::still::render_model(
        held,
        root.path("cache.3d"),
        key,
        None,
        Some(root.path("")),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(still.rgba.len(), PIXELS.0 * PIXELS.1 * 4);
    still
        .rgba
        .chunks_exact(4)
        .map(|p| [p[0], p[1], p[2]])
        .collect()
}
// The faces drawn in the role ink: the only pixels a foreground change moves, since ground and plate keep their own roles.
fn role_faces(root: &Fixture, format: &str, source: &[u8]) -> Vec<usize> {
    let shown = still(root, format, source, board());
    let probed = still(
        root,
        format,
        source,
        Palette {
            foreground: PROBE_FOREGROUND,
            ..board()
        },
    );
    let faces: Vec<usize> = (0..shown.len())
        .filter(|&i| shown[i] != probed[i])
        .collect();
    assert!(
        faces.len() >= MIN_PART_PIXELS,
        "only {} pixels follow the foreground role in the uncoloured {format} still",
        faces.len()
    );
    faces
}

#[test]
fn fixtui3d_ey284_a_colors_toml_sets_the_still_palette_and_none_gives_the_capture_pair() {
    let home = std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap();
    assert_eq!(
        Theme::path(),
        home.join(THEME_FILE),
        "the TUI reads the file the battery driver writes under its HOME"
    );
    let root = Fixture::new();
    std::fs::write(root.path("colors.toml"), BOARD_THEME).unwrap();
    let watch = Watch::start(root.path("colors.toml"));
    let text = watch
        .updates
        .recv_timeout(THEME_WAIT)
        .expect("the theme watcher delivered colors.toml");
    let live = palette(&Theme::with_depth(&text, Depth::Truecolor));
    assert_eq!(
        (bytes(live.ground), bytes(live.foreground)),
        (GROUND, FOREGROUND),
        "the still's palette is the theme file's ground and foreground"
    );
    let none = palette(&Theme::with_depth("", Depth::Truecolor));
    assert_eq!(
        (bytes(none.ground), bytes(none.foreground)),
        (FALLBACK_GROUND, FALLBACK_FOREGROUND),
        "only a TUI that read no theme draws the capture's stage and parts"
    );
}

#[test]
fn fixtui3d_ey284_still_draws_on_the_theme_ground_with_foreground_parts() {
    let root = Fixture::new();
    let pixels = still(&root, "obj", TETRA_OBJ, board());
    for corner in [
        0,
        PIXELS.0 - 1,
        PIXELS.0 * (PIXELS.1 - 1),
        PIXELS.0 * PIXELS.1 - 1,
    ] {
        assert_eq!(
            pixels[corner], GROUND,
            "the stage is the card ground, not a near-black fallback"
        );
    }
    for i in role_faces(&root, "obj", TETRA_OBJ) {
        assert!(
            warmth(pixels[i]) > GROUND_WARMTH,
            "a part with no colour of its own takes the tan foreground role at pixel {i}: {:?}",
            pixels[i]
        );
    }
}

#[test]
fn fixtui3d_ey284_a_3mf_basematerial_colour_wins_over_the_foreground() {
    let root = Fixture::new();
    let faces = role_faces(&root, "3mf", &tetra_3mf(false));
    let tan = still(&root, "3mf", &tetra_3mf(false), board());
    let white = still(&root, "3mf", &tetra_3mf(true), board());
    let probed = still(
        &root,
        "3mf",
        &tetra_3mf(true),
        Palette {
            foreground: PROBE_FOREGROUND,
            ..board()
        },
    );
    assert!(
        white == probed,
        "a face with a basematerial never draws in the foreground role"
    );
    // #EEEEEC is lighter than the #c6b395 role at every tone step, so each face pixel must be too.
    for &i in &faces {
        assert!(
            warmth(white[i]) <= GROUND_WARMTH,
            "white PLA stays neutral on a tan theme at pixel {i}: {:?}",
            white[i]
        );
        assert!(
            white[i].iter().zip(tan[i]).all(|(&w, t)| w > t),
            "white PLA draws lighter than the tan role at pixel {i}: {:?} against {:?}",
            white[i],
            tan[i]
        );
    }
}

#[test]
fn fixtui3d_ey284_a_theme_change_renders_a_new_still() {
    let root = Fixture::new();
    let mut m = model(&root);
    let (mut wire, reader) = wire::echo_wire();
    let (mut graphics, mut shown) = (Graphics::new(), stage::Shown::default());
    graphics.protocol = Protocol::Kitty;
    m.capabilities.graphics = graphics.protocol;
    prime(&mut m, &root, &mut graphics, &mut shown, &mut wire);
    settle(|| {
        step(&mut m, &mut graphics, &mut shown, &mut wire, false);
        m.model3d.ready.is_some()
    });
    assert_eq!(m.model3d.renders, 1);
    assert_ne!(m.model3d_palette, board());
    m.model3d_palette = board();
    settle(|| {
        step(&mut m, &mut graphics, &mut shown, &mut wire, false);
        m.model3d
            .ready
            .as_ref()
            .is_some_and(|r| r.key.palette == board())
    });
    assert_eq!(
        m.model3d.renders, 2,
        "the new theme rendered a new still rather than keeping the old one"
    );
    assert_eq!(
        m.model3d.ready.as_ref().unwrap().rgba[..3],
        GROUND,
        "the new still stands on the new ground"
    );
    m.model3d.stop(&mut wire).unwrap();
    drop(graphics);
    wire::finish(wire, reader);
}
