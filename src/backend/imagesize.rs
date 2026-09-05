// Pixel dimensions read from a file header, because the preview column names them and the wire does
// not carry them. Only the first bytes are read: no image is ever decoded to answer this.
use crate::backend::regfile::open_regular;
use std::io::Read;
use std::path::Path;

// Enough for a JPEG's segment chain to reach its first frame header on every file this box produces.
const PROBE: usize = 8192;

// PNG's IHDR width and height sit at a fixed offset, so the whole answer is in the first 24 bytes.
const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";
const GIF_MAGIC: &[u8] = b"GIF8";
const BMP_MAGIC: &[u8] = b"BM";
const RIFF_MAGIC: &[u8] = b"RIFF";
const WEBP_MAGIC: &[u8] = b"WEBP";

pub fn dimensions(path: &Path) -> Option<(u32, u32)> {
    let mut buf = vec![0u8; PROBE];
    let mut f = open_regular(path)?;
    let n = f.read(&mut buf).ok()?;
    buf.truncate(n);
    from_header(&buf)
}

pub fn from_header(b: &[u8]) -> Option<(u32, u32)> {
    if b.starts_with(PNG_MAGIC) {
        return png(b);
    }
    if b.starts_with(&[0xFF, 0xD8]) {
        return jpeg(b);
    }
    if b.starts_with(GIF_MAGIC) {
        return gif(b);
    }
    if b.starts_with(BMP_MAGIC) {
        return bmp(b);
    }
    if b.len() > 12 && b.starts_with(RIFF_MAGIC) && &b[8..12] == WEBP_MAGIC {
        return webp(b);
    }
    None
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at + 4)?;
    Some(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

fn le32(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at + 4)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn le16(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at + 2)?;
    Some(u16::from_le_bytes([s[0], s[1]]) as u32)
}

fn be16(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at + 2)?;
    Some(u16::from_be_bytes([s[0], s[1]]) as u32)
}

// Sample input: 89 50 4E 47 0D 0A 1A 0A | 00 00 00 0D "IHDR" | width(4) height(4)
fn png(b: &[u8]) -> Option<(u32, u32)> {
    if b.get(12..16)? != b"IHDR" {
        return None;
    }
    Some((be32(b, 16)?, be32(b, 20)?))
}

// Sample input: FF D8 | FF C0 <len:2> <precision:1> <height:2> <width:2> ..., with any number of
// other FF xx segments before that frame header.
fn jpeg(b: &[u8]) -> Option<(u32, u32)> {
    let mut i = 2;
    while i + 9 < b.len() {
        if b[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = b[i + 1];
        // A frame header, except the four that are not: DHT, JPG, DAC and the restart markers.
        let is_frame = (0xC0..=0xCF).contains(&marker)
            && marker != 0xC4
            && marker != 0xC8
            && marker != 0xCC;
        if is_frame {
            return Some((be16(b, i + 7)?, be16(b, i + 5)?));
        }
        let len = be16(b, i + 2)? as usize;
        if len < 2 {
            return None;
        }
        i += 2 + len;
    }
    None
}

// Sample input: "GIF89a" | width(2, little endian) | height(2, little endian)
fn gif(b: &[u8]) -> Option<(u32, u32)> {
    Some((le16(b, 6)?, le16(b, 8)?))
}

// Sample input: "BM" ... | width(4, little endian, signed) at 18 | height(4) at 22, height may be negative for a top-down bitmap.
fn bmp(b: &[u8]) -> Option<(u32, u32)> {
    let w = le32(b, 18)? as i32;
    let h = le32(b, 22)? as i32;
    Some((w.unsigned_abs(), h.unsigned_abs()))
}

// Sample input: "RIFF" <size:4> "WEBP" then one of "VP8 ", "VP8L" or "VP8X", each carrying the size differently.
fn webp(b: &[u8]) -> Option<(u32, u32)> {
    match b.get(12..16)? {
        b"VP8 " => {
            // The lossy frame header: a 3-byte start code, then 14-bit width and height.
            let w = le16(b, 26)? & 0x3FFF;
            let h = le16(b, 28)? & 0x3FFF;
            Some((w, h))
        }
        b"VP8L" => {
            // Lossless packs both as 14-bit values across four bytes after a one-byte signature.
            let bits = le32(b, 21)?;
            Some(((bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1))
        }
        b"VP8X" => {
            // The extended header stores each dimension minus one as three little-endian bytes.
            let w = u32::from(*b.get(24)?) | u32::from(*b.get(25)?) << 8 | u32::from(*b.get(26)?) << 16;
            let h = u32::from(*b.get(27)?) | u32::from(*b.get(28)?) << 8 | u32::from(*b.get(29)?) << 16;
            Some((w + 1, h + 1))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::fifotest::{mkfifo, peek, within, FifoWriter};
    use crate::backend::testdir::TestDir;
    use std::path::PathBuf;

    fn png_header(w: u32, h: u32) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(PNG_MAGIC);
        v.extend_from_slice(&13u32.to_be_bytes());
        v.extend_from_slice(b"IHDR");
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&h.to_be_bytes());
        v
    }

    #[test]
    fn a_png_reports_its_ihdr_dimensions() {
        assert_eq!(from_header(&png_header(2560, 1440)), Some((2560, 1440)));
        assert_eq!(from_header(&png_header(1, 1)), Some((1, 1)));
    }

    #[test]
    fn a_jpeg_walks_its_segments_to_the_frame_header() {
        // FFD8, then an APP0 segment of 16 bytes, then SOF0 carrying 1920 x 1080.
        let mut v = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        v.extend_from_slice(&[0u8; 14]);
        v.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08]);
        v.extend_from_slice(&1080u16.to_be_bytes());
        v.extend_from_slice(&1920u16.to_be_bytes());
        v.extend_from_slice(&[0u8; 8]);
        assert_eq!(from_header(&v), Some((1920, 1080)));
    }

    #[test]
    fn a_jpeg_does_not_mistake_a_huffman_table_for_a_frame() {
        // FFC4 is DHT, which is inside the 0xC0..0xCF range and is not a frame header.
        let mut v = vec![0xFF, 0xD8, 0xFF, 0xC4, 0x00, 0x06, 0, 0, 0, 0];
        v.extend_from_slice(&[0xFF, 0xC2, 0x00, 0x11, 0x08]);
        v.extend_from_slice(&600u16.to_be_bytes());
        v.extend_from_slice(&800u16.to_be_bytes());
        v.extend_from_slice(&[0u8; 8]);
        assert_eq!(from_header(&v), Some((800, 600)), "the progressive frame FFC2 is the answer");
    }

    #[test]
    fn a_gif_and_a_bmp_read_their_little_endian_fields() {
        let mut gif = b"GIF89a".to_vec();
        gif.extend_from_slice(&320u16.to_le_bytes());
        gif.extend_from_slice(&200u16.to_le_bytes());
        assert_eq!(from_header(&gif), Some((320, 200)));

        let mut bmp = b"BM".to_vec();
        bmp.extend_from_slice(&[0u8; 16]);
        bmp.extend_from_slice(&640i32.to_le_bytes());
        // A top-down bitmap stores a negative height, and the answer is still 480 rows.
        bmp.extend_from_slice(&(-480i32).to_le_bytes());
        assert_eq!(from_header(&bmp), Some((640, 480)));
    }

    #[test]
    fn a_truncated_or_unknown_header_is_none_rather_than_a_panic() {
        assert_eq!(from_header(&[]), None);
        assert_eq!(from_header(PNG_MAGIC), None, "the magic alone carries no IHDR");
        assert_eq!(from_header(&png_header(1, 1)[..20]), None, "a header cut mid-field");
        assert_eq!(from_header(b"GIF8"), None);
        assert_eq!(from_header(b"not an image at all"), None);
        assert_eq!(from_header(&[0xFF, 0xD8]), None, "a jpeg with no segments");
        // A segment claiming a zero length would otherwise loop forever.
        assert_eq!(from_header(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x00, 0, 0, 0, 0, 0, 0]), None);
    }

    #[test]
    fn a_file_on_disk_is_read_without_decoding_it() {
        let d = TestDir::new("imagesize");
        let p = d.join("shot.png");
        std::fs::write(&p, png_header(1234, 567)).unwrap();
        assert_eq!(dimensions(&p), Some((1234, 567)));
        assert_eq!(dimensions(&d.join("missing.png")), None);
        assert_eq!(dimensions(&d.file("notes.txt", "plain text")), None);
    }

    // Measured on a thread with a bound, because the defect under test is an open that never returns.
    fn measured(path: PathBuf) -> Option<(u32, u32)> {
        within("dimensions", move || dimensions(&path))
    }

    #[test]
    fn nothing_but_a_regular_file_is_ever_opened_to_be_measured() {
        let d = TestDir::new("imagesizekinds");
        let fifo = d.join("pipe");
        mkfifo(&fifo);
        assert_eq!(measured(fifo), None, "opening a fifo with no writer never returns");
        std::os::unix::fs::symlink("pipe", d.join("topipe")).unwrap();
        assert_eq!(measured(d.join("topipe")), None, "the open follows the link, so the check has to as well");
        assert_eq!(measured(d.dir("sub")), None, "a directory has no header to read");
        let sock = d.join("sock");
        let _listener = std::os::unix::net::UnixListener::bind(&sock).unwrap();
        assert_eq!(measured(sock), None, "a socket is not a file to measure");
        // The guard is this narrow so a real image is still measured, which is what this file is for.
        let png = d.join("shot.png");
        std::fs::write(&png, png_header(1234, 567)).unwrap();
        assert_eq!(measured(png), Some((1234, 567)));
        // And through a link to one, the case that separates the two candidate calls: the open follows it.
        std::os::unix::fs::symlink("shot.png", d.join("toshot")).unwrap();
        assert_eq!(measured(d.join("toshot")), Some((1234, 567)));
    }

    // A real GIF header, so a drained pipe would answer 8224 by 8481 rather than an unreadable None.
    const GIF_IN_A_PIPE: &str = "GIF89a  !!";

    #[test]
    fn a_fifo_that_has_a_writer_keeps_every_byte_a_measurement_did_not_read() {
        let d = TestDir::new("imagesizefed");
        let p = d.join("pipe");
        mkfifo(&p);
        // feeding returns only once the bytes are in the pipe, or this case degenerates into the writerless one.
        let mut writer = FifoWriter::feeding(&p, GIF_IN_A_PIPE, &d.join("wrote"));
        let seen = measured(p.clone());
        let left = peek(p);
        let stopped = writer.stop();
        assert_eq!(seen, None, "a pipe is not a file to measure, whoever is filling it");
        assert_eq!(left, GIF_IN_A_PIPE.as_bytes(), "a measurement must never eat a pipe someone else is reading");
        assert!(stopped, "the writer this test started is killed by its own pid");
    }
}
