//! Camera RAW names shared by MIME fallback, thumbnail routing and GUI request policy.
pub const FORMATS: [(&str, &str); 14] = [
    ("cr2", "image/x-canon-cr2"),
    ("cr3", "image/x-canon-cr3"),
    ("nef", "image/x-nikon-nef"),
    ("nrw", "image/x-nikon-nrw"),
    ("arw", "image/x-sony-arw"),
    ("sr2", "image/x-sony-sr2"),
    ("srf", "image/x-sony-srf"),
    ("dng", "image/x-adobe-dng"),
    ("raf", "image/x-fuji-raf"),
    ("orf", "image/x-olympus-orf"),
    ("rw2", "image/x-panasonic-rw2"),
    ("pef", "image/x-pentax-pef"),
    ("srw", "image/x-samsung-srw"),
    ("iiq", "image/x-phaseone-iiq"),
];

// Sample input: "dir/photo.SRW" resolves to image/x-samsung-srw; ".srw" has no MIME suffix stem.
pub fn mime_for_name(name: &str) -> Option<&'static str> {
    let base = name.rsplit('/').next().unwrap_or(name);
    let (stem, ext) = base.rsplit_once('.')?;
    if stem.is_empty() {
        return None;
    }
    FORMATS
        .iter()
        .find(|(known, _)| ext.eq_ignore_ascii_case(known))
        .map(|(_, mime)| *mime)
}

// Sample input: "dir/.NEF" remains a RAW-reader candidate, as does "dir/photo.nef".
pub fn is_raw_name(name: &str) -> bool {
    let base = name.rsplit('/').next().unwrap_or(name);
    let Some((_, ext)) = base.rsplit_once('.') else {
        return false;
    };
    FORMATS
        .iter()
        .any(|(known, _)| ext.eq_ignore_ascii_case(known))
}
