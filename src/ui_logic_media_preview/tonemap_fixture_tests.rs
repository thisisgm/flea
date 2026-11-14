//! One PQ and one HLG frame of tests/fixtures/hdr-tonemap through the helper's own chain in libavfilter, against libplacebo's rendering.
use super::tonemap::{filter, DEFAULT_PEAK_NITS};
use std::path::{Path, PathBuf};
use std::process::Command;

const W: u32 = 256;
const H: u32 = 144;
/// The mapped frame lands at least this close to the reference, in PSNR over RGB.
const MAPPED_MIN_DB: f64 = 22.;
/// Without the curve everything above SDR white clips, so the frame stays at least this far from the reference.
const UNMAPPED_MAX_DB: f64 = 16.;
/// The share of pixels with every channel at 255 the mapped frame may have beyond the reference's.
const CLIPPED_SLACK: f64 = 0.01;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/hdr-tonemap")
        .join(name)
}

/// The first frame of a file after a filter graph, as packed 8-bit RGB.
fn rgb(input: &Path, graph: &str) -> Vec<u8> {
    let output = Command::new("ffmpeg")
        .args(["-nostdin", "-v", "error", "-i"])
        .arg(input)
        .args(["-frames:v", "1", "-vf", graph])
        .args(["-pix_fmt", "rgb24", "-f", "rawvideo", "-"])
        .output()
        .expect("ffmpeg with zscale runs this check");
    assert!(
        output.status.success(),
        "ffmpeg failed on {}: {}",
        input.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.stdout.len(),
        (W * H * 3) as usize,
        "{}",
        input.display()
    );
    output.stdout
}

fn psnr(test: &[u8], reference: &[u8]) -> f64 {
    let sum: f64 = test
        .iter()
        .zip(reference)
        .map(|(a, b)| (f64::from(*a) - f64::from(*b)).powi(2))
        .sum();
    let mse = sum / test.len() as f64;
    10. * (255f64.powi(2) / mse.max(1e-9)).log10()
}

fn clipped(frame: &[u8]) -> f64 {
    let white = frame
        .chunks(3)
        .filter(|p| p.iter().all(|&c| c == 255))
        .count();
    white as f64 / (frame.len() / 3) as f64
}

/// The chain the helper sets for this frame size at the fixtures' 1000-nit peak, without the lavfi wrapper.
fn mapped_chain() -> String {
    let vf = filter(Some((W, H)), DEFAULT_PEAK_NITS);
    vf.strip_prefix("lavfi=[")
        .and_then(|graph| graph.strip_suffix(']'))
        .unwrap_or_else(|| panic!("filter() no longer wraps its graph in lavfi=[...]: {vf}"))
        .to_owned()
}

/// The conversion with no tone map: 0.4.2's PQ chain, and what the software renderer does for HLG.
fn unmapped_chain() -> String {
    format!("zscale=w={W}:h={H}:f=lanczos:d=random:t=bt709:p=bt709:m=gbr:r=pc,format=gbrp,setsar=1")
}

fn check(transfer: &str) {
    let clip = fixture(&format!("{transfer}.mkv"));
    let reference = rgb(&fixture(&format!("{transfer}-ref.png")), "null");
    let mapped = rgb(&clip, &mapped_chain());
    let unmapped = rgb(&clip, &unmapped_chain());
    let (near, far) = (psnr(&mapped, &reference), psnr(&unmapped, &reference));
    assert!(
        near >= MAPPED_MIN_DB,
        "{transfer}: the helper's chain is {near:.2} dB from the reference, under {MAPPED_MIN_DB} dB"
    );
    assert!(
        clipped(&mapped) <= clipped(&reference) + CLIPPED_SLACK,
        "{transfer}: the helper's chain clips {:.3} of the frame, the reference {:.3}",
        clipped(&mapped),
        clipped(&reference)
    );
    assert!(
        far < UNMAPPED_MAX_DB,
        "{transfer}: the unmapped conversion reaches {far:.2} dB, so the fixture no longer tells the two apart"
    );
}

#[test]
fn pv3_a_pq_frame_through_the_helper_chain_lands_near_its_reference() {
    check("pq");
}

#[test]
fn pv3_an_hlg_frame_through_the_helper_chain_lands_near_its_reference() {
    check("hlg");
}
