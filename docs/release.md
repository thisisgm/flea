# Releasing Flea

A release is a `vX.Y.Z` tag. Pushing one runs `.github/workflows/release.yml`, which builds and tests
Flea on a real x86_64 machine and a real aarch64 machine, and attaches four kinds of asset to the tag's
GitHub release: one prebuilt tarball per architecture with its `.sha256` sidecar, the source tarball
`flea-vX.Y.Z.tar.gz`, and `SHASUMS256.txt` over all of them. It needs no configuration.

Flea is published as one package, `flea` in Omarchy's repository (OPR): the tagged source, built and
signed by Omarchy. OPR's own sync verifies `flea-vX.Y.Z.tar.gz` against `SHASUMS256.txt` a day or more
after the tag. OPR's recipe lives in `omacom/omarchy-pkgs` and is the root `PKGBUILD` but for its
release fields, so a change to `PKGBUILD` reaches users only through the same change there;
`tests/opr-recipe.sh` compares the two (see `tests/fixtures/opr/README.md`). The prebuilt tarballs, and
`packaging/flea-bin/PKGBUILD` that packages one, are for installers that build from a release; no
`flea-bin` package is published.

## Cutting a release

1. The tree says the version in three places, and the workflow refuses a tag where they disagree:
   `version` in `Cargo.toml`, and `pkgver` in `PKGBUILD` and `packaging/flea-bin/PKGBUILD`.
   `Cargo.lock` follows `Cargo.toml` on the next `cargo build`, and `--locked` fails until it has.
   `"version"` in `shelf/manifest.json` is a fourth, and `cargo test` refuses a tree where it differs
   from `Cargo.toml`, so the build job stops before anything is published. The
   Enable shelf switch replaces a user's copy of the bar plugin only when the packaged version differs
   from it, so a shelf change under an unchanged version would never reach anyone who has the shelf
   on. Every `package()` stamps the packaged copy with `$pkgver-$pkgrel`, so a rebuild of one release
   replaces the copy too.
2. Notes, optionally, in `docs/release-notes-X.Y.Z.md`. The workflow reads that file into the
   release body when it is the one creating the release; a release that already exists keeps
   whatever body it has.
3. Run `tools/flea-pkgrel-check` on the candidate commit before tagging: it fails when `pkgver`
   changed since the previous `vX.Y.Z` tag and `pkgrel` is not 1 in either PKGBUILD, so a fresh
   version always starts at 1 and only a rebuild of one version keeps a higher number.
   `tools/flea-pkgrel-check [COMMIT]` (default `HEAD`) exits 0 when the check passes, 1 when the
   pkgver/pkgrel guard fails, and 2 on usage, git or file errors, including a shallow checkout or a
   parent with no reachable `vX.Y.Z` tag, which mean tags were never fetched.
4. Push the candidate commit to a `candidate/X.Y.Z` branch. The Candidate workflow
   (`.github/workflows/candidate.yml`) runs the release's verify checks, with `Cargo.toml`'s version
   in place of the tag, and its build job up to publication on both runners: the same packages,
   `cargo test --locked`, the build and the flea-bin tarball check. Tag only the commit it
   passed, then delete the branch. A Docker image of the runner OS cannot stand in for this; 0.3.10
   and 0.3.11 were each tagged three times on failures only the runners showed.
5. Tag and push. Only the repository owner can create a `v*` tag (the ruleset below):

   ```
   git tag -a vX.Y.Z -m "Flea X.Y.Z"
   git push origin vX.Y.Z
   ```

   Creating the release through GitHub's web form does the same thing: the form pushes the tag,
   the tag starts the workflow, and the workflow attaches its assets to the release the form made.

Then the workflow runs four jobs, in order:

- **verify.** The tag is exactly `vX.Y.Z` and the three versions above equal it, and
  `packaging/flea-bin/PKGBUILD` has the root `PKGBUILD`'s `depends` and `optdepends`. It also
  resolves the tag to its commit, `refs/tags/X` and never a branch of the same name, and every later
  job checks out that commit.
- **build**, twice, on `ubuntu-24.04` and on `ubuntu-24.04-arm`. Each runs `cargo test
  --locked`, builds the release binary, and stages `flea-vX.Y.Z-linux-<arch>.tar.gz` with
  `packaging/flea-bin-tarball`, which refuses a binary of the wrong architecture or one that prints
  a different version. It then checks that the tarball holds every path `flea-bin`'s `package()`
  installs, so a file added to that `package()` and not to the staging fails here, before anything is
  published. The test step runs the debug profile, the same tests CI gates on, because GPUI
  compiles out of release builds the debug selectors the flea-gui tests query; the binary is still
  built release. `tests/js.sh` and `tests/keymap-gen.sh` run in no job of this workflow: both need `qml6`,
  which Ubuntu does not ship on PATH, and the first reads an installed Omarchy besides. They gate a
  release on the maintainer's Omarchy box, in `tests/run-all.sh` before the tag is made, and OPR's
  build runs them again in `check()`; nothing in this workflow would catch a broken `ui/js` file.
  Flea has no crate dependencies and links only glibc and gcc-libs, so a binary built on Ubuntu
  24.04 runs on Arch, whose glibc is never the older one.
- **source**, beside build. `packaging/flea-source-tarball` makes `flea-vX.Y.Z.tar.gz`: `git archive`
  of the tag under one `flea-X.Y.Z/` root, every entry stamped with the tag commit's time, through
  `gzip -n` so gzip adds no name or time of its own. It builds the tarball twice and refuses unless
  the two are byte for byte the same, and refuses a tag that no longer names the commit verify
  checked.
- **release.** Writes `SHASUMS256.txt`, one `sha256sum` line per asset in its own
  `<hash>  <name>` form, which is exactly what OPR's `upstream.sh` reads, and refuses a manifest
  without exactly one line for the source tarball. Then it creates the GitHub release if the tag has
  none, marked Latest only when the tag is the newest `vX.Y.Z` on the remote at that moment, and
  otherwise attaches to it. Nothing is ever written over a published asset: if the release already
  carries any file of the same name, the source tarball and `SHASUMS256.txt` included, the job stops
  before uploading anything, because a published checksum may already be pinned, the source
  tarball's by the OPR.

A run that failed for a reason outside the tree, a runner outage or a mirror that timed out, is
started again with `Re-run failed jobs` on that same run. A fresh `Run workflow` rebuilds, and must be
started from the tag itself (`Use workflow from`, the tag), so that it follows the tag's own
`release.yml`; if the assets were already published it stops rather than replace them, since a
published checksum may already be pinned, so delete them from the release first only to rebuild on
purpose. GitHub allows a re-run for 30 days after the run, and only while the run's artifacts are
still retained; past either, delete the assets from the release and run a fresh `Run workflow`, which
publishes new ones.

## The tag ruleset, set up once

Settings, Rules, Rulesets, `New ruleset`, `New tag ruleset`. Name it `release tags`, enforcement
status `Active`. Bypass list: `Add bypass`, `Repository admin`. Target tags: `Add target`,
`Include by pattern`, `v*`. Rules: `Restrict creations`, `Restrict updates`, `Restrict deletions` and
`Block force pushes`. Create. On a personal repository the owner is the only admin, so only the owner
can create, move or delete a `v*` tag.

## By hand

Everything the workflow does has a hand-run form, which is also how a change to any of it is proved
before it is committed.

**The binary tarball**, on a box that has built `target/release/flea`:

```
packaging/flea-bin-tarball target/release/flea X.Y.Z x86_64 dist
```

For aarch64, build in a container of that architecture and stage inside it, because the staging
runs the binary to read its version and a cross-built binary has no loader here:

```
docker run --rm --platform linux/arm64 -u "$(id -u):$(id -g)" -e HOME=/tmp -e CARGO_HOME=/tmp/cargo \
  -v "$PWD:/src" -w /src rust:1-bookworm \
  bash -c 'cargo build --release --locked && packaging/flea-bin-tarball target/release/flea X.Y.Z aarch64 dist'
```

Running as your own user keeps `target/` and `dist/` yours, and it is what lets git inside read the
checkout at all: git refuses a repository owned by another uid, and the tarball's date comes from it.

**The source tarball and the manifest**, from a checkout that has the tag:

```
packaging/flea-source-tarball vX.Y.Z dist
(cd dist && sha256sum -- flea-* > SHASUMS256.txt)
```

The same tag gives the same bytes on any box with GNU gzip, which is what the workflow runs; the
`git archive` stream is the same across git versions. macOS's gzip compresses differently, so a Mac
build of the same tag has another checksum: v0.3.2's published tarball was made on a Mac and matches
a Mac rebuild, not a Linux one. The release job never replaces a published tarball for exactly that
reason.

**The packages.** `makepkg` reads a source file that is already beside the PKGBUILD instead of
downloading it, so the tarballs are copied in under the names the `source` arrays give them, and the
checksums are pinned from the sidecars. `flea-bin`:

```
cd packaging/flea-bin
cp ../../dist/flea-vX.Y.Z-linux-*.tar.gz .
sed -i "s/^sha256sums_x86_64=.*/sha256sums_x86_64=('$(cut -d' ' -f1 ../../dist/flea-vX.Y.Z-linux-x86_64.tar.gz.sha256)')/" PKGBUILD
sed -i "s/^sha256sums_aarch64=.*/sha256sums_aarch64=('$(cut -d' ' -f1 ../../dist/flea-vX.Y.Z-linux-aarch64.tar.gz.sha256)')/" PKGBUILD
makepkg --nodeps --clean
pacman -Qlp flea-bin-X.Y.Z-1-x86_64.pkg.tar.zst
```

`--nodeps` because a build box need not be an Omarchy box, and `package()` only copies files.
The same command with `--config` pointing at a `makepkg.conf` whose `CARCH` says `aarch64` produces
the aarch64 package on any machine. The file list is `make install`'s with `/usr/share/licenses/flea/`
read as `/usr/share/licenses/flea-bin/`, which `tests/figure-package.sh` checks. Everything it leaves
behind is in the directory's `.gitignore`; put the `SKIP`s back before committing.

**The default check**, as root at the checkout's root in a throwaway `archlinux:base-devel` container
that holds the checkout and the x86_64 package built above:

```
packaging/flea-default-check packaging/flea-bin/flea-bin-X.Y.Z-1-x86_64.pkg.tar.zst
```

It installs the package and, as a fresh user whose folders another handler owns, runs
`flea --default` and `flea --default off`, and fails unless folders, Show in folder (an installed
helper), file dialogs and the Hyprland keys all move to Flea and all come back.
