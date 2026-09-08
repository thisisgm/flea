# 0.1.6 benchmark results

Measured on 8 September 2026. Each fixture contains three runs of seven GUI and eight terminal apps. Flea commit `f8751aa` and released commit `784da46` have identical source tree `921b1c26ce5c3763de24cf2b815cd9bf9edf40a6`.

## Source records

- [Scale CSV](f016c-scale.csv) and [manifest](f016c-scale.manifest.md): 100,000 empty text files on btrfs.
- [Media CSV](f016c-media.csv) and [manifest](f016c-media.manifest.md): 2,000 files, including 200 HEIC images and 200 text controls.
- [Provenance hashes](f016c-provenance.json); manifest hostname and absolute paths are anonymized.

The manifests record package versions and capture times. Strata was built from v0.13.0 (`200165606d007622d014e4119233ec96f0f0ce65`); its binary SHA-256 was `783fb596a765dd8c0ddd5040636a935c743ab1cfafe968a7a24588b2e37d52b4`.

## GUI results

The [README](../../README.md#performance) contains the current GUI tables. Media thumbnail counts vary from 36 to 606 across individual runs; the table reports each entrant’s median. Counts describe work observed, not which formats the app supports.

## Terminal apps

All terminal apps ran under kitty 0.48.2 with the configuration recorded in the manifests. Their first-window timings measure terminal startup and cannot be compared with GUI launch timings. Image-preview timings come from a separate launch. Flea has no terminal interface yet.

| App | Scale settled | Media settled | First image preview, media | Preview runs |
|---|---:|---:|---:|---:|
| yazi | 1972 ms | 1756 ms | 746 ms | 3/3 |
| mc | 2353 ms | 856 ms | Not observed | 0/3 |
| broot | 471 ms | 513 ms | Not observed | 0/3 |
| nnn | 2029 ms | 599 ms | 715 ms | 3/3 |
| lf | 2123 ms | 1823 ms | 744 ms | 3/3 |
| ranger | 8629 ms | 1027 ms | 1113 ms | 3/3 |
| xplr | 3604 ms | 598 ms | Not observed | 0/3 |
| superfile | 2143 ms | 1161 ms | 736 ms | 2/3 |

No observed preview is not a zero-millisecond result or proof that a format is unsupported.

## Reporting limits

CPU is measured over the process tree; window PSS excludes helper processes and is affected by library sharing. “Settled” is CPU quiescence, not an input-readiness measurement. PCManFM’s media `-1` values are timeouts.

The historical chart generator remains tied to the older `scale-rc-2026` and `media-rc-2044` runs. Its chart files are historical and are not used in the current README. The legacy report’s media rankings must not be used for these results: thumbnail work differs between entrants.
