//! Caption holds keep the pane cached while the independent Omi overlay continues to animate.
use super::empty_omi_tests::{dual_scenes, edit, open, sample_players};
use super::hero_tests::frame;
use crate::state::Shell;
use gpui::TestAppContext;

#[gpui::test]
fn empty_omi_keeps_looping_between_caption_beats_without_rendering_the_panes(
    cx: &mut TestAppContext,
) {
    use flea::ui_logic_status_feedback::empty::{PHRASES, ROTATE_MS};
    cx.update(Shell::observe_empty_omi_shell_renders);
    let mut players = std::array::from_fn(|_| {
        let mut player = omi::Player::with_options(
            &omi::Kit::bundled(),
            omi::Options {
                mode: Some("thinking".into()),
                ..Default::default()
            },
        );
        player.frame(0.);
        player
    });
    let handle = open(cx, true);
    edit(cx, handle, |shell, _, cx| {
        for surface in &shell.surfaces {
            let mark = &surface.hero.as_ref().unwrap().read(cx).omi;
            assert!(
                mark.offset() <= 0.016,
                "startup samples precede one platform frame"
            );
        }
    });
    sample_players(cx, handle, &mut players);
    // Player::frame integrates clamped deltas; replay samples rather than treating wall time as phase.
    let quiet_at = 3 * PHRASES.len() as u64 * ROTATE_MS + 2400;
    for _ in 0..(quiet_at - super::hero_tests::FRAME) / super::hero_tests::FRAME {
        frame(cx, handle);
        sample_players(cx, handle, &mut players);
    }
    let counts = |cx: &mut TestAppContext| {
        edit(cx, handle, |shell, _, cx| {
            [0, 1].map(|side| {
                let hero = shell.surfaces[side].hero.as_ref().unwrap().read(cx);
                assert!(!hero.rotation().caption_animating());
                hero.counters.renders.get()
            })
        })
    };
    edit(cx, handle, |shell, _, cx| {
        for surface in &shell.surfaces {
            assert_eq!(
                surface.hero.as_ref().unwrap().read(cx).rotation().beats(),
                3 * PHRASES.len()
            );
        }
    });
    for hold in 0..4 {
        let before = counts(cx);
        let pane_renders = cx.update(|cx| Shell::empty_omi_shell_renders(cx));
        assert!(
            pane_renders > 0,
            "opening the real Shell exercises the probe"
        );
        let scene = dual_scenes(cx, handle, &players);
        let mut changed = [false; 2];
        for _ in 0..16 {
            assert_eq!(frame(cx, handle), 1, "both empty Omis share one callback");
            sample_players(cx, handle, &mut players);
            let next = dual_scenes(cx, handle, &players);
            for side in 0..2 {
                changed[side] |= next[side] != scene[side];
            }
        }
        assert_eq!(
            cx.update(|cx| Shell::empty_omi_shell_renders(cx)),
            pane_renders,
            "Omi ticks must not re-enter the cached Shell/pane renderer"
        );
        assert_eq!(
            counts(cx),
            before,
            "Omi ticks leave both heroes and captions cached"
        );
        assert!(
            changed.into_iter().all(|side| side),
            "each Omi animates between caption beats"
        );
        if hold < 3 {
            // The next beat's whole fade repaints only the captions, never a hero or the cached Shell.
            let renders = counts(cx);
            let shell = cx.update(|cx| Shell::empty_omi_shell_renders(cx));
            for _ in 0..ROTATE_MS / super::hero_tests::FRAME - 16 {
                frame(cx, handle);
                sample_players(cx, handle, &mut players);
            }
            assert_eq!(counts(cx), renders, "a caption fade renders no hero");
            assert_eq!(
                cx.update(|cx| Shell::empty_omi_shell_renders(cx)),
                shell,
                "a caption fade renders no Shell"
            );
        }
    }

    // A genuine Shell invalidation must be visible to this independent render observer.
    let pane_renders = cx.update(|cx| Shell::empty_omi_shell_renders(cx));
    edit(cx, handle, |_, _, cx| cx.notify());
    frame(cx, handle);
    assert!(
        cx.update(|cx| Shell::empty_omi_shell_renders(cx)) > pane_renders,
        "Shell invalidation must increment the actual render probe"
    );
}
