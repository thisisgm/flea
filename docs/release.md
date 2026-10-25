# Releasing Flea

A release is a `vX.Y.Z` tag. Pushing one runs `.github/workflows/release.yml`, which builds and tests
Flea on a real x86_64 machine and a real aarch64 machine, and attaches four kinds of asset to the tag's
GitHub release: one prebuilt tarball per architecture with its `.sha256` sidecar, the source tarball
`flea-vX.Y.Z.tar.gz`, and `SHASUMS256.txt` over all of them. It then builds the `flea` and `flea-bin`
PKGBUILDs against those published assets, gives `flea-git` the tag's `pkgver` without building it (it
builds `main`, not the release), and pushes all three. Everything but the AUR push works with no
configuration at all; the push needs a one-time setup, described below.

One release feeds four packages:

| Package | Where | What it is | Who publishes it |
|---|---|---|---|
| `flea` | Omarchy's repository (OPR), the default | the tagged source, built and signed by Omarchy | OPR's own sync, which verifies `flea-vX.Y.Z.tar.gz` against `SHASUMS256.txt` a day or more after the tag |
| `flea-bin` | AUR | the binary the workflow built; no Rust toolchain, seconds not minutes | the workflow, on every tag |
| `flea` | AUR | `flea-vX.Y.Z.tar.gz` built on the user's machine; on Omarchy the OPR package of the same name comes first | the workflow, on every tag |
| `flea-git` | AUR | `main`, built on the user's machine | the workflow, on every tag |

The AUR PKGBUILDs are `packaging/flea-bin/PKGBUILD`, `packaging/flea/PKGBUILD` and
`packaging/flea-git/PKGBUILD`. Those files are the source of truth: an edit made on the AUR itself is
replaced by the next tag, so a change goes in by pull request here.

## Cutting a release

1. The tree says the version in four places, and the workflow refuses a tag where they disagree:
   `version` in `Cargo.toml`, and `pkgver` in `PKGBUILD`, `packaging/flea-bin/PKGBUILD` and
   `packaging/flea/PKGBUILD`. `Cargo.lock` follows `Cargo.toml` on the next `cargo build`, and
   `--locked` fails until it has. `packaging/flea-git/PKGBUILD`'s `pkgver` is not one of them: the
   workflow writes it. `"version"` in `shelf/manifest.json` is a fifth, and `cargo test` refuses a
   tree where it differs from `Cargo.toml`, so the build job stops before anything is published. The
   Enable shelf switch replaces a user's copy of the bar plugin only when the packaged version differs
   from it, so a shelf change under an unchanged version would never reach anyone who has the shelf
   on. Every `package()` stamps the packaged copy with `$pkgver-$pkgrel`, which is how `flea-git`
   replaces the copy on each build between releases, and a rebuild of one release does too.
2. Notes, optionally, in `docs/release-notes-X.Y.Z.md`. The workflow reads that file into the
   release body when it is the one creating the release; a release that already exists keeps
   whatever body it has.
3. Run `tools/flea-pkgrel-check` on the candidate commit before tagging: it fails when `pkgver`
   changed since the previous `vX.Y.Z` tag and `pkgrel` is not 1 in either PKGBUILD, so a fresh
   version always starts at 1 and only a rebuild of one version keeps a higher number.
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

Then the workflow runs five jobs, in order:

- **verify.** The tag is exactly `vX.Y.Z`, the four versions above equal it, and each of the three
  AUR PKGBUILDs declares the same `depends` and `optdepends` as `PKGBUILD` and runs the same
  `package()` commands, comments, blank lines, the `cd` line and the binary line aside. This is the
  drift guard: a runtime dependency or an installed file added to one PKGBUILD and not the others
  fails here, before anything is built. The AUR's own `flea` and `flea-git` PKGBUILDs of 0.3.1 are
  why it covers all three: neither installs `ui/boot/`, which Flea needs from 0.3.2, so a version
  bump of either builds a Flea that exits with "the shell config is missing". It also resolves the
  tag to its commit, `refs/tags/X` and never a branch of the same name, and every later job checks
  out that commit.
- **build**, twice, on `ubuntu-24.04` and on `ubuntu-24.04-arm`. Each runs `cargo test
  --locked`, builds the release binary, and stages `flea-vX.Y.Z-linux-<arch>.tar.gz` with
  `packaging/flea-bin-tarball`, which refuses a binary of the wrong architecture or one that prints
  a different version. It then checks that the tarball holds every path `flea-bin`'s `package()`
  installs, so a file added to the PKGBUILDs and not to the staging fails here, before anything is
  published. The test step runs the debug profile, the same tests CI gates on, because GPUI
  compiles out of release builds the debug selectors the flea-gui tests query; the binary is still
  built release. `tests/js.sh` and `tests/keymap-gen.sh` run in no job of this workflow: both need `qml6`,
  which Ubuntu does not ship on PATH, and the first reads an installed Omarchy besides. They gate a
  release on the maintainer's Omarchy box, in `tests/run-all.sh` before the tag is made, and OPR's
  build runs them again in `check()`; the `flea` and `flea-bin` legs here pass `--nocheck` or have no
  `check()`, so nothing in this workflow would catch a broken `ui/js` file. Flea has no crate dependencies and links only glibc and
  gcc-libs, so a binary built on Ubuntu 24.04 runs on Arch, whose glibc is never the older one.
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
  before uploading anything, because the AUR and OPR pin those checksums.
- **publish-aur**, three times, once per AUR package, each its own job in the `aur` environment. A
  leg that fails, for example because the key cannot push `flea` yet, fails red on its own and the
  other two still publish.
  - `flea-bin` pins the two binary checksums from their sidecars into a copy of its PKGBUILD, then
    runs `makepkg` in an `archlinux:base-devel` container against the assets the release job just
    published, once as x86_64 and once under a `makepkg.conf` that says aarch64, and fails unless the
    two packages hold the same file list with `usr/share/flea/ui/boot/shell.qml` in it.
  - `flea` pins the source tarball's checksum from the published `SHASUMS256.txt`, read with the
    same `awk` OPR uses, so both channels pin one number. It then builds the package with `rust` in
    the same kind of container, `makepkg --nodeps --nocheck` against the published tarball, and fails
    unless `usr/share/flea/ui/boot/shell.qml` is in it. `--nocheck` because `check()` needs `qml6`
    and an installed Omarchy; the build job ran `cargo test` on this commit already.
  - Both built legs then run `packaging/flea-default-check` on their x86_64 package: it installs the
    package in the throwaway container and, as a fresh user whose folders another handler owns, runs
    `flea --default` and `flea --default off`, and fails unless folders, Show in folder (an installed
    helper), file dialogs and the Hyprland keys all move to Flea and all come back. `flea-git`
    installs the same files, because verify fails any `package()` that drifts. OPR's PKGBUILD is
    written by hand in `omacom/omarchy-pkgs`, which verify never sees, so a file added to
    `package()` reaches OPR only through a change there: 0.3.5's `packaging/flea.hook` needs
    `install -Dm644 packaging/flea.hook -t "$pkgdir/usr/share/libalpm/hooks"`.
  - `flea-git` gets the `pkgver` its `pkgver()` prints on the tag itself, `X.Y.Z.r0.g<7 hex>`. It is
    pushed on every tag, like `flea` and `flea-bin` (decided 2026-10-01): only a PKGBUILD byte-identical
    to the one on the AUR pushes nothing.

  Each leg then checks that the tag is still the newest `vX.Y.Z` on the remote, so rebuilding an old
  tag, or a run a newer tag overtook, ends in a warning and never downgrades the AUR. A missing
  `AUR_SSH_KEY` secret or `AUR_COMMIT_NAME` / `AUR_COMMIT_EMAIL` variable also ends the leg in a
  warning that names what is missing, so a release made before the AUR account exists still ships.
  The push is `packaging/aur-push`, run as a normal user in an `archlinux:base-devel` container. The
  key can push to every package its AUR account maintains, so `aur-push` refuses any package but `flea`,
  `flea-bin` and `flea-git` before it touches the key or the network (`tests/aurpush.sh`). It
  clones `ssh://aur@aur.archlinux.org/<package>.git`, copies the pinned PKGBUILD in, regenerates
  `.SRCINFO` with `makepkg --printsrcinfo`, commits as `AUR_COMMIT_NAME <AUR_COMMIT_EMAIL>`, and
  pushes. The key reaches the container on its stdin, never in `docker run`'s environment, which
  Docker keeps in the container's config on disk while it runs; `pacman` runs without it, and
  `aur-push` loads it into an `ssh-agent` of its own and unsets it, so it is never in a file or an argv
  and the agent dies with the run. It never forces: if the AUR moved since the clone, the push is
  refused and the leg fails.
  When the AUR already carries the same files there is no commit and no push. SSH trusts only the
  three AUR host keys pinned in `packaging/aur.known_hosts`, with `StrictHostKeyChecking=yes`;
  nothing is scanned or learned at run time. After the workflow, `tools/flea-aur-versions X.Y.Z <tag commit>`
  reads the AUR RPC and checks `flea` and `flea-bin` report `X.Y.Z-<pkgrel>`, the pkgrel each
  PKGBUILD carries at the tag commit itself (so a rebuild at pkgrel 2 expects `X.Y.Z-2`, not
  `X.Y.Z-1`), and `flea-git` the tag's own `pkgver`. Its usage is
  `flea-aur-versions [--json FILE] X.Y.Z COMMIT`: one line per package on stdout, exit 1 on
  any mismatch, exit 2 on usage errors. `tools/flea-pkgrel-check [COMMIT]` (default `HEAD`)
  is the pre-tag guard above: it exits 0 when the check passes, 1 when the pkgver/pkgrel
  guard fails, and 2 on usage, git or file errors, including a shallow checkout or a parent
  with no reachable `vX.Y.Z` tag, which mean tags were never fetched.

A run that failed for a reason outside the tree, a runner outage or a mirror that timed out, is
started again. When only publish-aur failed, use `Re-run failed jobs` on that same run: it re-runs the
failed legs alone and reuses the run's own artifacts, which are the published tarballs, so nothing
public changes. The same button finishes a run whose legs failed because the key was not on the
account yet, once it is. A leg that skipped because the key or the variables were missing
finished green, so open it in the tag's run and use `Re-run this job` once they are set: the other
jobs are left alone and the release is not touched. GitHub keeps a run's artifacts for 30 days. A fresh `Run workflow` rebuilds, and must
be started from the tag itself (`Use workflow from`, the tag), because the `aur` environment only
serves `v*` tags; if the assets were already published it stops rather than replace them, since the
AUR and OPR pin their checksums, so delete them from the release first only to rebuild on purpose.
GitHub allows a re-run for 30 days after the run, and only while the run's artifacts are still
retained; past either, delete the assets from the release and run a fresh `Run workflow`, which
publishes new ones and pins their new checksums on the AUR in the same run.

## The AUR push, set up once

**Whose account.** The AUR is not registering new accounts, so there is no account of Flea's own
and no co-maintainer to add: the workflow pushes as taxin, who maintains the AUR `flea` and
`flea-git`, with a key of its own on his existing account. The AUR takes several SSH public keys per
account, one per line, so this key sits beside taxin's own and deleting its line revokes it alone,
at once. An AUR key belongs to an account, not to a package, so it can push to every package that
account maintains, and a pushed PKGBUILD runs on users' machines at their next update, unread,
because `omarchy update` runs `yay -Sua --noconfirm`; that is why only a `v*` tag the repository owner created can
read it. If taxin prefers to publish by hand, the workflow's legs skip green and each release is
three `packaging/aur-push` runs from the tag with the checksums pinned, as "By hand" shows.

### taxin, once

1. **The key**, on a machine you trust, with no passphrase because the workflow cannot type one:

   ```
   ssh-keygen -t ed25519 -N '' -C 'flea release workflow' -f flea-aur-deploy
   ```

   This writes `flea-aur-deploy` (private) and `flea-aur-deploy.pub` (public).
2. **The public half goes beside your own.** My Account, the `SSH Public Key` field: keep your
   current key and add the one line from `flea-aur-deploy.pub` on a new line below it, enter your
   password to confirm, and save. `flea-bin` does not exist yet; the first tag's push creates it
   under your account.
3. **Prove the pair.** `ssh -T -i flea-aur-deploy aur@aur.archlinux.org` answers
   `Welcome to AUR, <account>! Interactive shell is disabled.`, naming your account. If ssh first
   asks to trust the host, answer yes only when the fingerprint it shows is one of the three listed
   on the [AUR home page](https://aur.archlinux.org): `SHA256:RFzBCUItH9LZS0cKB5UE6ceAYhBD5C8GeOBip8Z11+4`
   (Ed25519), `SHA256:uTa/0PndEgPZTf76e1DFqXKJEXKsn7m9ivhLQtzGOCI` (ECDSA),
   `SHA256:5s5cIyReIfNNVGRFdDbe3hdYiI5OelHGpw2rOUud3Q8` (RSA).
4. **The private half goes to the repository owner** over an end-to-end encrypted channel
   that does not keep it: a one-time Bitwarden Send or 1Password share link, or Signal. Never email,
   Discord, or a GitHub issue or comment.
5. **Delete both files** once the repository owner has stored the key. The AUR has the public half, GitHub has the
   private half, and nothing else needs either.

### The repository owner, once

1. **The environment.** On GitHub, Settings, Environments, New environment, named exactly `aur`.
   Under `Deployment branches and tags` choose `Selected branches and tags`, then
   `Add deployment branch or tag rule`, ref type `Tag`, name pattern `v*`. Add no branch rule, so no
   branch can ever read the key. Optionally tick `Required reviewers` and add yourself: every
   release then waits for your approval before its AUR legs start.
2. **The key.** In that environment, `Add environment secret`, named exactly `AUR_SSH_KEY`, with the
   whole of `flea-aur-deploy` as its value, `BEGIN` and `END` lines included. First run step 3 of
   taxin's list yourself: a secret cannot be read back, so this is the last moment the check can
   run. Then delete the file.
3. **The commit author.** Settings, Secrets and variables, Actions, the `Variables` tab,
   `New repository variable`, twice: `AUR_COMMIT_NAME` and `AUR_COMMIT_EMAIL`, the name and address
   the AUR commits are authored with: `GM` and `gianmarcomorales@icloud.com`, the author of every
   commit in every repository GM owns. The `# Maintainer:` lines of the PKGBUILDs author nothing.
4. **The tag ruleset.** Settings, Rules, Rulesets, `New ruleset`, `New tag ruleset`. Name it
   `release tags`, enforcement status `Active`. Bypass list: `Add bypass`, `Repository admin`.
   Target tags: `Add target`, `Include by pattern`, `v*`. Rules: `Restrict creations`,
   `Restrict updates`, `Restrict deletions` and `Block force pushes`. Create. On a personal
   repository the owner is the only admin, so only the owner can create, move or delete a `v*` tag, and with
   step 1 only a tag the owner created can read the key.
5. **No second copy.** If a repository secret named `AUR_SSH_KEY` exists from an earlier setup,
   delete it (Settings, Secrets and variables, Actions, `Repository secrets`), so the key exists only
   behind the `v*` rule.

That is all: the next `vX.Y.Z` tag adds `flea-bin` to taxin's packages and updates `flea` and
`flea-git`. Each leg's log ends in `aur-push: pushed <package> as <commit>` or in a line saying
nothing is pushed and why. To pause the automation, delete the secret: the `flea` and `flea-bin` legs
still prove their PKGBUILDs, `flea-git` still gets its `pkgver`, and every leg skips the push, green, with an `AUR push skipped` warning on the run, which is then
the only sign that nothing reached the AUR; the release itself is still complete.

### Rotating the key

Whenever a copy may have leaked, or someone who handled it should no longer have it: taxin makes a
new pair (step 1), adds its public line to the account's `SSH Public Key` field, deletes the old line,
and sends the private half as before; the repository owner replaces the value of `AUR_SSH_KEY` in the `aur` environment,
which can be overwritten but never read. Deleting the old public line is what revokes the old key,
at once; when a leak is suspected, do that first.

### When a leg fails

- **`Permission denied (publickey)`**: the key in the environment and the public line in the
  account are not a pair. Rotate.
- **`git-receive-pack: permission denied: <account>`**: the account may not push that package, for
  example `flea-bin` if another account created it first. The other legs have published; fix the
  access, then `Re-run failed jobs`.
- **`Your account email is not verified`**: verify it from the account's profile and re-run.
- **`Host key verification failed`**: the server did not present a pinned key. Compare the
  fingerprints on the AUR home page with `ssh-keygen -lf packaging/aur.known_hosts`. If the AUR has
  changed its keys, take the new `aur.archlinux.org` lines from Arch's own
  [infrastructure repository](https://gitlab.archlinux.org/archlinux/infrastructure/-/blob/main/docs/ssh-known_hosts.txt),
  check they print the fingerprints the home page lists, and commit them; if the home page still
  lists the pinned ones, something between the runner and the AUR is not the AUR, and nothing is
  pushed.
- **`denying non-fast-forward (you should pull first)`**: the package changed on the AUR between
  the clone and the push. Re-run the leg: it clones the new head and commits on top of it, and its
  content replaces the AUR-side edit.

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
checksums are pinned exactly as the workflow pins them. `flea-bin`:

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
read as `/usr/share/licenses/flea-bin/`. `flea`, which needs `cargo`:

```
cd packaging/flea
cp ../../dist/flea-vX.Y.Z.tar.gz .
sed -i "s/^sha256sums=.*/sha256sums=('$(awk '$2 == "flea-vX.Y.Z.tar.gz" { print $1 }' ../../dist/SHASUMS256.txt)')/" PKGBUILD
makepkg --nodeps --nocheck --clean
```

On an Omarchy box drop `--nocheck` and the suite runs too. Everything these leave behind is in each
directory's `.gitignore`; put the `SKIP`s back before committing.

**The AUR push**, rehearsed against local bare repositories standing in for the AUR, on an Arch box
or in an `archlinux:base-devel` container with `git` installed. `AUR_REMOTE_BASE` replaces
`ssh://aur@aur.archlinux.org`, and no key is involved:

```
git init --bare /tmp/fake-aur/flea-bin.git
AUR_REMOTE_BASE=/tmp/fake-aur AUR_COMMIT_NAME=test AUR_COMMIT_EMAIL=test@example.invalid \
  packaging/aur-push flea-bin packaging/flea-bin/PKGBUILD "flea-bin vX.Y.Z"
git -C /tmp/fake-aur/flea-bin.git log --stat master
```

Run it twice and the second run says nothing is pushed. For the real AUR, when the workflow cannot,
drop `AUR_REMOTE_BASE` and let your own SSH setup reach the AUR, with the pinned PKGBUILD, never the
`SKIP` one. `AUR_SSH_KEY`, holding the private key itself, is how the workflow passes its key; unset, as
here, `aur-push` leaves ssh to your agent and `~/.ssh/config`, host key pinning included.
