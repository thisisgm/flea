// Photo and media facts in the card language: camera, exposure, focal length, taken, codecs and bit rate.
use super::FACT_SEP;
use crate::i18n::I18n;

// A rational as numerator and denominator; zero on either side is no value.
pub type Ratio = Option<(f64, f64)>;

pub fn ratio(pair: Ratio) -> f64 {
    match pair {
        Some((n, d)) if n > 0.0 && d > 0.0 => n / d,
        _ => 0.0,
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Photo {
    pub make: String,
    pub model: String,
    pub lens: String,
    pub exp: Ratio,
    pub fnum: Ratio,
    pub iso: u32,
    pub focal: Ratio,
    pub taken: String,
    pub w: u32,
    pub h: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PhotoFacts {
    pub camera: String,
    pub lens: String,
    pub exposure: String,
    pub focal: String,
    pub taken: String,
}

// JavaScript's String(Math.round(value * 10) / 10): one decimal, a trailing zero dropped.
pub fn tenths(value: f64) -> String {
    let t = (value * 10.0 + 0.5).floor() / 10.0;
    if t.fract() == 0.0 {
        format!("{}", t as i64)
    } else {
        format!("{t:.1}")
    }
}

fn js_round(value: f64) -> i64 { (value + 0.5).floor() as i64 }

// A model that starts with the make's first word stands alone, in any case.
pub fn camera(make: &str, model: &str) -> String {
    if make.is_empty() || model.is_empty() {
        return if model.is_empty() { make.into() } else { model.into() };
    }
    let brand = make.split_whitespace().next().unwrap_or("").to_lowercase();
    if model.to_lowercase().starts_with(&brand) {
        model.into()
    } else {
        format!("{make} {model}")
    }
}

// "2026:09:12 18:31:00", EXIF's stamp, reads "2026-09-12 18:31"; a zero or malformed one is absent.
pub fn taken(stamp: &str) -> String {
    let b = stamp.as_bytes();
    let digits = |r: std::ops::Range<usize>| b.get(r.clone()).is_some_and(|s| s.iter().all(u8::is_ascii_digit));
    let shaped = b.len() >= 16 && digits(0..4) && b[4] == b':' && digits(5..7) && b[7] == b':' && digits(8..10)
        && b[10] == b' ' && digits(11..13) && b[13] == b':' && digits(14..16);
    if !shaped || &stamp[0..4] == "0000" {
        return String::new();
    }
    format!("{}-{}-{} {}:{}", &stamp[0..4], &stamp[5..7], &stamp[8..10], &stamp[11..13], &stamp[14..16])
}

// The column's photo facts: 1/N under 0.3 s, one-place seconds from there on.
pub fn photo_facts(photo: Option<&Photo>, i18n: &I18n) -> PhotoFacts {
    let default = Photo::default();
    let p = photo.unwrap_or(&default);
    let mut parts = Vec::new();
    let seconds = ratio(p.exp);
    if seconds > 0.0 {
        let (n, d) = p.exp.unwrap_or((1.0, 1.0));
        parts.push(if seconds < 0.3 {
            i18n.tr_args("preview.photo.fraction", &[&js_round(d / n).to_string()])
        } else {
            i18n.tr_args("preview.photo.seconds", &[&local(i18n, &tenths(seconds))])
        });
    }
    if ratio(p.fnum) > 0.0 {
        parts.push(i18n.tr_args("preview.photo.aperture", &[&local(i18n, &tenths(ratio(p.fnum)))]));
    }
    if p.iso > 0 {
        parts.push(i18n.tr_args("preview.photo.iso", &[&p.iso.to_string()]));
    }
    let focal = ratio(p.focal);
    PhotoFacts {
        camera: camera(&p.make, &p.model),
        lens: p.lens.clone(),
        exposure: parts.join(FACT_SEP),
        focal: if focal > 0.0 { i18n.tr_args("preview.photo.focal", &[&js_round(focal).to_string()]) } else { String::new() },
        taken: taken(&p.taken),
    }
}

fn local(i18n: &I18n, number: &str) -> String { i18n.locale().localize_number(number) }

// The Properties card's shutter: a whole second reads as one, a fraction whose numerator divides stays 1/n.
pub fn shutter_text(num: f64, den: f64, seconds: f64, i18n: &I18n) -> String {
    let tenths_count = js_round(seconds * 10.0);
    if tenths_count >= 10 {
        return i18n.tr_args("preview.photo.seconds", &[&local(i18n, &tenths(tenths_count as f64 / 10.0))]);
    }
    if den % num == 0.0 || num == 1.0 {
        let n = if num == 1.0 { den } else { den / num };
        return i18n.tr_args("preview.photo.fraction", &[&format!("{n}")]);
    }
    if tenths_count > 0 {
        let text = format!("{:.1}", tenths_count as f64 / 10.0);
        return i18n.tr_args("preview.photo.seconds", &[&local(i18n, &text)]);
    }
    i18n.tr_args("preview.photo.fraction", &[&js_round(1.0 / seconds).to_string()])
}

// exposure((1, 250), (28, 10), 400) reads "1/250 s · f/2.8 · ISO 400"; a part the file lacks is left out.
pub fn exposure(shutter: Ratio, fnumber: Ratio, iso: u32, i18n: &I18n) -> String {
    let mut parts = Vec::new();
    let seconds = ratio(shutter);
    if let (true, Some((n, d))) = (seconds > 0.0, shutter) {
        parts.push(shutter_text(n, d, seconds, i18n));
    }
    let stop = ratio(fnumber);
    if stop > 0.0 {
        parts.push(i18n.tr_args("preview.photo.aperture", &[&local(i18n, &tenths(stop))]));
    }
    if iso > 0 {
        parts.push(i18n.tr_args("preview.photo.iso", &[&iso.to_string()]));
    }
    parts.join(FACT_SEP)
}

const CODECS: [(&str, &str); 17] = [
    ("h264", "H.264"), ("hevc", "H.265"), ("vp8", "VP8"), ("vp9", "VP9"), ("av1", "AV1"), ("mpeg4", "MPEG-4"),
    ("mpeg2video", "MPEG-2"), ("mpeg1video", "MPEG-1"), ("aac", "AAC"), ("mp3", "MP3"), ("opus", "Opus"),
    ("vorbis", "Vorbis"), ("flac", "FLAC"), ("ac3", "AC-3"), ("eac3", "E-AC-3"), ("dts", "DTS"), ("alac", "ALAC"),
];

// ffprobe's codec name as the card writes it; one the table does not know is upper-cased.
pub fn codec_name(name: &str) -> String {
    if name.is_empty() {
        return String::new();
    }
    if name.starts_with("pcm_") {
        return "PCM".into();
    }
    CODECS.iter().find(|(k, _)| *k == name).map_or_else(|| name.to_uppercase(), |(_, v)| (*v).into())
}

pub fn codecs(video: &str, audio: &str) -> String {
    [codec_name(video), codec_name(audio)].into_iter().filter(|n| !n.is_empty()).collect::<Vec<_>>().join(FACT_SEP)
}

// 8100000 reads "8.1 Mb/s": ffprobe's bits per second, whole kb/s below a megabit.
pub fn bit_rate(bps: f64, i18n: &I18n) -> String {
    if bps.is_nan() || bps <= 0.0 {
        return String::new();
    }
    if bps < 1000.0 {
        return i18n.tr_args("preview.rate.bits", &[&js_round(bps).to_string()]);
    }
    let kilo = js_round(bps / 1000.0);
    if kilo < 1000 {
        return i18n.tr_args("preview.rate.kilobits", &[&kilo.to_string()]);
    }
    i18n.tr_args("preview.rate.megabits", &[&i18n.locale().format_number(bps / 1_000_000.0, 1)])
}

// "3:05", or "1:03:05" once an hour is on the clock.
pub fn duration(ms: i64) -> String {
    let total = (ms / 1000).max(0);
    let (hours, minutes, seconds) = (total / 3600, (total % 3600) / 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

// 48000 is "48 kHz" and 44100 is "44.1 kHz"; nothing probed is no value.
pub fn sample_rate(hz: f64, i18n: &I18n) -> String {
    if hz.is_nan() || hz <= 0.0 {
        return String::new();
    }
    let khz = hz / 1000.0;
    let text = if khz.fract() == 0.0 { format!("{}", khz as i64) } else { format!("{khz:.1}") };
    i18n.tr_args("preview.rate.khz", &[&local(i18n, &text)])
}

// The low nine bits of st_mode, read three at a time.
pub fn permissions(mode: u32) -> String {
    (0..9).map(|i| if mode & (1 << (8 - i)) != 0 { ['r', 'w', 'x'][i % 3] } else { '-' }).collect()
}

pub fn is_symlink(mode: u32) -> bool { mode & 0o170000 == 0o120000 }
