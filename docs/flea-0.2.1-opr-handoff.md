# Flea 0.2.1: unblock the OPR build

Written 2026-09-11. Evidence gathered on minipc against the published v0.2.0 release
tarball. Everything below was measured, not inferred.

## Outcome in one sentence

v0.2.0 satisfies every automated OPR ingestion gate but fails `check()` on the Omarchy
build container, because three tests added in 0.2.0 reach the bwrap sandbox without the
`sandboxprobe::skipped()` guard that the other ten sandboxed tests already use; 0.2.1
adds that guard to those three tests.

## Do not redo this: what v0.2.0 already satisfies

Verified against the live `omacom/omarchy-pkgs/pkgbuilds/flea/.omarchy/upstream.sh`
fetched 2026-09-11, not against a local checkout.

- Tag `v0.2.0` matches the strict `^v([0-9]+\.[0-9]+\.[0-9]+)$` pattern. All eight
  published stable releases do, so the updater's hard-exit on an unusable tag never fires.
- Release is not a draft and not a prerelease, and is the highest eligible semver.
- `SHASUMS256.txt` yields exactly one 64-hex value for `flea-v0.2.0.tar.gz`. The
  downloaded archive hashes to `e4f260bea1c743af27dccbf3e6a486cedd3da10120a0f96e420592cafc973f9d`,
  matching the manifest.
- The archive has a single root, `flea-0.2.0`, which is what the updater requires.
- All ten files the updater extracts are present. A missing one would hard-fail the
  updater under `set -euo pipefail`, not merely fail a grep.
- All nine required security-fix greps pass.
- `cargo build --frozen` and `cargo test --frozen --release` both work from the tarball:
  `Cargo.lock` ships, declares version 0.2.0, and the crate has no dependencies.
- Every file the OPR `package()` installs exists in the tarball with the right exec bits.
- `./tests/js.sh` exits 0 (9 QML checks) and `./tests/keymap-gen.sh` exits 0.

0.2.1 must not regress any of the above. The nine security-fix grep strings are the
easiest to break by accident, because they are literal source fragments and a refactor
that only moves code will still fail the updater. They are listed in `upstream.sh`; read
that file before touching `src/backend/archive.rs`, `archiveops.rs`, `run.rs`,
`archivereq.rs`, `archivework.rs`, `mediaprobe.rs`, `metareq.rs`, `copyfile.rs`,
`regfile.rs` or `ui/ShareLink.qml`.

## The defect

Flea already owns the correct mechanism. `src/backend/sandboxprobe.rs` runs the real
production jail argv against `/usr/bin/true` and returns true when it cannot work, which
covers both constrained shapes: bwrap absent from PATH, and bwrap present but forbidden
to create a namespace. Its own header comment records that it exists because "bwrap sits
on PATH inside a container and still cannot build a namespace there".

Ten tests call it and self-skip correctly. Three added in 0.2.0 do not:

| Test | Location at v0.2.0 | What it does instead |
|---|---|---|
| `backend::archiveops::tests::a_silent_failure_names_the_operation_that_was_running` | `src/backend/archiveops.rs:307` | No guard at all, while its four siblings in the same module have one |
| `tui::job::tests::preview_worker_reads_held_source_in_readonly_sandbox` | `src/tui/job.rs:106` | `assert!(sandbox::available(), ...)` at line 107 |
| `tui::job::tests::missing_preview_source_reports_plain_worker_error` | `src/tui/job.rs:120` | `assert!(sandbox::available(), ...)` at line 121 |

`sandbox::available()` only reads PATH. It answers true inside the Omarchy build
container, where bwrap is installed but namespace creation is denied, so the assert
passes and the test then fails further down on a real assertion. That is why the two
`tui/job.rs` tests are wrong twice over: they assert where they should skip, and they
consult the weaker of the two checks.

`src/tui` is new in 0.2.0, which is why this class of regression appeared now. 152 tests
were added between v0.1.6 and v0.2.0.

## Evidence

Both constrained shapes were reproduced on minipc against the published v0.2.0 tarball,
built with `cargo test --frozen --release --no-run` in a `mktemp -d` sandbox.

**Real builder condition.** `systemd-run --user -p RestrictNamespaces=yes` gives exactly
the container's shape: bwrap on PATH, namespace creation refused. bwrap under it prints

```
bwrap: No permissions to create a new namespace, likely because the kernel does not allow non-privileged user namespaces.
```

which is the sample stderr `sandboxprobe.rs` documents.

With the current OPR skip list applied, the suite exits 101:

```
test result: FAILED. 590 passed; 2 failed; 0 ignored; 0 measured; 10 filtered out
    backend::archiveops::tests::a_silent_failure_names_the_operation_that_was_running
    tui::job::tests::preview_worker_reads_held_source_in_readonly_sandbox
```

With no skip arguments at all, ten tests print their own `SKIP` line and the same two
fail:

```
test result: FAILED. 600 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out
```

Two of the self-skipping tests, `backend::thumbs::tests::an_unwinding_fixture_joins_its_worker_before_removing_the_sandbox`
and `backend::sandbox::tests::a_real_sandboxed_child_is_held_to_two_gibibytes_of_address_space`,
are not in the OPR skip list and still behave correctly, which is the guard working.

**Second condition, bwrap absent from PATH.** Same binary, PATH rebuilt as symlinks to
`/usr/bin` minus bwrap and prlimit, no skip arguments:

```
test result: FAILED. 599 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out
    backend::archiveops::tests::a_silent_failure_names_the_operation_that_was_running
    tui::job::tests::missing_preview_source_reports_plain_worker_error
    tui::job::tests::preview_worker_reads_held_source_in_readonly_sandbox
```

`missing_preview_source_reports_plain_worker_error` passes under the first condition and
fails under the second, so the failing set is two or three depending on the builder. Both
conditions need the same fix.

**Positive control.** v0.1.6, same stripped PATH, same OPR skip list: `431 passed; 0 failed`.
So 0.2.0 is the regression and the harness is sound.

**Unconstrained control.** v0.2.0 full suite on the box with a working sandbox:
`602 passed; 0 failed`.

**Proof the amendment works.** v0.2.0 with the three names added to the skip list:
`589 passed; 0 failed`. This is the OPR-side workaround, not the fix 0.2.1 should ship.

## The fix

Three edits, all in test code, no product behavior change.

1. `src/backend/archiveops.rs`, in `a_silent_failure_names_the_operation_that_was_running`,
   add the guard its four siblings already carry as the first line of the body:

   ```rust
   if crate::backend::sandboxprobe::skipped() { return; }
   ```

2. `src/tui/job.rs`, in `preview_worker_reads_held_source_in_readonly_sandbox`, replace
   `assert!(sandbox::available(), "preview worker test requires bwrap and prlimit");`
   with the same guard.

3. `src/tui/job.rs`, in `missing_preview_source_reports_plain_worker_error`, the same
   replacement.

Use the existing helper rather than inventing a second one, and use `sandboxprobe::skipped()`
rather than `sandbox::available()`, because only the former actually executes the jail.

Then sweep the suite once for any other test that reaches the sandbox without the guard,
by rerunning both constrained conditions below and requiring zero failures. A new test
that needs the sandbox and forgets the guard is the same defect again.

## Reproduce after the fix

On minipc, from a `mktemp -d` of its own, never inside `$HOME` or an existing checkout:

```bash
d=$(mktemp -d /tmp/flea-021-check.XXXXXX); cd "$d"
# build the candidate the same way the OPR builder does
export CARGO_TARGET_DIR="$d/target"
cargo test --frozen --release --no-run
bin=$(ls "$d"/target/release/deps/flea-* | grep -v '\.d$' | head -1)

# condition A: the real builder, bwrap present and namespaces denied
systemd-run --user --wait --pipe --collect -q -p RestrictNamespaces=yes "$bin" --test-threads=4

# condition B: bwrap and prlimit absent from PATH
fake="$d/fakebin"; mkdir -p "$fake"
for f in /usr/bin/*; do n=$(basename "$f"); [ "$n" = bwrap ] && continue; [ "$n" = prlimit ] && continue; ln -sf "$f" "$fake/$n"; done
PATH="$fake" "$bin" --test-threads=4
```

Both must report `0 failed`. Read the exit status, not the tail of the output.
Remove the sandbox directory afterwards.

## What 0.2.1 does not fix

**OPR dependency drift, needs a PR to omacom/omarchy-pkgs.** The OPR PKGBUILD is missing
`kimageformats` and `libheif`. Flea's own PKGBUILD gained both at v0.1.6 in commit
`0d70a9d`, "fix: HEIC previews decode through kimageformats". The OPR sync bot rewrites
only `pkgver` and `sha256sums`, confirmed by its own commit `6ce22c36`, so it never
carried them across. OPR users have had no HEIC preview since 0.1.6 and 0.2.1 will not
change that on its own. This needs a separate OPR pull request and GM's explicit
authorization; OPR upstream mutation is not covered by a Flea release.

**Undeclared poppler.** The new TUI PDF preview execs `/usr/bin/pdfinfo` and
`/usr/bin/pdftoppm`, both owned by `poppler`, which neither PKGBUILD declares in `depends`
or `optdepends`. It degrades to a plain error rather than crashing. Adding
`'poppler: PDF preview in the terminal UI'` to the repo PKGBUILD's `optdepends` helps the
AUR packages; it does not reach OPR, whose optdepends list is also maintained by hand.

## Release mechanics to observe

- Bump `Cargo.toml`, `Cargo.lock` and the repo `PKGBUILD` together before the final
  battery, not after it.
- Publish `flea-v0.2.1.tar.gz` and `SHASUMS256.txt`. Build from the annotated tag with
  suppressed gzip metadata, build twice in separate temporary directories and compare
  bytes, and verify the single `flea-0.2.1/` root. Generated GitHub archives are not
  release inputs.
- Published tags and assets are immutable. A defect found after publishing is repaired
  with another patch version, never by retagging or replacing an asset.
- The OPR updater selects the highest eligible release, so shipping 0.2.1 makes OPR skip
  0.2.0 entirely. No OPR-side change is needed for the blocker once 0.2.1 is out.
- OPR enforces a 24 hour minimum release age. That belongs to Omarchy's automation. Do
  not wait on it, probe it, or set `BYPASS_MIN_RELEASE_AGE=1`.
- Commits are authored as `GM <gianmarcomorales@icloud.com>`, verified with
  `git log -1 --format="%an <%ae>"` before pushing. No AI attribution anywhere.
