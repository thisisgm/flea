# Pinned OPR recipe

`PKGBUILD` is the proposed 0.4.0 OPR recipe. It starts with
`omacom/omarchy-pkgs` branch `flea-leave-aur`, commit
`8dda872396dcea4ef7d12d135195a8ca1ef82ece`, `pkgbuilds/flea/PKGBUILD`.
Its version and checksum still identify the published 0.3.12 source; the OPR sync
replaces those fields for each release.

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
