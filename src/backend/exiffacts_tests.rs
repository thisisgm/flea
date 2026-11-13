use super::*;
use crate::backend::testdir::TestDir;

// One IFD entry: tag, EXIF type (2 ASCII, 3 SHORT, 4 LONG, 5 RATIONAL), element count and the value bytes in the file's byte order.
struct Tag(u16, u16, u32, Vec<u8>);

fn put16(little: bool, v: u16) -> [u8; 2] { if little { v.to_le_bytes() } else { v.to_be_bytes() } }
fn put32(little: bool, v: u32) -> [u8; 4] { if little { v.to_le_bytes() } else { v.to_be_bytes() } }
fn ascii(tag: u16, text: &str) -> Tag {
    let mut v = text.as_bytes().to_vec();
    v.push(0);
    Tag(tag, 2, v.len() as u32, v)
}
fn rational(little: bool, tag: u16, n: u32, d: u32) -> Tag {
    let mut v = put32(little, n).to_vec();
    v.extend_from_slice(&put32(little, d));
    Tag(tag, 5, 1, v)
}
fn short(little: bool, tag: u16, value: u16) -> Tag { Tag(tag, 3, 1, put16(little, value).to_vec()) }

// A TIFF block: header, IFD0 holding `ifd0` (and a pointer to the Exif IFD when `exif` is non-empty), the Exif IFD, then the out-of-line values.
fn tiff(little: bool, ifd0: Vec<Tag>, exif: Vec<Tag>) -> Vec<u8> {
    let has_exif = !exif.is_empty();
    let ifd0_len = 2 + 12 * (ifd0.len() + has_exif as usize) + 4;
    let exif_len = if has_exif { 2 + 12 * exif.len() + 4 } else { 0 };
    let exif_at = 8 + ifd0_len;
    let mut data = Vec::new();
    let data_at = 8 + ifd0_len + exif_len;
    let ifd = |tags: &[Tag], extra: Option<Tag>, out: &mut Vec<u8>, data: &mut Vec<u8>| {
        let all: Vec<&Tag> = tags.iter().chain(extra.iter()).collect();
        out.extend_from_slice(&put16(little, all.len() as u16));
        for Tag(id, ty, count, bytes) in all {
            out.extend_from_slice(&put16(little, *id));
            out.extend_from_slice(&put16(little, *ty));
            out.extend_from_slice(&put32(little, *count));
            if bytes.len() <= 4 {
                out.extend_from_slice(bytes);
                out.extend(std::iter::repeat_n(0, 4 - bytes.len()));
            } else {
                out.extend_from_slice(&put32(little, (data_at + data.len()) as u32));
                data.extend_from_slice(bytes);
            }
        }
        out.extend_from_slice(&[0, 0, 0, 0]);
    };
    let mut out = if little { b"II".to_vec() } else { b"MM".to_vec() };
    out.extend_from_slice(&put16(little, 42));
    out.extend_from_slice(&put32(little, 8));
    let pointer = has_exif.then(|| Tag(0x8769, 4, 1, put32(little, exif_at as u32).to_vec()));
    ifd(&ifd0, pointer, &mut out, &mut data);
    if has_exif {
        ifd(&exif, None, &mut out, &mut data);
    }
    out.extend_from_slice(&data);
    out
}

// The board's RAW photo, as a camera's Exif IFD holds it.
fn exif_tags(little: bool) -> Vec<Tag> {
    vec![rational(little, 0x829A, 1, 250), rational(little, 0x829D, 28, 10), short(little, 0x8827, 400),
         ascii(0x9003, "2026:09:12 18:31:07"), ascii(0xA434, "RF35mm F1.8 MACRO IS STM")]
}
fn camera_tags() -> Vec<Tag> { vec![ascii(0x010F, "Canon"), ascii(0x0110, "Canon EOS R6")] }

fn canon_r6() -> Photo {
    Photo { make: "Canon".into(), model: "Canon EOS R6".into(), lens: "RF35mm F1.8 MACRO IS STM".into(), shutter: (1, 250), fnumber: (28, 10), iso: 400,
            taken: "2026:09:12 18:31:07".into() }
}

fn jpeg_with(exif: &[u8]) -> Vec<u8> {
    let mut v = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
    v.extend_from_slice(&[0u8; 14]);
    v.extend_from_slice(&[0xFF, 0xE1]);
    v.extend_from_slice(&((exif.len() + 8) as u16).to_be_bytes());
    v.extend_from_slice(b"Exif\0\0");
    v.extend_from_slice(exif);
    v.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08, 0x0B, 0xB8, 0x0F, 0xA0, 0x03]);
    v.extend_from_slice(&[0u8; 8]);
    v
}

fn bx(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut v = ((body.len() + 8) as u32).to_be_bytes().to_vec();
    v.extend_from_slice(kind);
    v.extend_from_slice(body);
    v
}
const CANON_UUID: [u8; 16] = [0x85, 0xc0, 0xb6, 0x87, 0x82, 0x0f, 0x11, 0xe0, 0x81, 0x11, 0xf4, 0xce, 0x46, 0x2b, 0x6a, 0x48];

// A CR3: ftyp crx, a moov holding the Canon uuid with its CMT boxes, then an mdat that must never be read.
fn cr3(cmt1: &[u8], cmt2: &[u8], cmt3: &[u8]) -> Vec<u8> {
    let mut canon = CANON_UUID.to_vec();
    canon.extend(bx(b"CNCV", b"CanonCR3_001/00.09.00/00.00.00"));
    canon.extend(bx(b"CMT1", cmt1));
    canon.extend(bx(b"CMT2", cmt2));
    canon.extend(bx(b"CMT3", cmt3));
    let mut v = bx(b"ftyp", b"crx \0\0\0\x01crx isom");
    v.extend(bx(b"moov", &[bx(b"mvhd", &[0u8; 100]), bx(b"uuid", &canon)].concat()));
    v.extend(bx(b"mdat", &[0u8; 64]));
    v
}

fn read_bytes(tag: &str, name: &str, bytes: &[u8]) -> Photo {
    let d = TestDir::new(tag);
    let p = d.join(name);
    std::fs::write(&p, bytes).unwrap();
    read(&p)
}

#[test]
fn a_jpegs_exif_gives_every_fact_the_board_draws() {
    let block = tiff(true, camera_tags(), exif_tags(true));
    assert_eq!(read_bytes("exifjpeg", "shot.jpg", &jpeg_with(&block)), canon_r6());
}

#[test]
fn the_exif_block_is_read_in_either_byte_order() {
    let block = tiff(false, camera_tags(), exif_tags(false));
    assert_eq!(read_bytes("exifbig", "shot.jpg", &jpeg_with(&block)), canon_r6());
}

#[test]
fn a_tiff_based_raw_file_is_read_from_its_own_ifds() {
    for name in ["IMG_0412.CR2", "DSC_1.NEF", "A7.ARW", "x.dng"] {
        let block = tiff(true, camera_tags(), exif_tags(true));
        assert_eq!(read_bytes("exifraw", name, &block), canon_r6(), "{name} is a TIFF and is read by its content, not its name");
    }
}

#[test]
fn a_cr3_is_read_from_its_canon_uuid_boxes() {
    // CMT1 is IFD0 (make and model), CMT2 the Exif IFD as an IFD0 of its own, CMT3 the maker note whose 0x0095 names the lens.
    let cmt1 = tiff(true, camera_tags(), vec![]);
    let mut exif = exif_tags(true);
    exif.pop();
    let cmt2 = tiff(true, exif, vec![]);
    let cmt3 = tiff(true, vec![ascii(0x0095, "RF35mm F1.8 MACRO IS STM")], vec![]);
    assert_eq!(read_bytes("exifcr3", "IMG_0412.CR3", &cr3(&cmt1, &cmt2, &cmt3)), canon_r6());
}

#[test]
fn a_cr3_whose_exif_ifd_names_the_lens_needs_no_maker_note() {
    let cmt1 = tiff(true, camera_tags(), vec![]);
    let cmt2 = tiff(true, exif_tags(true), vec![]);
    assert_eq!(read_bytes("exifcr3lens", "a.cr3", &cr3(&cmt1, &cmt2, &[])).lens, "RF35mm F1.8 MACRO IS STM");
}

#[test]
fn a_file_without_exif_has_no_facts() {
    assert_eq!(read_bytes("exifnone", "plain.jpg", &jpeg_with(&[])), Photo::default());
    let bare = [vec![0xFF, 0xD8, 0xFF, 0xDA, 0x00, 0x02], vec![0u8; 16]].concat();
    assert_eq!(read_bytes("exifbare", "bare.jpg", &bare), Photo::default());
    assert_eq!(read_bytes("exifpng", "a.png", b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01"), Photo::default());
    assert_eq!(read_bytes("exifempty", "empty.cr3", b""), Photo::default());
    assert_eq!(read_bytes("exiftext", "notes.txt", b"hello\n"), Photo::default());
}

#[test]
fn a_photo_with_only_some_facts_keeps_the_rest_empty() {
    let block = tiff(true, vec![ascii(0x0110, "X100V")], vec![rational(true, 0x829D, 56, 10)]);
    let photo = read_bytes("exifsome", "x.jpg", &jpeg_with(&block));
    assert_eq!(photo, Photo { model: "X100V".into(), fnumber: (56, 10), ..Photo::default() });
}

#[test]
fn a_hostile_ifd_ends_the_read_and_never_panics() {
    // The Exif pointer is IFD0's third entry, after Make and Model, so its value field is that entry's last four bytes.
    let pointer_at = 8 + 2 + 12 * 2 + 8;
    let only_the_camera = Photo { make: "Canon".into(), model: "Canon EOS R6".into(), ..Photo::default() };
    for (name, target) in [("past", tiff(true, camera_tags(), exif_tags(true)).len() as u32 + 99), ("far", u32::MAX)] {
        let mut block = tiff(true, camera_tags(), exif_tags(true));
        block[pointer_at..pointer_at + 4].copy_from_slice(&target.to_le_bytes());
        assert_eq!(read_bytes("exifhostile1", "a.jpg", &jpeg_with(&block)), only_the_camera, "an Exif pointer {name} the block keeps IFD0 and drops the rest");
    }
    // exif() follows no pointer, so an Exif pointer back at IFD0 reads that directory once and the read ends.
    let mut looped = tiff(true, camera_tags(), exif_tags(true));
    looped[pointer_at..pointer_at + 4].copy_from_slice(&8u32.to_le_bytes());
    assert_eq!(read_bytes("exifhostile3", "c.jpg", &jpeg_with(&looped)), only_the_camera, "an Exif pointer back at IFD0 ends the read");
    let mut counted = tiff(true, camera_tags(), vec![]);
    counted[8..10].copy_from_slice(&0xFFFFu16.to_le_bytes());
    let _ = read_bytes("exifhostile2", "b.jpg", &jpeg_with(&counted));
    let truncated = jpeg_with(&tiff(true, camera_tags(), exif_tags(true)));
    let _ = read_bytes("exifhostile4", "d.jpg", &truncated[..60]);
    let _ = read_bytes("exifhostile5", "e.cr3", &cr3(&[0xFF; 40], &[], &[])[..90]);
}

#[test]
fn text_is_trimmed_of_padding_and_cut_to_a_line() {
    let long = "L".repeat(500);
    let block = tiff(true, vec![ascii(0x010F, "  Canon \u{7}  "), ascii(0x0110, &long)], vec![ascii(0xA434, "Lens\nname")]);
    let photo = read_bytes("exiftext2", "t.jpg", &jpeg_with(&block));
    assert_eq!(photo.make, "Canon");
    assert_eq!(photo.model.chars().count(), MAX_TEXT, "an attacker's string never reaches the card whole");
    assert_eq!(photo.lens, "Lens name", "a control character is a space and never a line break");
}

#[test]
fn the_read_is_bounded_on_a_file_of_endless_segments() {
    // The Exif segment comes after more empty segments than the budget allows, so only the budget keeps the walk from finding it.
    let block = tiff(true, camera_tags(), exif_tags(true));
    let after = |empty: usize| {
        let mut v = vec![0xFF, 0xD8];
        for _ in 0..empty {
            v.extend_from_slice(&[0xFF, 0xE5, 0x00, 0x02]);
        }
        v.extend_from_slice(&jpeg_with(&block)[2..]);
        v
    };
    assert_eq!(read_bytes("exifendless", "e.jpg", &after(SEGMENT_BUDGET + 6)), Photo::default());
    assert_eq!(read_bytes("exifwithin", "w.jpg", &after(SEGMENT_BUDGET - 4)), canon_r6(), "within the budget the walk still finds it");
}
