use super::*;
use crate::backend::model3d::codecs::crc32;
use crate::backend::testdir::TestDir;
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/backend/imageformats_fixtures")
        .join(name)
}

fn on_disk(dir: &TestDir, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

fn sized(w: u32, h: u32, orientation: u8) -> Option<Header> {
    Some(Header {
        width: w,
        height: h,
        orientation,
    })
}

#[test]
fn each_sweep_fixture_answers_its_size() {
    assert_eq!(header(&fixture("image-tiff.tiff")), sized(320, 240, 1));
    assert_eq!(header(&fixture("image-ico.ico")), sized(48, 36, 1));
    assert_eq!(header(&fixture("image-avif.avif")), sized(320, 240, 1));
    assert_eq!(header(&fixture("image-svg.svg")), sized(120, 80, 1));
    assert_eq!(header(&fixture("image-svgz.svgz")), sized(120, 80, 1));
}

#[test]
fn the_kinds_imagesize_reads_still_come_from_imagesize() {
    let dir = TestDir::new("imageformats-png");
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend_from_slice(&13u32.to_be_bytes());
    png.extend_from_slice(b"IHDR");
    png.extend_from_slice(&7u32.to_be_bytes());
    png.extend_from_slice(&5u32.to_be_bytes());
    assert_eq!(header(&on_disk(&dir, "a.png", &png)), sized(7, 5, 1));
    assert_eq!(header(&on_disk(&dir, "a.txt", b"plain text\n")), None);
}

// Sample: one IFD after the pixel data, entries (tag, type, count, value) in the file's byte order.
fn tiff(little: bool, entries: &[(u16, u16, u32)], pad: usize) -> Vec<u8> {
    let w16 = |v: u16| {
        if little {
            v.to_le_bytes()
        } else {
            v.to_be_bytes()
        }
    };
    let w32 = |v: u32| {
        if little {
            v.to_le_bytes()
        } else {
            v.to_be_bytes()
        }
    };
    let mut b = if little {
        b"II".to_vec()
    } else {
        b"MM".to_vec()
    };
    b.extend_from_slice(&w16(42));
    b.extend_from_slice(&w32(8 + pad as u32));
    b.extend(std::iter::repeat_n(0u8, pad));
    b.extend_from_slice(&w16(entries.len() as u16));
    for &(tag, kind, value) in entries {
        b.extend_from_slice(&w16(tag));
        b.extend_from_slice(&w16(kind));
        b.extend_from_slice(&w32(1));
        if kind == 3 {
            b.extend_from_slice(&w16(value as u16));
            b.extend_from_slice(&[0, 0]);
        } else {
            b.extend_from_slice(&w32(value));
        }
    }
    b.extend_from_slice(&w32(0));
    b
}

#[test]
fn a_tiff_reads_its_first_ifd_in_either_byte_order_past_the_probe() {
    let dir = TestDir::new("imageformats-tiff");
    let le = tiff(true, &[(256, 3, 640), (257, 4, 480)], 20_000);
    assert_eq!(header(&on_disk(&dir, "le.tiff", &le)), sized(640, 480, 1));
    let be = tiff(false, &[(256, 4, 70_000), (257, 3, 30), (274, 3, 6)], 0);
    assert_eq!(header(&on_disk(&dir, "be.tif", &be)), sized(70_000, 30, 6));
}

#[test]
fn a_tiff_missing_a_side_or_naming_a_bad_orientation_is_handled() {
    let dir = TestDir::new("imageformats-tiff-bad");
    assert_eq!(
        header(&on_disk(&dir, "w.tiff", &tiff(true, &[(256, 3, 640)], 0))),
        None
    );
    let odd = tiff(true, &[(256, 3, 4), (257, 3, 3), (274, 3, 9)], 0);
    assert_eq!(header(&on_disk(&dir, "o.tiff", &odd)), sized(4, 3, 1));
    let mut cut = tiff(true, &[(256, 3, 4), (257, 3, 3)], 0);
    cut.truncate(12);
    assert_eq!(header(&on_disk(&dir, "cut.tiff", &cut)), None);
}

// Sample: 00 00 01 00 count(2) | per entry: w h colours 0 planes(2) bpp(2) size(4) offset(4)
fn ico(entries: &[(u8, u8, u16, Vec<u8>)]) -> Vec<u8> {
    let mut b = vec![0, 0, 1, 0];
    b.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    let mut at = 6 + 16 * entries.len() as u32;
    for (w, h, bpp, data) in entries {
        b.extend_from_slice(&[*w, *h, 0, 0, 1, 0]);
        b.extend_from_slice(&bpp.to_le_bytes());
        b.extend_from_slice(&(data.len() as u32).to_le_bytes());
        b.extend_from_slice(&at.to_le_bytes());
        at += data.len() as u32;
    }
    for (_, _, _, data) in entries {
        b.extend_from_slice(data);
    }
    b
}

fn png_entry(w: u32, h: u32) -> Vec<u8> {
    let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
    v.extend_from_slice(&13u32.to_be_bytes());
    v.extend_from_slice(b"IHDR");
    v.extend_from_slice(&w.to_be_bytes());
    v.extend_from_slice(&h.to_be_bytes());
    v
}

#[test]
fn an_icon_answers_the_entry_the_decoder_picks() {
    let dir = TestDir::new("imageformats-ico");
    // The image crate keeps the deepest colour, then the largest area; zero in the directory means 256.
    let pick = ico(&[
        (16, 16, 32, vec![0; 40]),
        (0, 0, 8, vec![0; 40]),
        (32, 24, 32, vec![0; 40]),
    ]);
    assert_eq!(header(&on_disk(&dir, "a.ico", &pick)), sized(32, 24, 1));
    let big = ico(&[(48, 48, 32, vec![0; 40]), (0, 0, 32, vec![0; 40])]);
    assert_eq!(header(&on_disk(&dir, "b.ico", &big)), sized(256, 256, 1));
    // A PNG entry's own header is the size, past the directory's 256 ceiling.
    let png = ico(&[(16, 16, 32, vec![0; 40]), (0, 0, 32, png_entry(512, 384))]);
    assert_eq!(header(&on_disk(&dir, "c.ico", &png)), sized(512, 384, 1));
    assert_eq!(header(&on_disk(&dir, "none.ico", &ico(&[]))), None);
}

fn bmff(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut b = ((body.len() + 8) as u32).to_be_bytes().to_vec();
    b.extend_from_slice(kind);
    b.extend_from_slice(body);
    b
}

fn full(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut v = vec![0, 0, 0, 0];
    v.extend_from_slice(body);
    bmff(kind, &v)
}

// Sample: ftyp avif | meta { pitm item, iprp { ipco { ispe..., irot... }, ipma item -> props } }
fn avif(primary: u16, props: &[Vec<u8>], links: &[(u16, Vec<u8>)]) -> Vec<u8> {
    let mut ipco = Vec::new();
    for p in props {
        ipco.extend_from_slice(p);
    }
    let mut ipma = (links.len() as u32).to_be_bytes().to_vec();
    for (item, indices) in links {
        ipma.extend_from_slice(&item.to_be_bytes());
        ipma.push(indices.len() as u8);
        ipma.extend_from_slice(indices);
    }
    let mut iprp = bmff(b"ipco", &ipco);
    iprp.extend_from_slice(&full(b"ipma", &ipma));
    let mut meta = full(b"hdlr", b"\0\0\0\0pict\0\0\0\0\0\0\0\0\0\0\0\0\0");
    meta.extend_from_slice(&full(b"pitm", &primary.to_be_bytes()));
    meta.extend_from_slice(&bmff(b"iprp", &iprp));
    let mut file = bmff(b"ftyp", b"avif\0\0\0\0avifmif1");
    file.extend_from_slice(&full(b"meta", &meta));
    file.extend_from_slice(&bmff(b"mdat", &[0; 32]));
    file
}

fn ispe(w: u32, h: u32) -> Vec<u8> {
    let mut v = w.to_be_bytes().to_vec();
    v.extend_from_slice(&h.to_be_bytes());
    full(b"ispe", &v)
}

#[test]
fn a_heif_meta_reply_uses_the_primary_image_dimensions() {
    let dir = TestDir::new("imageformats-heif");
    let mut bytes = avif(
        9,
        &[ispe(64, 32), ispe(400, 300)],
        &[(1, vec![1]), (9, vec![2])],
    );
    bytes[8..12].copy_from_slice(b"heic");
    bytes[16..20].copy_from_slice(b"heic");
    for name in ["a.heic", "b.HEIF"] {
        let path = on_disk(&dir, name, &bytes);
        assert_eq!(header(&path), sized(400, 300, 1), "{name}");
        let meta = crate::backend::metareq::read(&path, true, false, None);
        assert_eq!(
            (meta.width, meta.height, meta.orientation, meta.lines),
            (400, 300, 1, 0)
        );
    }
    let mut disguised = bytes;
    disguised[8..12].copy_from_slice(b"mp42");
    disguised[12..16].copy_from_slice(b"heic");
    disguised[16..24].copy_from_slice(b"mp42mp42");
    assert_eq!(
        header(&on_disk(&dir, "minor.heic", &disguised)),
        None,
        "minor version bytes are not compatible brands"
    );
}

#[test]
fn an_avif_answers_its_primary_items_size_turned_by_irot() {
    let dir = TestDir::new("imageformats-avif");
    let plain = avif(
        1,
        &[ispe(64, 32), ispe(400, 300)],
        &[(2, vec![1]), (1, vec![2])],
    );
    assert_eq!(header(&on_disk(&dir, "a.avif", &plain)), sized(400, 300, 1));
    // libheif applies irot while decoding, so a quarter turn swaps the sides and leaves no orientation.
    let turned = avif(
        1,
        &[ispe(400, 300), bmff(b"irot", &[1])],
        &[(1, vec![0x81, 2])],
    );
    assert_eq!(
        header(&on_disk(&dir, "b.avif", &turned)),
        sized(300, 400, 1)
    );
    let half = avif(
        1,
        &[ispe(400, 300), bmff(b"irot", &[2])],
        &[(1, vec![1, 2])],
    );
    assert_eq!(header(&on_disk(&dir, "c.avif", &half)), sized(400, 300, 1));
    let lost = avif(3, &[ispe(400, 300)], &[(1, vec![1])]);
    assert_eq!(header(&on_disk(&dir, "d.avif", &lost)), None);
}

fn svg_size(text: &str) -> Option<Header> {
    let dir = TestDir::new("imageformats-svg");
    header(&on_disk(&dir, "a.svg", text.as_bytes()))
}

#[test]
fn an_svg_answers_the_size_usvg_gives_its_root() {
    let ns = "xmlns=\"http://www.w3.org/2000/svg\"";
    assert_eq!(
        svg_size(&format!("<svg {ns} width='24' height=\"12px\"/>")),
        sized(24, 12, 1)
    );
    assert_eq!(
        svg_size(&format!("<svg {ns} viewBox='0 0 300,150'></svg>")),
        sized(300, 150, 1)
    );
    assert_eq!(
        svg_size(&format!("<svg {ns} width='50%' viewBox='0 0 300 150'/>")),
        sized(150, 150, 1)
    );
    assert_eq!(
        svg_size(&format!("<svg {ns} width='1in' height='72pt'/>")),
        sized(96, 96, 1)
    );
    assert_eq!(svg_size(&format!("<svg {ns}/>")), sized(100, 100, 1));
    let preamble = format!(
        "<?xml version='1.0'?>\n<!-- <svg width='1' height='1'> -->\n<!DOCTYPE svg>\n<svg\n {ns}\n height=\"40\" width=\"80.4\">"
    );
    assert_eq!(svg_size(&preamble), sized(80, 40, 1));
    assert_eq!(svg_size("<html><body>not a picture</body></html>"), None);
    assert_eq!(
        svg_size(&format!("<svg {ns} width='0' height='10'/>")),
        None
    );
}

// One stored deflate block in a gzip member, which is all the inflate needs to read.
fn gzip(data: &[u8]) -> Vec<u8> {
    let mut b = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 3, 1];
    b.extend_from_slice(&(data.len() as u16).to_le_bytes());
    b.extend_from_slice(&(!(data.len() as u16)).to_le_bytes());
    b.extend_from_slice(data);
    b.extend_from_slice(&crc32(data).to_le_bytes());
    b.extend_from_slice(&(data.len() as u32).to_le_bytes());
    b
}

#[test]
fn an_svgz_inflates_before_its_root_is_read() {
    let dir = TestDir::new("imageformats-svgz");
    let text = b"<svg xmlns='http://www.w3.org/2000/svg' width='33' height='22'/>";
    assert_eq!(
        header(&on_disk(&dir, "a.svgz", &gzip(text))),
        sized(33, 22, 1)
    );
    let mut broken = gzip(text);
    let at = broken.len() - 6;
    broken[at] ^= 0xff;
    assert_eq!(header(&on_disk(&dir, "b.svgz", &broken)), None);
    assert_eq!(header(&on_disk(&dir, "c.svgz", b"\x1f\x8b")), None);
}

#[test]
fn the_meta_reply_carries_each_fixtures_size_and_counts_no_lines() {
    for (name, w, h) in [
        ("image-tiff.tiff", 320, 240),
        ("image-ico.ico", 48, 36),
        ("image-avif.avif", 320, 240),
        ("image-svg.svg", 120, 80),
        ("image-svgz.svgz", 120, 80),
    ] {
        let meta = crate::backend::metareq::read(&fixture(name), true, false, None);
        assert_eq!(
            (meta.width, meta.height, meta.orientation, meta.lines),
            (w, h, 1, 0),
            "{name}"
        );
    }
}

#[test]
fn only_the_names_quick_look_decodes_itself_are_measured_here() {
    let dir = TestDir::new("imageformats-names");
    let tiff_bytes = std::fs::read(fixture("image-tiff.tiff")).unwrap();
    // A camera RAW is TIFF inside; its sensor size comes from the RAW facts, never from IFD0.
    assert_eq!(header(&on_disk(&dir, "camera.dng", &tiff_bytes)), None);
    let svg_text = b"<svg xmlns='http://www.w3.org/2000/svg' width='9' height='9'/>";
    assert_eq!(header(&on_disk(&dir, "notes.txt", svg_text)), None);
    assert_eq!(header(&on_disk(&dir, "page.html", svg_text)), None);
    // Each name is measured only as the kind the card decodes it as.
    assert_eq!(header(&on_disk(&dir, "odd.svg", &tiff_bytes)), None);
    assert_eq!(header(&on_disk(&dir, "odd.avif", &tiff_bytes)), None);
    assert_eq!(header(&on_disk(&dir, "odd.tiff", svg_text)), None);
    let gz = std::fs::read(fixture("image-svgz.svgz")).unwrap();
    assert_eq!(header(&on_disk(&dir, "archive.gz", &gz)), None);
}
