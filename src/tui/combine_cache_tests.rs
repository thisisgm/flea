//! Stale external thumbnails cannot replace held pixels; warm recompositions reuse the snapshot.
use super::*;

pub(super) fn stale_cache_revision(root: &Path) {
    use crate::backend::thumbcache::{self, Cache};
    use std::fs::{File, FileTimes};
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::time::{Duration, UNIX_EPOCH};

    fixture(root);
    let source = root.join("files/a.png");
    let stale = root.join("old-red-thumbnail.png");
    let seconds = 1_700_000_000u64;
    File::open(&source)
        .unwrap()
        .set_times(
            FileTimes::new().set_modified(
                UNIX_EPOCH + Duration::from_secs(seconds) + Duration::from_millis(100),
            ),
        )
        .unwrap();
    tool(&["magick", source.to_str().unwrap(), stale.to_str().unwrap()]);
    // magick writes tEXt after IDAT, where readers stop, so stamp it the way the cache writer does.
    crate::backend::thumbwrite::stamp(&stale, &thumbcache::uri_for(&source), seconds as i64)
        .unwrap();
    tool(&[
        "magick",
        "-size",
        "40x80",
        "xc:cyan",
        source.to_str().unwrap(),
    ]);
    File::open(&source)
        .unwrap()
        .set_times(
            FileTimes::new().set_modified(
                UNIX_EPOCH + Duration::from_secs(seconds) + Duration::from_millis(900),
            ),
        )
        .unwrap();
    let cached = Cache::new().large_path(&thumbcache::uri_for(&source));
    std::fs::create_dir_all(cached.parent().unwrap()).unwrap();
    std::fs::copy(&stale, &cached).unwrap();
    assert_eq!(
        thumbcache::png_text(&std::fs::read(&cached).unwrap(), "Thumb::MTime"),
        Some(seconds.to_string()),
        "fixture must carry the ambiguous second-resolution cache timestamp"
    );
    let before = source.metadata().unwrap();
    let cache_time = cached.metadata().unwrap();
    assert!(
        (cache_time.ctime(), cache_time.ctime_nsec()) >= (before.ctime(), before.ctime_nsec()),
        "stale thumbnail must be installed after the current source revision"
    );
    let (mut m, mut w) = open(root);
    let locale_root = root.join("cache-locale");
    std::fs::create_dir_all(locale_root.join("default/i18n")).unwrap();
    std::fs::write(locale_root.join("default/i18n/locales.json"),
        r#"{"en":{"name":"English","formatLocale":"en-US","locale":"C","zeroDigit":"0"},"ar":{"name":"Arabic","formatLocale":"ar","locale":"C","zeroDigit":"٠","direction":"rtl"}}"#).unwrap();
    let language = root.join("cache-language");
    std::fs::write(&language, "en\n").unwrap();
    m.i18n = crate::i18n::I18n::load(crate::i18n::Paths {
        omarchy: locale_root,
        language_file: language.clone(),
        catalogs: vec![root.join("i18n")],
    });
    let wrapper = root.join("bin/bwrap");
    std::fs::write(
        &wrapper,
        r#"#!/bin/sh
set -eu
root=${FLEA_TUI_COMBINE_ROOT:?}
case "$root" in /*) test -f "$root/.flea-test-sandbox" ;; *) exit 98 ;; esac
for arg in "$@"; do
  if test "$arg" = --ro-bind-fd; then
    printf 'held source decoder\n' >> "$root/held-source-decodes"
    break
  fi
done
exec /usr/bin/bwrap "$@"
"#,
    )
    .unwrap();
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
    let graphics = preview(&mut m, &mut w, Protocol::Kitty);
    assert_current_pixels(root, &m, &graphics, "initial");
    let decodes = || {
        std::fs::read_to_string(root.join("held-source-decodes"))
            .unwrap()
            .lines()
            .count()
    };
    let first = decodes();
    assert_eq!(
        first, 6,
        "three held sources each need one dimension and one first-thumbnail decoder"
    );
    choose(&mut m, &mut w, 4, 2);
    let graphics = preview(&mut m, &mut w, Protocol::Kitty);
    assert_current_pixels(root, &m, &graphics, "spacing");
    assert_eq!(
        decodes(),
        first,
        "settings recomposition must reuse per-card source snapshots"
    );
    std::fs::write(&language, "ar\n").unwrap();
    assert!(m.i18n.poll_changed());
    let graphics = preview(&mut m, &mut w, Protocol::Kitty);
    assert_current_pixels(root, &m, &graphics, "locale");
    assert_eq!(
        decodes(),
        first,
        "locale recomposition must reuse per-card source snapshots"
    );
}

fn assert_current_pixels(root: &Path, m: &Model, graphics: &Graphics, phase: &str) {
    let output = root.join(format!("cached-revision-{phase}.png"));
    std::fs::write(&output, graphics_tests::kitty_png(&graphics.transmit)).unwrap();
    let f = m.combining.card.as_ref().unwrap();
    assert_eq!(f.sizes[0], (40, 80));
    let g = combine::geometry(m);
    let tile = f
        .composition()
        .unwrap()
        .fitted(
            ((g.preview.width - 2) * 8) as f64,
            ((g.preview.height - 2) * 16) as f64,
            0.,
        )
        .tiles[0];
    let color = tool(&[
        "magick",
        output.to_str().unwrap(),
        "-crop",
        &format!(
            "1x1+{}+{}",
            (tile.x + tile.width / 2.) as usize,
            (tile.y + tile.height / 2.) as usize
        ),
        "+repage",
        "-depth",
        "8",
        "rgb:-",
    ]);
    assert_eq!(
        color,
        [0, 255, 255],
        "current cyan source dimensions must not accompany stale red cache pixels"
    );
}
