use super::tonemap::*;
use std::time::Duration;

fn with(source: Source) -> Tonemap {
    let mut tonemap = Tonemap::default();
    tonemap.source = source;
    tonemap
}

fn hdr_4k() -> Tonemap {
    with(Source {
        hdr: true,
        width: 3840,
        height: 2160,
        rotated: false,
    })
}

#[test]
fn pv3_only_hdr_is_tone_mapped() {
    for gamma in ["pq", "hlg"] {
        assert!(hdr(gamma), "{gamma}");
    }
    for other in ["bt.1886", "srgb", "linear", "gamma2.2", "auto", ""] {
        assert!(!hdr(other), "{other}");
    }
}

#[test]
fn pv3_an_hdr_clip_gets_the_tone_map_filter_at_its_decoded_size() {
    let mut tonemap = hdr_4k();
    assert_eq!(
        tonemap.update(1920, 1080).as_deref(),
        Some("lavfi=[zscale=w=1920:h=1080:f=lanczos:t=linear:npl=203,format=gbrpf32le,zscale=p=bt709,tonemap=tonemap=mobius:param=0.3:peak=4.93:desat=0,zscale=t=bt709:m=gbr:r=pc:d=random,format=gbrp,setsar=1]")
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
    let mut tonemap = hdr_4k();
    tonemap.update(960, 540);
    assert_eq!(tonemap.wanted(2560, 1440), Some((2560, 1440)));
    assert!(tonemap
        .update(2560, 1440)
        .unwrap()
        .contains("w=2560:h=1440"));
    assert_eq!(tonemap.wanted(7680, 4320), Some((3840, 2160)));
}

#[test]
fn pv3_sdr_rotated_and_unsized_clips_keep_the_renderer_path() {
    for source in [
        Source {
            hdr: false,
            width: 3840,
            height: 2160,
            rotated: false,
        },
        Source {
            hdr: true,
            width: 3840,
            height: 2160,
            rotated: true,
        },
        Source {
            hdr: true,
            width: 0,
            height: 2160,
            rotated: false,
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
fn pv3_a_clip_that_turns_out_not_hdr_takes_the_filter_off() {
    let mut tonemap = hdr_4k();
    tonemap.update(1920, 1080);
    tonemap.source.hdr = false;
    assert_eq!(tonemap.update(1920, 1080).as_deref(), Some(""));
    assert!(!tonemap.on());
}

#[test]
fn pv3_a_failed_filter_comes_off_and_stays_off() {
    let mut tonemap = hdr_4k();
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
