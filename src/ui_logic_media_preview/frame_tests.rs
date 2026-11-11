use super::frame::*;
use super::picture::{decode_size, fit};

fn raw(tag: &[u8; 4], a: u32, b: u32, length: u32) -> [u8; HEADER] {
    let mut out = [0; HEADER];
    out[..4].copy_from_slice(tag);
    out[4..8].copy_from_slice(&a.to_le_bytes());
    out[8..12].copy_from_slice(&b.to_le_bytes());
    out[12..].copy_from_slice(&length.to_le_bytes());
    out
}

#[test]
fn pv3_a_frame_header_round_trips_and_names_its_payload() {
    let record = Record::Frame {
        width: 1920,
        height: 1080,
    };
    assert_eq!(parse_header(&header(record)), Ok(record));
    assert_eq!(record.length(), 8_294_400);
    let event = Record::Event { length: 9 };
    assert_eq!(parse_header(&header(event)), Ok(event));
}

// The g07 frame test's negatives: oversized, mismatched, unknown and empty records are refused before any allocation.
#[test]
fn pv3_corrupt_headers_are_refused_before_allocating() {
    assert!(parse_header(&raw(b"FRM1", u32::MAX, 1, 4)).is_err());
    assert!(parse_header(&raw(b"FRM1", 3841, 2160, 3841 * 2160 * 4)).is_err());
    assert!(parse_header(&raw(b"FRM1", 3840, 2161, 0)).is_err());
    assert!(parse_header(&raw(b"FRM1", 0, 1, 0)).is_err());
    assert!(parse_header(&raw(b"FRM1", 2, 2, 15)).is_err());
    assert!(parse_header(&raw(b"XRM1", 2, 2, 16)).is_err());
    assert!(parse_header(&raw(b"EVT1", 0, 0, 0)).is_err());
    assert!(parse_header(&raw(b"EVT1", 0, 0, MAX_EVENT + 1)).is_err());
    assert!(parse_header(&raw(b"EVT1", 1, 0, 4)).is_err());
    assert!(parse_header(&raw(b"FRM1", 3840, 2160, 3840 * 2160 * 4)).is_ok());
}

#[test]
fn pv3_helper_events_round_trip_and_garbage_is_ignored() {
    let events = [
        Event::Loaded {
            video: true,
            duration_ms: 2_000,
        },
        Event::Loaded {
            video: false,
            duration_ms: 0,
        },
        Event::Playing,
        Event::Paused,
        Event::Stopped,
        Event::Position(1_234),
        Event::Seeked(700),
        Event::Ended,
        Event::Failed(Failure::Unsupported),
        Event::Failed(Failure::Unreadable),
        Event::Failed(Failure::Helper),
    ];
    for event in events {
        assert_eq!(Event::parse(&event.render()), Some(event));
    }
    for bad in [
        "",
        "loaded 1",
        "position -1",
        "position x",
        "failed",
        "failed boom",
        "seeked",
        "rewound",
    ] {
        assert_eq!(Event::parse(bad), None, "{bad:?}");
    }
}

#[test]
fn pv3_commands_round_trip_and_bad_lines_are_ignored() {
    for command in [
        Command::Play,
        Command::Pause,
        Command::Stop,
        Command::Quit,
        Command::Seek(1_500),
        Command::Mute(true),
        Command::Mute(false),
        Command::Gain(0.25),
    ] {
        assert_eq!(Command::parse(&command.render()), Some(command));
    }
    assert_eq!(Command::Seek(-3).render(), "seek 0");
    assert_eq!(Command::Gain(4.).render(), "gain 1.000000");
    assert_eq!(Command::parse("gain 7"), Some(Command::Gain(1.)));
    for bad in [
        "", "seek", "seek -1", "seek x", "mute 2", "gain NaN", "gain inf", "play now", "jump",
    ] {
        assert_eq!(Command::parse(bad), None, "{bad:?}");
    }
}

#[test]
fn pv3_a_picture_fits_its_box_centred_without_cropping() {
    assert_eq!(fit(1920., 1080., 960., 960.), [0., 210., 960., 540.]);
    assert_eq!(fit(640., 480., 320., 240.), [0., 0., 320., 240.]);
    assert_eq!(fit(100., 400., 300., 200.), [125., 0., 50., 200.]);
    assert_eq!(fit(0., 10., 10., 10.), [0.; 4]);
    assert_eq!(fit(10., 10., 0., 10.), [0.; 4]);
}

#[test]
fn pv3_the_decoded_size_never_grows_and_stays_in_bounds() {
    assert_eq!(decode_size(1920, 1080, 960, 700), (960, 540));
    assert_eq!(decode_size(320, 240, 1100, 700), (320, 240));
    assert_eq!(decode_size(7680, 4320, 8000, 8000), (3840, 2160));
    assert_eq!(decode_size(0, 4, 10, 10), (0, 0));
    assert_eq!(decode_size(3, 3, 1, 1), (2, 2));
}
