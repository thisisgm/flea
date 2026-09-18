# Releasing Flea

A release is a `vX.Y.Z` tag. Pushing one runs `.github/workflows/release.yml`, which builds and tests
Flea on a real x86_64 machine and a real aarch64 machine, attaches one prebuilt tarball per
architecture to the tag's GitHub release, proves `packaging/flea-bin/PKGBUILD` against those two
assets, and pushes `flea-bin` to the AUR. Everything but the AUR push works with no configuration at
all; the push needs one secret, set up once, described below.

Two AUR packages come out of a release, and they install the same file list:

| Package | What it is | Who publishes it |
|---|---|---|
| `flea` | builds the tagged source on the user's machine and runs the suite there | its AUR maintainer, by hand |
| `flea-bin` | installs the binary the workflow built; no Rust toolchain, seconds not minutes | the workflow, on every tag |

`flea-bin` is what an Omarchy user on a laptop, or on an aarch64 box where a release build of Flea
is a long wait, is told to install. `flea-git` is unchanged by any of this.

## Cutting a release

1. The tree says the version in three places, and the workflow refuses a tag where they disagree:
   `version` in `Cargo.toml`, `pkgver` in `PKGBUILD`, and `pkgver` in `packaging/flea-bin/PKGBUILD`.
   `Cargo.lock` follows `Cargo.toml` on the next `cargo build`, and `--locked` fails until it has.
2. Notes, optionally, in `docs/release-notes-X.Y.Z.md`. The workflow reads that file into the
   release body when it is the one creating the release; a release that already exists keeps
   whatever body it has.
3. Tag and push:

   ```
   git tag -a vX.Y.Z -m "Flea X.Y.Z"
   git push origin vX.Y.Z
   ```

   Creating the release through GitHub's web form does the same thing: the form pushes the tag,
   the tag starts the workflow, and the workflow attaches its assets to the release the form made.

Then the workflow runs four jobs, in order:

- **verify.** The tag is exactly `vX.Y.Z`, the three versions above equal it, and
  `packaging/flea-bin/PKGBUILD` declares the same `depends` and `optdepends` and installs the same
  `package()` body as `PKGBUILD`, cd line and binary line aside. This is the drift guard: a runtime
  dependency added to one PKGBUILD and not the other fails here, before anything is built.
- **build**, twice, on `ubuntu-24.04` and on `ubuntu-24.04-arm`. Each runs `cargo test --release
  --locked`, builds the release binary, and stages `flea-vX.Y.Z-linux-<arch>.tar.gz` with
  `packaging/flea-bin-tarball`, which refuses a binary of the wrong architecture or one that prints
  a different version. `tests/js.sh` and `tests/keymap-gen.sh` are not run there: both need `qml6`,
  which Ubuntu does not ship on PATH, and the first reads an installed Omarchy besides; the Arch box
  building the source package has both. Flea has no crate dependencies and links only glibc and
  gcc-libs, so a binary built on Ubuntu 24.04 runs on Arch, whose glibc is never the older one.
- **release.** Creates the GitHub release if the tag has none, otherwise attaches to it. Only the
  two tarballs and their `.sha256` sidecars are ever written or replaced; a source tarball or a
  checksum file uploaded by hand is left alone.
- **publish-aur.** Pins the two checksums into a copy of `packaging/flea-bin/PKGBUILD`, then runs
  `makepkg` in an `archlinux:base-devel` container against the assets the release job just
  published, once as x86_64 and once under a `makepkg.conf` that says aarch64, and fails unless the
  two packages hold the same file list. Only then, and only when the `AUR_SSH_KEY` secret is set,
  does it commit that PKGBUILD and a regenerated `.SRCINFO` to `ssh://aur@aur.archlinux.org/flea-bin.git`.
  Without the secret the job ends with a warning and the release is still complete.

A run that failed for a reason outside the tree, a runner outage or a mirror that timed out, is
started again from the Actions tab: `Release`, `Run workflow`, with the existing tag as `ref`. It
attaches to the release it already made and replaces the assets in place.

## The AUR push, set up once

The workflow pushes to the AUR over SSH as whatever account owns the key it is given. It never
sees a password and never touches the `flea` or `flea-git` packages.

1. **An AUR account.** [aur.archlinux.org/register](https://aur.archlinux.org/register), or the
   account that already exists. The first push to a package name that does not exist yet creates
   the package and makes that account its maintainer; `flea-bin` has no owner today.
2. **A key for this purpose alone**, so it can be revoked without touching anything else:

   ```
   ssh-keygen -t ed25519 -N '' -C 'flea-bin release workflow' -f flea-aur-deploy
   ```

   This writes `flea-aur-deploy` (private) and `flea-aur-deploy.pub` (public).
3. **The public half goes to the AUR account.** aur.archlinux.org, My Account, the
   `SSH Public Key` field: paste the one line from `flea-aur-deploy.pub` and save. The field holds
   more than one key, one per line, so an existing key stays.
4. **The private half goes to the repository.** On GitHub, Settings, Secrets and variables,
   Actions, New repository secret, named exactly `AUR_SSH_KEY`, with the whole of the
   `flea-aur-deploy` file as its value, `BEGIN` and `END` lines included.
5. **Prove the pair while the private half is still on disk.** `ssh -T aur@aur.archlinux.org -i
   flea-aur-deploy` answers with the account name the key maps to, which has to be the account
   from step 1. A GitHub secret cannot be read back, so this is the last moment the check can run.
6. **Delete both files** from the disk they were made on. The AUR has the public half, GitHub has
   the private half, and nothing else needs either.

That is all: the next `vX.Y.Z` tag publishes `flea-bin`. The commit on the AUR is authored by the
`# Maintainer:` line of `packaging/flea-bin/PKGBUILD`, so that line is the one to change if the
account is somebody else's. To take the automation away, delete the secret; the workflow goes back
to warning and skipping.

If the push is ever refused with `permission denied`, the public key in the AUR account and the
private key in the secret are not a pair, or the package already exists under an account that does
not hold this key. The private half is gone from disk by then and the secret cannot be read back,
so the repair is a new pair: steps 2 to 6 again, replacing the old line in the AUR account and the
old value of the secret.

## By hand

Everything the workflow does has a hand-run form, which is also how a change to any of it is proved
before it is committed.

**The tarball**, on a box that has built `target/release/flea`:

```
packaging/flea-bin-tarball target/release/flea X.Y.Z x86_64 dist
```

For aarch64, build in a container of that architecture and stage inside it, because the staging
runs the binary to read its version and a cross-built binary has no loader here:

```
docker run --rm --platform linux/arm64 -v "$PWD:/src" -w /src rust:1-bookworm \
  bash -c 'cargo build --release --locked && packaging/flea-bin-tarball target/release/flea X.Y.Z aarch64 dist'
```

**The package**, from the tarballs in `dist/`. `makepkg` reads a source file that is already
beside the PKGBUILD instead of downloading it, so the tarballs are copied in under the names the
`source_*` arrays give them, and the checksums are pinned exactly as the workflow pins them:

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
the aarch64 package on any machine. The file list is `flea`'s own with `/usr/share/licenses/flea/`
read as `/usr/share/licenses/flea-bin/`. Everything this leaves behind is in the directory's
`.gitignore`; put the two `SKIP`s back before committing.

**The AUR**, when the workflow cannot:

```
git clone ssh://aur@aur.archlinux.org/flea-bin.git
cp packaging/flea-bin/PKGBUILD flea-bin/          # the pinned one, never the SKIP one
cd flea-bin && makepkg --printsrcinfo > .SRCINFO
git add PKGBUILD .SRCINFO && git commit -m "flea-bin vX.Y.Z" && git push
```
