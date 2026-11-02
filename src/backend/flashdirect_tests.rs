// The flasher against an O_DIRECT target: every buffer aligned, a ragged tail buffered, in an ext4 root of the worktree.
use crate::backend::flash::{CHUNK_BYTES, DIRECT_ALIGN, WIPE_BYTES};
use crate::backend::flashfake::{DirectDir, O_DIRECT as O_DIRECT_FLAG};
use crate::backend::flashio::direct_flag;
use crate::backend::flashverify::{verify, wipe_ends};
use crate::backend::flashwrite::{write_stream, Fail, Job};
use std::sync::atomic::AtomicBool;

// A ragged image: two whole chunks and a tail that is no block multiple.
const RAGGED_BYTES: usize = 2 * CHUNK_BYTES + 611_393;
const DRIVE_BYTES: u64 = 16_000_000;
// A drive whose last MiB starts off the logical block, so only a rounded-down wipe offset is accepted.
const RAGGED_DRIVE_BYTES: u64 = 16_000_100;
const SEED: u8 = 0x5a;
const FILL: u8 = 0xab;

fn pattern(bytes: usize) -> Vec<u8> {
    (0..bytes).map(|n| (n % 251) as u8 ^ SEED).collect()
}

fn digests(body: &[u8]) -> Vec<u64> {
    body.chunks(CHUNK_BYTES).map(crate::backend::xxh64::hash).collect()
}

fn flash(job: &Job) -> Result<(Vec<u64>, u64), Fail> {
    write_stream(job, &mut |_, _| {})
}

#[test]
fn b3_a_piped_flash_lands_whole_on_an_o_direct_target() {
    let Some(dir) = DirectDir::new("direct-piped") else { return };
    let body = pattern(RAGGED_BYTES);
    std::fs::write(dir.join("in.img"), &body).unwrap();
    let dest = dir.direct_target("drive.img", DRIVE_BYTES);
    let cancel = AtomicBool::new(false);
    let image = dir.join("in.img");
    let mut job = Job::new(&image, "gz", "", &dest, DRIVE_BYTES, &cancel);
    job.decompressor = Some(vec!["/bin/cat".to_string(), dir.join("in.img").to_string_lossy().to_string()]);
    let (table, written) = flash(&job).unwrap_or_else(|f| panic!("the piped flash failed: {f:?}"));
    assert_eq!((written, table), (RAGGED_BYTES as u64, digests(&body)));
    assert_eq!(&std::fs::read(dir.join("drive.img")).unwrap()[..RAGGED_BYTES], &body[..], "every byte landed");
}

#[test]
fn b4_a_raw_image_with_a_ragged_tail_lands_on_an_o_direct_target() {
    let Some(dir) = DirectDir::new("direct-raw") else { return };
    let body = pattern(RAGGED_BYTES);
    std::fs::write(dir.join("in.img"), &body).unwrap();
    let dest = dir.direct_target("drive.img", DRIVE_BYTES);
    let cancel = AtomicBool::new(false);
    let image = dir.join("in.img");
    let (table, written) = flash(&Job::new(&image, "img", "", &dest, DRIVE_BYTES, &cancel)).unwrap_or_else(|f| panic!("the raw flash failed: {f:?}"));
    assert_eq!((written, table), (RAGGED_BYTES as u64, digests(&body)));
    assert_eq!(&std::fs::read(dir.join("drive.img")).unwrap()[..RAGGED_BYTES], &body[..], "the tail landed");
}

#[test]
fn b3_a_verify_reads_an_o_direct_target_through_aligned_buffers() {
    let Some(dir) = DirectDir::new("direct-verify") else { return };
    let body = pattern(RAGGED_BYTES);
    let dest = dir.direct_target("drive.img", DRIVE_BYTES);
    std::fs::write(dir.join("drive.img"), &body).unwrap();
    let cancel = AtomicBool::new(false);
    let out = verify(&dest, &digests(&body), RAGGED_BYTES as u64, &cancel, &mut |_| {});
    assert!(out.is_ok(), "the read-back of a ragged image verifies: {out:?}");
}

#[test]
fn b3_a_verify_still_catches_a_flipped_byte_on_an_o_direct_target() {
    let Some(dir) = DirectDir::new("direct-flip") else { return };
    let mut body = pattern(RAGGED_BYTES);
    let table = digests(&body);
    body[CHUNK_BYTES + 10] ^= 0xff;
    let dest = dir.direct_target("drive.img", DRIVE_BYTES);
    std::fs::write(dir.join("drive.img"), &body).unwrap();
    let cancel = AtomicBool::new(false);
    match verify(&dest, &table, RAGGED_BYTES as u64, &cancel, &mut |_| {}) {
        Err(Fail::Mismatch(at)) => assert_eq!(at, CHUNK_BYTES as u64),
        other => panic!("a flipped byte must name its chunk, got {other:?}"),
    }
}

#[test]
fn b3_a_wipe_zeroes_both_ends_of_an_o_direct_target() {
    for bytes in [DRIVE_BYTES, 16 * 1024 * 1024, RAGGED_DRIVE_BYTES] {
        let Some(dir) = DirectDir::new("direct-wipe") else { return };
        let dest = dir.direct_target("drive.img", bytes);
        std::fs::write(dir.join("drive.img"), vec![FILL; bytes as usize]).unwrap();
        assert!(wipe_ends(&dest, bytes), "the wipe of {bytes} bytes succeeds");
        let body = std::fs::read(dir.join("drive.img")).unwrap();
        let wipe = WIPE_BYTES as usize;
        assert!(body[..wipe].iter().all(|b| *b == 0), "the first MiB is zero");
        assert!(body[body.len() - wipe..].iter().all(|b| *b == 0), "the last MiB is zero");
        // The last wipe starts on a block, so up to one block before the last MiB goes with it.
        assert!(body[wipe..body.len() - wipe - DIRECT_ALIGN].iter().all(|b| *b == FILL), "the middle is untouched");
    }
}

#[test]
fn b7_a_failed_flag_read_is_not_o_direct() {
    assert!(!direct_flag(-1), "F_GETFL answering -1 is no O_DIRECT");
    assert!(direct_flag(O_DIRECT_FLAG), "the flag itself reads as on");
    assert!(!direct_flag(0));
}

#[test]
fn arm_oflags_flash_recognizes_the_target_direct_io_bit() {
    assert!(
        direct_flag(libc::O_DIRECT),
        "the helper's native O_DIRECT must be recognized"
    );
    assert!(
        !direct_flag(libc::O_DIRECTORY),
        "O_DIRECTORY must never authorize a direct I/O descriptor"
    );
    assert_eq!(O_DIRECT_FLAG, libc::O_DIRECT);
}
