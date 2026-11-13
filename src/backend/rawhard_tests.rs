// Hostile-input sweeps over every shape in rawshape_tests.rs: truncation, hostile size, offset and count fields, IFD limits and bytes read.
use super::shape_tests::shapes;
use super::tests::{fat_jpeg, tiff_one};
use crate::backend::rawbmff::tests::{bx, ftyp, moov_box, prvw_box, prvw_payload, PRVW};
use crate::backend::rawembed::{self, Preview, BLOB_BUDGET, HEADER_BUDGET, SOF_BUDGET};
use crate::backend::rawexif as ex;
use crate::backend::rawraf::tests::raf;
use crate::backend::rawtiff::tests::{jpeg, pair, write, Ev, Tb};
use crate::backend::rawtiff::{inspect_file, walk_ifd, IfdLoc, Oly, SliceSrc, Src, Walk};
use crate::backend::testdir::TestDir;
use std::panic::{catch_unwind, AssertUnwindSafe};

// The preview of the file written to the sandbox, with a panic turned into a failure that names the input.
fn probe(d: &TestDir, what: &str, bytes: &[u8]) -> Option<Preview> {
    let p = write(d, "probe.bin", bytes);
    let got = catch_unwind(AssertUnwindSafe(|| rawembed::inspect(&p)));
    assert!(got.is_ok(), "the walk panicked on {}", what);
    got.unwrap().and_then(|(preview, _)| preview)
}

// A preview the walker names must lie wholly inside the file it was found in.
fn inside(preview: &Option<Preview>, file_len: usize) -> bool {
    preview.as_ref().is_none_or(|p| matches!(p.offset.checked_add(p.length), Some(end) if end <= file_len as u64))
}

#[test]
fn every_shape_cut_at_each_64_bytes_names_no_preview_past_the_end() {
    let d = TestDir::new("rawcut");
    for s in shapes() {
        for cut in (0..s.bytes.len()).step_by(64).chain([s.bytes.len()]) {
            let got = probe(&d, &format!("{} cut at {}", s.kind, cut), &s.bytes[..cut]);
            assert!(inside(&got, cut), "{} cut at {} names {:?}", s.kind, cut, got);
            if cut == s.bytes.len() {
                assert_eq!(got, Some(s.want.clone()), "{} uncut", s.kind);
            }
        }
    }
}

// Every 4-byte window takes u32::MAX, one past the file and the sizes below a box header, every 2-byte window 0xFFFF and one past, in both orders.
#[test]
fn every_count_size_and_offset_field_set_hostile_never_panics_or_leaves_the_file() {
    let d = TestDir::new("rawfields");
    for s in shapes() {
        let past = s.bytes.len() as u32 + 1;
        let mut values: Vec<Vec<u8>> = vec![u32::MAX.to_le_bytes().to_vec(), 0xFFFFu16.to_le_bytes().to_vec()];
        for v in [past, 0, 1, 8, 16, 23] {
            values.push(v.to_le_bytes().to_vec());
            values.push(v.to_be_bytes().to_vec());
        }
        values.push((past as u16).to_le_bytes().to_vec());
        values.push((past as u16).to_be_bytes().to_vec());
        for v in &values {
            for at in 0..=s.bytes.len() - v.len() {
                let mut b = s.bytes.clone();
                b[at..at + v.len()].copy_from_slice(v);
                let got = probe(&d, &format!("{} with {:?} at {}", s.kind, v, at), &b);
                assert!(inside(&got, b.len()), "{} with {:?} at {} names {:?}", s.kind, v, at, got);
            }
        }
    }
}

#[test]
fn a_uuid_box_too_short_for_its_usertype_holds_no_payload() {
    let d = TestDir::new("rawshortuuid");
    let payload = prvw_payload(&jpeg(640, 480));
    // The usertype and a whole PRVW payload follow the header, so only the declared size keeps the payload out of the box.
    let file = |header: &[u8]| {
        let mut f = ftyp();
        f.extend_from_slice(header);
        f.extend_from_slice(&PRVW);
        f.extend_from_slice(&payload);
        f
    };
    let small = |size: u32| [size.to_be_bytes().as_slice(), b"uuid"].concat();
    let large = |size: u64| [1u32.to_be_bytes().as_slice(), b"uuid", size.to_be_bytes().as_slice()].concat();
    let whole = payload.len() as u64 + 16;
    assert!(probe(&d, "an honest uuid box", &file(&small(8 + whole as u32))).is_some(), "the control: an honest size is a preview");
    assert!(probe(&d, "an honest largesize box", &file(&large(16 + whole))).is_some(), "the control: an honest largesize is a preview");
    for size in 8u32..24 {
        assert_eq!(probe(&d, &format!("uuid box of {} bytes", size), &file(&small(size))), None);
    }
    // A largesize box has a 16-byte header, so 16 to 31 bytes is too short for the 16-byte usertype as well.
    for size in 16u64..32 {
        assert_eq!(probe(&d, &format!("largesize uuid box of {} bytes", size), &file(&large(size))), None);
    }
}

#[test]
fn a_length_one_past_the_end_is_no_preview_in_each_container() {
    let d = TestDir::new("rawpastend");
    let pic = jpeg(640, 480);
    // TIFF: the one pair sits at the end of the file and its length names one byte more than is there.
    let mut tiff = tiff_one(38, pic.len() as u32);
    tiff.extend_from_slice(&pic);
    assert!(probe(&d, "tiff", &tiff).is_some(), "the exact length is a preview");
    let mut past = tiff_one(38, pic.len() as u32 + 1);
    past.extend_from_slice(&pic);
    assert_eq!(probe(&d, "tiff", &past), None);
    // RAF: the header's length field names one byte past the end.
    assert_eq!(probe(&d, "raf", &raf(128, pic.len() as u32, &pic)).map(|p| p.length), Some(pic.len() as u64));
    assert_eq!(probe(&d, "raf", &raf(128, pic.len() as u32 + 1, &pic)), None);
    // CR3: the PRVW box is cut one byte short of its declared size.
    let mut f = ftyp();
    f.extend_from_slice(&prvw_box(&pic));
    assert!(probe(&d, "cr3", &f).is_some(), "the whole box holds a preview");
    assert_eq!(probe(&d, "cr3 cut", &f[..f.len() - 1]), None);
}

// A TIFF whose one IFD holds `n` entries: filler first, then the JPEG pair, so the pair is found only if all are walked.
fn tiff_of_entries(n: usize) -> Vec<u8> {
    let pic = jpeg(64, 48);
    let mut t = Tb::new(true);
    let po = t.blob(&pic);
    let mut entries: Vec<(u16, Ev)> = (0..n - 2).map(|i| (0xC000 + i as u16, Ev::T(4, i as u32))).collect();
    entries.extend(pair(po, pic.len()));
    let ifd0 = t.ifd(entries, 0);
    t.set_ifd0(ifd0);
    t.b
}

#[test]
fn an_ifd_walks_700_and_1024_entries_and_skips_1025() {
    let d = TestDir::new("rawentries");
    for n in [700, 1024] {
        assert!(probe(&d, &format!("{} entries", n), &tiff_of_entries(n)).is_some(), "{} entries all walk", n);
    }
    assert_eq!(probe(&d, "1025 entries", &tiff_of_entries(1025)), None, "past the cap the IFD is skipped");
}

// A chain of `n` IFDs, each holding the same pair, the last pointing at `last_next`.
fn chain_of(n: u32, last_next: Option<u32>) -> Vec<u8> {
    let pic = jpeg(64, 48);
    let mut t = Tb::new(true);
    let po = t.blob(&pic);
    let first = t.b.len() as u32;
    let size = 2 + 2 * 12 + 4;
    for i in 0..n {
        let next = if i + 1 < n { first + (i + 1) * size } else { last_next.unwrap_or(0) };
        t.ifd(pair(po, pic.len()), next);
    }
    t.set_ifd0(first);
    t.b
}

fn candidates_of(d: &TestDir, bytes: &[u8]) -> Vec<(u64, u64)> {
    let p = write(d, "chain.tif", bytes);
    let mut ctx = rawembed::Ctx::open(&p).expect("opens");
    inspect_file(&mut ctx).expect("walks").0
}

#[test]
fn a_self_referencing_ifd_and_a_long_chain_end_at_the_visited_set_and_the_cap() {
    let d = TestDir::new("rawchain");
    let own = 8 + jpeg(64, 48).len() as u32;
    assert_eq!(candidates_of(&d, &chain_of(1, Some(own))).len(), 1, "an IFD naming itself is walked once");
    assert_eq!(candidates_of(&d, &chain_of(3, Some(own))).len(), 3, "a tail naming the head is walked once each");
    assert_eq!(candidates_of(&d, &chain_of(100, None)).len(), 64, "a long chain stops at the IFD cap");
}

// Sample input: "rchar: 2837" among the "name: count" rows of /proc/thread-self/io.
// The kernel's per-thread count of bytes read(2) returned, so the figure is not the walker's own bookkeeping.
fn bytes_read() -> u64 {
    let io = std::fs::read_to_string("/proc/thread-self/io").expect("per-thread I/O accounting is on");
    io.lines().find_map(|l| l.strip_prefix("rchar: ")).expect("an rchar row").trim().parse().expect("a count")
}

const IFDS: usize = 64;
const ENTRIES: usize = 1024;
const IFD_LEN: usize = 2 + ENTRIES * 12 + 4;

// 64 full IFDs: two entries name a fat preview, the rest are 64-byte notes the walk reads and skips.
fn hungry_tiff(magic: &[u8; 4], le: bool) -> Vec<u8> {
    let w16 = |v: u16| if le { v.to_le_bytes() } else { v.to_be_bytes() };
    let w32 = |v: u32| if le { v.to_le_bytes() } else { v.to_be_bytes() };
    let pic = fat_jpeg(640, 480, 4);
    let pic_at = 8 + IFDS * IFD_LEN;
    let mut b = magic.to_vec();
    b.extend_from_slice(&w32(8));
    for k in 0..IFDS {
        b.extend_from_slice(&w16(ENTRIES as u16));
        for i in 0..ENTRIES {
            let (tag, typ, cnt, val) = match i {
                0 => (ex::JPEG_OFF, 4, 1, pic_at as u32),
                1 => (ex::JPEG_LEN, 4, 1, pic.len() as u32),
                _ => (ex::MAKER_NOTE, 7, 64, 8),
            };
            b.extend_from_slice(&w16(tag));
            b.extend_from_slice(&w16(typ));
            b.extend_from_slice(&w32(cnt));
            b.extend_from_slice(&w32(val));
        }
        let next = if k + 1 < IFDS { 8 + (k + 1) * IFD_LEN } else { 0 };
        b.extend_from_slice(&w32(next as u32));
    }
    b.extend_from_slice(&pic);
    b
}

// One moov box of 32 maximal CMT boxes, then 31 PRVW boxes that each name a 60 KB preview.
fn hungry_cr3() -> Vec<u8> {
    let pic = fat_jpeg(640, 480, 1);
    let cmts: Vec<u8> = (0..32).flat_map(|_| bx(b"CMT1", &vec![0u8; 262_144])).collect();
    let mut f = ftyp();
    f.extend_from_slice(&moov_box(&cmts));
    for _ in 0..31 {
        f.extend_from_slice(&prvw_box(&pic));
    }
    f
}

fn hungry_files() -> Vec<(&'static str, Vec<u8>, u64)> {
    let tiffs: [(&str, &[u8; 4], bool); 9] = [
        ("cr2", b"II*\x00", true),
        ("nef", b"MM\x00*", false),
        ("nrw", b"II*\x00", true),
        ("arw", b"II*\x00", true),
        ("sr2", b"II*\x00", true),
        ("dng", b"II*\x00", true),
        ("pef", b"MM\x00*", false),
        ("orf", b"IIRO", true),
        ("rw2", b"IIU\x00", true),
    ];
    let mut all: Vec<_> = tiffs.iter().map(|&(k, m, le)| (k, hungry_tiff(m, le), 3 * HEADER_BUDGET as u64 / 2)).collect();
    all.push(("cr3", hungry_cr3(), 3 * HEADER_BUDGET as u64 / 2));
    let pic = fat_jpeg(640, 480, 4);
    all.push(("raf", raf(128, pic.len() as u32, &pic), 400_000));
    all
}

// The two reads of /proc/thread-self/io that bracket each call.
const SLACK: u64 = 512;

#[test]
fn no_kind_reads_more_than_its_three_budgets_from_the_file() {
    let d = TestDir::new("rawbytes");
    let bound = (HEADER_BUDGET + BLOB_BUDGET + SOF_BUDGET) as u64 + SLACK;
    for (kind, bytes, floor) in hungry_files() {
        let p = write(&d, &format!("hungry.{}", kind), &bytes);
        let before = bytes_read();
        let got = rawembed::inspect(&p);
        let read = bytes_read() - before;
        assert!(got.is_some(), "{} is still recognised", kind);
        assert!(read <= bound, "{} read {} bytes against a bound of {}", kind, read, bound);
        assert!(read >= floor, "{} read only {}: the fixture is not hungry", kind, read);
    }
}

#[test]
fn a_walk_in_hand_copies_no_more_than_the_bytes_it_holds() {
    struct Counted<'a> {
        inner: SliceSrc<'a>,
        copied: usize,
    }
    impl Src for Counted<'_> {
        fn get(&mut self, off: u64, n: usize) -> Option<Vec<u8>> {
            let v = self.inner.get(off, n)?;
            self.copied += v.len();
            Some(v)
        }
    }
    // 1024 entries, each naming the same 32767-value SHORT array, under a tag no first-wins rule ever skips.
    let values_at = 8 + IFD_LEN;
    let mut b = b"II*\x00\x08\x00\x00\x00".to_vec();
    b.extend_from_slice(&(ENTRIES as u16).to_le_bytes());
    for _ in 0..ENTRIES {
        b.extend_from_slice(&ex::JPEG_OFF.to_le_bytes());
        b.extend_from_slice(&3u16.to_le_bytes());
        b.extend_from_slice(&32_767u32.to_le_bytes());
        b.extend_from_slice(&(values_at as u32).to_le_bytes());
    }
    b.extend_from_slice(&0u32.to_le_bytes());
    b.resize(values_at + 65_534, 0);
    let mut src = Counted { inner: SliceSrc::new(&b), copied: 0 };
    let loc = IfdLoc { base: 0, little_endian: true, depth: 0, in_sub_ifd: false, index_in_chain: 0, is_exif: false, olympus: Oly::No };
    let _ = walk_ifd(&mut src, &mut Walk::new(), &loc, 8);
    assert!(src.copied <= b.len(), "copied {} bytes out of {}", src.copied, b.len());
    // The floor: one whole 32767-value array was read, so the cap above bounds the copy path and not an early skip.
    assert!(src.copied >= 65_534, "copied only {}: the fixture is not hungry", src.copied);
}
