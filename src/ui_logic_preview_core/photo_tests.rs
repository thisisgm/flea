// propertyfacts.js codecs, bitRate, camera, exposure and taken checks for photo.rs, plus duration, rate, permissions and links.
use super::photo::*;

#[test]
fn codecs_names_streams_as_the_card_writes_them() {
    // H.264 and AAC read as the board has them, joined by a spaced middle dot
    assert_eq!(codecs("h264", "aac"), "H.264 \u{b7} AAC");
    // a video with no audio stream names its one codec
    assert_eq!(codecs("hevc", ""), "H.265");
    // an audio-only container names its one codec
    assert_eq!(codecs("", "opus"), "Opus");
    // no stream names no Codecs value
    assert_eq!(codecs("", ""), "");
    // a codec the table does not know is upper-cased as ffprobe names it
    assert_eq!(codecs("prores", "pcm_s24le"), "PRORES \u{b7} PCM");
    // VP9 and AV1 keep their own names
    assert_eq!(format!("{}|{}", codecs("vp9", "vorbis"), codecs("av1", "mp3")), "VP9 \u{b7} Vorbis|AV1 \u{b7} MP3");
}

#[test]
fn bit_rate_reads_decimal_prefixes() {
    let i18n = super::english();
    // a bit rate over a megabit is one decimal of Mb/s
    assert_eq!(bit_rate(8_100_000.0, &i18n), "8.1 Mb/s");
    // the megabit edge rounds up into Mb/s and never reads 1000 kb/s
    assert_eq!(
        format!("{}|{}|{}", bit_rate(999_499.0, &i18n), bit_rate(999_500.0, &i18n), bit_rate(1_000_000.0, &i18n)),
        "999 kb/s|1.0 Mb/s|1.0 Mb/s"
    );
    // a bit rate under a megabit is whole kb/s
    assert_eq!(
        format!("{}|{}|{}", bit_rate(640_000.0, &i18n), bit_rate(1000.0, &i18n), bit_rate(128_400.0, &i18n)),
        "640 kb/s|1 kb/s|128 kb/s"
    );
    // under a kilobit it is b/s
    assert_eq!(format!("{}|{}", bit_rate(999.0, &i18n), bit_rate(8.0, &i18n)), "999 b/s|8 b/s");
    // an unknown or zero bit rate names no value
    assert_eq!(
        format!("{}|{}|{}", bit_rate(0.0, &i18n), bit_rate(f64::NAN, &i18n), bit_rate(-5.0, &i18n)),
        "||"
    );
}

#[test]
fn camera_leads_with_the_make_once() {
    // a Canon model already carries its make, so it is not said twice
    assert_eq!(camera("Canon", "Canon EOS R6"), "Canon EOS R6");
    // a model without its make gets it in front
    assert_eq!(camera("SONY", "ILCE-7M4"), "SONY ILCE-7M4");
    // a make alone and a model alone each stand
    assert_eq!(format!("{}|{}", camera("Canon", ""), camera("", "X100V")), "Canon|X100V");
    // a make with a company suffix is not said twice when the model repeats its first word
    assert_eq!(
        format!(
            "{}|{}|{}",
            camera("NIKON CORPORATION", "NIKON Z 6"),
            camera("OM Digital Solutions", "OM-1"),
            camera("OLYMPUS IMAGING CORP.", "OLYMPUS E-M1")
        ),
        "NIKON Z 6|OM-1|OLYMPUS E-M1"
    );
    // a make whose model never repeats its first word still leads
    assert_eq!(
        format!("{}|{}", camera("SONY", "ILCE-7M4"), camera("OM Digital Solutions", "TG-7")),
        "SONY ILCE-7M4|OM Digital Solutions TG-7"
    );
}

#[test]
fn exposure_writes_shutter_aperture_and_iso() {
    let i18n = super::english();
    // the exposure is shutter, aperture and ISO as the board writes it
    assert_eq!(exposure(Some((1.0, 250.0)), Some((28.0, 10.0)), 400, &i18n), "1/250 s \u{b7} f/2.8 \u{b7} ISO 400");
    // a shutter that is not one over a whole number reads as the nearest 1/n
    assert_eq!(exposure(Some((10.0, 600.0)), None, 0, &i18n), "1/60 s");
    // a fast shutter no whole n divides reads as the nearest 1/n
    assert_eq!(exposure(Some((3.0, 1000.0)), None, 0, &i18n), "1/333 s");
    // a shutter that rounds to a second reads as one
    assert_eq!(
        format!("{}|{}", exposure(Some((96.0, 100.0)), None, 0, &i18n), exposure(Some((99.0, 100.0)), None, 0, &i18n)),
        "1 s|1 s"
    );
    // a shutter of a second or more is seconds
    assert_eq!(
        format!("{}|{}", exposure(Some((2.0, 1.0)), None, 0, &i18n), exposure(Some((13.0, 10.0)), None, 0, &i18n)),
        "2 s|1.3 s"
    );
    // a shutter under a second keeps its stored rational
    assert_eq!(
        format!(
            "{}|{}|{}|{}",
            exposure(Some((4.0, 10.0)), None, 0, &i18n),
            exposure(Some((5.0, 10.0)), None, 0, &i18n),
            exposure(Some((6.0, 10.0)), None, 0, &i18n),
            exposure(Some((8.0, 10.0)), None, 0, &i18n)
        ),
        "0.4 s|1/2 s|0.6 s|0.8 s"
    );
    // a numerator of one reads 1/den and a numerator that divides den reads 1/n
    assert_eq!(
        format!(
            "{}|{}|{}",
            exposure(Some((1.0, 3.0)), None, 0, &i18n),
            exposure(Some((1.0, 250.0)), None, 0, &i18n),
            exposure(Some((2.0, 500.0)), None, 0, &i18n)
        ),
        "1/3 s|1/250 s|1/250 s"
    );
    // 2.5 s reads one decimal of seconds
    assert_eq!(exposure(Some((5.0, 2.0)), None, 0, &i18n), "2.5 s");
    // a whole f-number drops the decimal
    assert_eq!(exposure(None, Some((4.0, 1.0)), 0, &i18n), "f/4");
    // an exposure missing parts keeps the rest in order
    assert_eq!(
        format!(
            "{}|{}",
            exposure(None, Some((18.0, 10.0)), 100, &i18n),
            exposure(Some((1.0, 60.0)), None, 3200, &i18n)
        ),
        "f/1.8 \u{b7} ISO 100|1/60 s \u{b7} ISO 3200"
    );
    // no exposure fact names no value
    assert_eq!(
        format!(
            "{}|{}",
            exposure(Some((0.0, 0.0)), Some((0.0, 0.0)), 0, &i18n),
            exposure(None, None, 0, &i18n)
        ),
        "|"
    );
    // a shutter with a zero denominator is no shutter
    assert_eq!(exposure(Some((1.0, 0.0)), None, 0, &i18n), "");
}

#[test]
fn taken_reads_minutes_of_the_card_stamp() {
    // Taken is the card's own stamp, minutes only
    assert_eq!(taken("2026:09:12 18:31:07"), "2026-09-12 18:31");
    // a camera's blank or malformed stamp names no value; JS undefined maps to ""
    assert_eq!(
        format!("{}|{}|{}|{}", taken("0000:00:00 00:00:00"), taken("yesterday"), taken(""), taken("")),
        "|||"
    );
}

#[test]
fn duration_reads_minutes_and_seconds() {
    // 72000 ms reads 1:12
    assert_eq!(duration(72_000), "1:12");
    // 245000 ms reads 4:05
    assert_eq!(duration(245_000), "4:05");
    // an hour on the clock adds an hour field
    assert_eq!(duration(3_785_000), "1:03:05");
    // zero reads 0:00
    assert_eq!(duration(0), "0:00");
}

#[test]
fn sample_rate_reads_kilohertz() {
    let i18n = super::english();
    // 44100 Hz reads 44.1 kHz
    assert_eq!(sample_rate(44_100.0, &i18n), "44.1 kHz");
    // 48000 Hz reads 48 kHz
    assert_eq!(sample_rate(48_000.0, &i18n), "48 kHz");
    // nothing probed reads no value
    assert_eq!(sample_rate(0.0, &i18n), "");
}

#[test]
fn permissions_read_the_low_nine_bits() {
    // 0o100644 reads rw-r--r--
    assert_eq!(permissions(0o100644), "rw-r--r--");
    // 0o100600 reads rw-------
    assert_eq!(permissions(0o100600), "rw-------");
}

#[test]
fn is_symlink_reads_the_file_type_bits() {
    // 0o120777 is a symlink
    assert!(is_symlink(0o120777));
    // 0o100644 is a regular file, not a symlink
    assert!(!is_symlink(0o100644));
}
