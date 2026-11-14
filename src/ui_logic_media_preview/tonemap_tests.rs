use super::tonemap::*;
use std::time::Duration;

fn with(source: Source) -> Tonemap {
    let mut tonemap = Tonemap::default();
    tonemap.source = source;
    tonemap
}

fn pq_4k() -> Tonemap {
    with(Source {
        hdr: true,
        width: 3840,
        height: 2160,
        rotated: false,
        peak_nits: 0,
    })
}

#[test]
fn pv3_pq_and_hlg_are_tone_mapped_sdr_and_unknown_are_not() {
    for gamma in ["pq", "hlg"] {
        assert!(hdr(gamma), "{gamma}");
    }
    for other in ["bt.1886", "srgb", "linear", "gamma2.2", "auto", "", "PQ"] {
        assert!(!hdr(other), "{other}");
    }
}

#[test]
fn pv3_an_hdr_clip_gets_the_tone_map_filter_at_its_decoded_size() {
    let mut tonemap = pq_4k();
    assert_eq!(
        tonemap.update(1920, 1080).as_deref(),
        Some("lavfi=[zscale=w=1920:h=1080:f=lanczos:t=linear:p=bt709:npl=203,format=gbrpf32le,tonemap=tonemap=reinhard:param=0.4:peak=4.926:desat=0,zscale=t=bt709:m=gbr:r=pc:d=random,format=gbrp,setsar=1]")
    );
    assert!(tonemap.on());
    assert_eq!(
        tonemap.update(1920, 1080),
        None,
        "an unchanged size sets nothing again"
    );
    // A scope 2.39:1 clip in a 16:9 box keeps its shape, as the renderer's own size does.
    tonemap.source.height = 1608;
    assert_eq!(tonemap.wanted(1920, 1080), Some((1920, 804)));
}

#[test]
fn pv3_a_resize_moves_the_filter_and_never_upscales() {
    let mut tonemap = pq_4k();
    tonemap.update(960, 540);
    assert_eq!(tonemap.wanted(2560, 1440), Some((2560, 1440)));
    assert!(tonemap
        .update(2560, 1440)
        .unwrap()
        .contains("w=2560:h=1440"));
    assert_eq!(tonemap.wanted(7680, 4320), Some((3840, 2160)));
}

#[test]
fn pv3_non_pq_rotated_and_unsized_clips_keep_the_renderer_path() {
    for source in [
        Source {
            hdr: false,
            width: 3840,
            height: 2160,
            rotated: false,
            peak_nits: 4000,
        },
        Source {
            hdr: true,
            width: 3840,
            height: 2160,
            rotated: true,
            peak_nits: 0,
        },
        Source {
            hdr: true,
            width: 0,
            height: 2160,
            rotated: false,
            peak_nits: 0,
        },
    ] {
        let mut tonemap = with(source);
        assert_eq!(
            tonemap.update(1920, 1080),
            None,
            "{source:?} sets no vf at all"
        );
        assert!(!tonemap.on());
    }
}

#[test]
fn pv3_a_clip_that_turns_out_not_pq_takes_the_filter_off() {
    let mut tonemap = pq_4k();
    tonemap.update(1920, 1080);
    tonemap.source.hdr = false;
    assert_eq!(tonemap.update(1920, 1080).as_deref(), Some(""));
    assert!(!tonemap.on());
}

#[test]
fn pv3_a_failed_filter_comes_off_and_stays_off() {
    let mut tonemap = pq_4k();
    tonemap.update(1920, 1080);
    assert_eq!(tonemap.fail().as_deref(), Some(""));
    assert_eq!(tonemap.update(2560, 1440), None);
    assert_eq!(tonemap.fail(), None, "nothing to take off twice");
    assert!(!tonemap.on());
}

#[test]
fn pv3_only_a_full_stall_under_the_filter_is_checked() {
    assert!(stall_due(true, STALL));
    assert!(!stall_due(true, STALL - Duration::from_millis(1)));
    assert!(
        !stall_due(false, STALL * 10),
        "no filter, nothing to give up"
    );
}

#[test]
fn pv3_the_curve_rolls_off_from_the_peak_the_decoder_reports() {
    // mpv reports the peak in multiples of 203 nits: an HDR10 master at 4000 nits, then HLG's nominal 1000.
    let mut tonemap = pq_4k();
    tonemap.source.observe_peak(4000. / 203.);
    assert_eq!(tonemap.source.peak_nits, 4000);
    assert!(tonemap
        .update(1920, 1080)
        .unwrap()
        .contains("tonemap=reinhard:param=0.4:peak=19.704:desat=0"));
    let mut hlg = pq_4k();
    hlg.source.observe_peak(1000. / 203.);
    assert!(hlg.update(960, 540).unwrap().contains(":peak=4.926:"));
}

#[test]
fn pv3_the_first_reported_peak_is_kept_and_bad_reports_are_ignored() {
    let mut source = Source::default();
    for bad in [0., -1., f64::NAN, f64::INFINITY] {
        source.observe_peak(bad);
        assert_eq!(source.peak_nits, 0, "{bad}");
    }
    assert_eq!(source.peak(), DEFAULT_PEAK_NITS);
    source.observe_peak(4000. / 203.);
    source.observe_peak(1000. / 203.);
    assert_eq!(
        source.peak(),
        4000,
        "per-frame metadata never moves the filter"
    );
    let mut low = Source::default();
    low.observe_peak(0.5);
    assert_eq!(low.peak(), WHITE_NITS);
    let mut high = Source::default();
    high.observe_peak(1e9);
    assert_eq!(high.peak(), MAX_PEAK_NITS);
}

#[test]
fn pv3_a_peak_reported_after_the_filter_is_on_resets_it_once() {
    let mut tonemap = pq_4k();
    assert!(tonemap.update(1920, 1080).unwrap().contains(":peak=4.926:"));
    tonemap.source.observe_peak(4000. / 203.);
    assert!(tonemap
        .update(1920, 1080)
        .unwrap()
        .contains(":peak=19.704:"));
    assert_eq!(tonemap.update(1920, 1080), None);
}
