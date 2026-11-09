use super::{
    action_key_tests::{drain, listing},
    actions,
    graphics::Protocol,
    input::Key,
    keymap::Map,
    media::Player,
    model::Model,
    wire,
};
use std::path::Path;

const PRESETS: [&str; 5] = ["default", "vim", "mac", "windows", "yazi"];

// A focused player whose mpv is starting, so a play or pause waits in resume where the test can read it.
fn focused(wire: &mut wire::Wire, preset: &str) -> Model {
    let mut m = listing(wire, preset);
    let mut player = Player::resting(
        Path::new("/nonexistent/clip.mp4"),
        Protocol::Kitty,
        true,
        (40, 30, 81, 3),
        (160, 50),
        (800, 600),
    );
    player.probe(6_000);
    player.force_starting_for_test();
    m.player = Some(player);
    m.preview_visible = true;
    m.preview_focus = true;
    m
}

fn press(m: &mut Model, w: &mut wire::Wire, key: Key) {
    actions::key(m, &key, &Map::load(), w).unwrap();
}

fn state(m: &Model) -> (bool, Option<(f64, bool)>) {
    let player = m.player.as_ref().expect("the player stays up");
    (player.paused, player.resume)
}

// S12C-4: p plays and pauses as Space and Return on the focused Play control do, in every preset and on either control.
#[test]
fn p_plays_and_pauses_the_focused_player_in_every_preset() {
    for preset in PRESETS {
        for control in [0, 1] {
            let (mut w, reader) = wire::echo_wire();
            let mut m = focused(&mut w, preset);
            m.player.as_mut().unwrap().control = control;
            press(&mut m, &mut w, Key::character('p', ""));
            assert_eq!(
                state(&m),
                (false, Some((0.0, false))),
                "{preset}: p plays with control {control}"
            );
            press(&mut m, &mut w, Key::character('p', ""));
            assert_eq!(
                state(&m),
                (true, Some((0.0, true))),
                "{preset}: p pauses with control {control}"
            );
            assert!(m.preview_focus, "{preset}: p keeps the preview focused");
            drain(&mut w);
            wire::finish(w, reader);
        }
        for key in [Key::named("Return", ""), Key::character(' ', "")] {
            let (mut w, reader) = wire::echo_wire();
            let mut m = focused(&mut w, preset);
            press(&mut m, &mut w, key.clone());
            assert_eq!(
                state(&m),
                (false, Some((0.0, false))),
                "{preset}: {} on Play plays as p does",
                key.name
            );
            drain(&mut w);
            wire::finish(w, reader);
        }
    }
}

// The terminal player draws no mute state, so m is not a media key there and leaves the player as it was.
#[test]
fn m_is_not_offered_as_mute_by_the_terminal_player() {
    let map = Map::load();
    for preset in PRESETS {
        assert_ne!(
            map.in_context(&Key::character('m', ""), preset, "media"),
            "mute",
            "{preset}"
        );
        let (mut w, reader) = wire::echo_wire();
        let mut m = focused(&mut w, preset);
        drain(&mut w);
        press(&mut m, &mut w, Key::character('m', ""));
        assert_eq!(state(&m), (true, None), "{preset}: m leaves the player");
        assert!(m.preview_focus, "{preset}: m keeps the preview focused");
        assert_eq!(m.key_arm, "", "{preset}: m arms no listing prefix");
        assert!(
            drain(&mut w).is_empty(),
            "{preset}: m sends no listing command"
        );
        wire::finish(w, reader);
    }
}
