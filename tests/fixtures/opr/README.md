# Pinned OPR recipe

`PKGBUILD` is the proposed OPR recipe. Its base is `omacom/omarchy-pkgs`
`master`, commit `e1bbf8545182f2ac00cb1455427c6d9e0a41ec39`,
`pkgbuilds/flea/PKGBUILD`, the published 0.4.1 recipe. Its version and checksum
identify the published 0.4.1 source; the OPR sync replaces those fields for each
release. It differs from that base in three places:

- One comment line states why the copystage pair runs on tmpfs.
- Each isolated SVG watchdog run must report one passed test, so a selector that
  matches nothing fails the check.
- The two optional skip messages state their condition without release history.

The 0.4.0 changes are the shared install target, the GPUI runtime and build
dependencies (each depend names what uses it), the audited license inventory, a
workspace binary build and GPUI checks in the debug profile. Packaging selects
every JS suite except the live stock-theme inventory using the existing suite-name
flag, including both timezone suites. Stock-theme inventory checks remain in
flea-ci. The existing tmpfs and aarch64 handling is retained.
The root recipe is the same text except for release metadata. Its checksum is 64
zeros, a placeholder that fails makepkg's verification until the OPR sync, or a gate
staging a candidate, writes the source tarball's SHA-256. The comparison refuses a
`SKIP` checksum in either recipe. docs/install.md builds a checkout by writing that
tarball with `git archive` and passing `--skipchecksums`.

Run `tests/opr-recipe.sh` to compare the root recipe with this pinned copy, with
no network access. To check another prepared or live checkout instead, run
`FLEA_OPR_PKGBUILD=/path/to/pkgbuilds/flea/PKGBUILD tests/opr-recipe.sh`.
The comparison reads text without sourcing either recipe and allows only pkgver,
pkgrel, sha256sums and the Maintainer line to differ. Before comparing, it runs
in-memory controls on the root recipe: changed release fields must pass, and a
changed dependency, code after pkgver, a `SKIP` checksum or an unparsable checksum
must fail. Update both recipes together when proposing an OPR packaging change;
CI can supply its independently fetched OPR recipe through the environment
override.
