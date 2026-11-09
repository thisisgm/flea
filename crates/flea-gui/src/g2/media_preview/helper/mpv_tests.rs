use super::*;

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
