use super::*;

#[test]
fn rotated_preview_fits_the_upright_aspect_before_sizing_the_render_buffer() {
    for degrees in [0, 90, 180, 270] {
        let mut state = State::new(false);
        state.clip = (1920, 1440);
        state.box_size = (800, 600);
        state.rotation = rotation::Rotation::from_degrees(degrees);
        state.update_size();
        let portrait = degrees == 90 || degrees == 270;
        assert_eq!(state.size, if portrait { (450, 600) } else { (800, 600) });
        assert_eq!(
            state.rotation.dimensions(state.size),
            if portrait { (600, 450) } else { (800, 600) }
        );
        state.box_size = (300, 900);
        state.update_size();
        assert_eq!(state.size, if portrait { (300, 400) } else { (300, 226) });
    }
}

#[test]
fn rotated_frame_wire_limits_apply_to_the_upright_picture() {
    let mut state = State::new(false);
    state.clip = (MAX_SIDE, MAX_SIDE / 2);
    state.box_size = (u32::MAX, u32::MAX);
    for degrees in [0, 90, 180, 270] {
        state.rotation = rotation::Rotation::from_degrees(degrees);
        state.update_size();
        let portrait = degrees == 90 || degrees == 270;
        assert_eq!(state.size.1 > state.size.0, portrait, "rotation {degrees}");
        let record = flea::ui_logic_media_preview::frame::Record::Frame {
            width: state.size.0,
            height: state.size.1,
        };
        assert_eq!(
            flea::ui_logic_media_preview::frame::parse_header(
                &flea::ui_logic_media_preview::frame::header(record)
            ),
            Ok(record)
        );
    }
}

#[test]
fn rotation_metadata_after_the_clip_size_refits_the_upright_picture() {
    let mut state = State::new(false);
    state.clip = (1920, 1440);
    state.box_size = (800, 600);
    state.update_size();
    assert_eq!(state.size, (800, 600));
    state.rotate(-90);
    assert_eq!(state.rotation, rotation::Rotation::Counterclockwise);
    assert_eq!(state.size, (450, 600));
    state.rotate(180);
    assert_eq!(state.size, (800, 600));
}

#[test]
fn upd_metadata_before_file_loaded_is_kept_and_later_changes_are_reported() {
    let mut state = State::new(false);
    assert_eq!(state.metadata(DURATION, 3_000), None);
    assert_eq!(state.metadata(VIDEO, 1), None);
    state.loaded = true;
    assert_eq!(
        state.facts(),
        Event::Loaded {
            video: true,
            duration_ms: 3_000
        }
    );
    assert_eq!(state.metadata(DURATION, 3_000), None);
    assert_eq!(
        state.metadata(DURATION, 4_000),
        Some(Event::Loaded {
            video: true,
            duration_ms: 4_000
        })
    );
    assert_eq!(
        state.metadata(VIDEO, 0),
        Some(Event::Loaded {
            video: false,
            duration_ms: 4_000
        })
    );
}

#[test]
fn upd_metadata_after_file_loaded_updates_video_and_audio_without_polling() {
    let mut state = State::new(false);
    state.loaded = true;
    assert_eq!(state.metadata(VIDEO, 0), None);
    assert_eq!(
        state.metadata(DURATION, 2_000),
        Some(Event::Loaded {
            video: false,
            duration_ms: 2_000
        })
    );
    assert_eq!(
        state.metadata(VIDEO, 2),
        Some(Event::Loaded {
            video: true,
            duration_ms: 2_000
        })
    );
}

#[test]
fn pv3_under_the_tone_map_a_half_reported_size_never_reaches_a_frame() {
    let mut state = State::new(false);
    state.clip = (3840, 2160);
    state.box_size = (1974, 1110);
    state.tonemap.source = flea::ui_logic_media_preview::tonemap::Source {
        hdr: true,
        width: 3840,
        height: 2160,
        rotated: false,
        peak_nits: 0,
    };
    state.tonemap.set_rate(29.97);
    state.update_size();
    assert_eq!(state.size, (1974, 1110));
    assert!(state.tonemap.update(1974, 1110).is_some());
    state.update_size();
    assert_eq!(state.size, (1480, 832));
    // mpv reports the filtered width before the height: the frame keeps the mapped size.
    state.clip = (1480, 2160);
    state.update_size();
    assert_eq!(state.size, (1480, 832));
    state.clip = (1480, 832);
    state.update_size();
    assert_eq!(state.size, (1480, 832));
}

/// One settle and one 2 s window presenting a quarter of a 29.97 fps clip's frames with every CPU busy.
fn starve(state: &mut State, look: &mut flea::ui_logic_media_preview::tonemap_pace::Look) {
    for (wall, frames) in [(0, 0), (1000, 8), (2000, 15)] {
        look.wall_ms += wall;
        look.position_ms += wall as i64;
        look.presented += frames;
        look.cpu_ms += wall * u64::from(state.cpus);
        filter::judge(state, *look);
    }
}

#[test]
fn pv3_a_map_at_its_floor_steps_again_once_the_box_grows() {
    let mut state = State::new(false);
    state.box_size = (752, 424);
    state.tonemap.source = flea::ui_logic_media_preview::tonemap::Source {
        hdr: true,
        width: 3840,
        height: 2160,
        rotated: false,
        peak_nits: 0,
    };
    state.tonemap.set_rate(29.97);
    let mut look = flea::ui_logic_media_preview::tonemap_pace::Look::default();
    assert!(state.tonemap.update(752, 424).is_some());
    starve(&mut state, &mut look);
    assert_eq!(state.tonemap.level(), 1, "the pane steps to its floor");
    starve(&mut state, &mut look);
    assert_eq!((state.floor_logged, state.tonemap.level()), (Some(1), 1));
    // Quick Look's box: the start cap holds level 1 there, and the starved map keeps stepping.
    state.box_size = (1974, 1110);
    assert!(state.tonemap.update(1974, 1110).is_some());
    assert_eq!(state.tonemap.size(), Some((1480, 832)));
    starve(&mut state, &mut look);
    assert_eq!(state.tonemap.level(), 2);
}
