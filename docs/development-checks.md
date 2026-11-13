# Development checks: hk hooks and the shared Clippy gate

One checker and one baseline serve both the local hooks and the external CI lane:
`tools/flea-clippy` judges, `tools/clippy-baseline.txt` holds the per-lint ratchet.
`tools/flea-commit-rules` judges commit ranges, `tools/flea-commit-msg` judges the
proposed message through the same policy. No package or runtime dependency was added;
every checker is stdlib-only Python 3; only the hook installer is POSIX shell.

## Matching versions

hk 2.4.0, Pkl schema 2.4.0 (hk embeds its own package, so no Pkl CLI is fetched),
Clippy 0.1.100 (0ef1bc3860 2026-11-09) from the Rust 1.100.0 toolchain selected by
`rust-toolchain.toml` when using rustup (distribution Cargo ignores that file),
Git 2.47.3 with per-repo legacy shims
(config-based hooks need Git 2.54+, which this tree does not assume).
`tools/flea-hooks-install` refuses any other hk version.

## Install and run

```
tools/flea-hooks-install            # per-repo legacy shims only, never global config
HK_PKL_OFFLINE=1 hk validate        # config only
HK_PKL_OFFLINE=1 hk check --all     # full local gate: clippy, file budget
```

Those two are the semantic check-hook CLI surface, confirmed against hk 2.4.0 `--help` on the host.

Re-running the installer over its own shims updates them. It refuses to overwrite
a hook file it did not install; move an unrelated hook aside and retry. A set
`core.hooksPath` points outside the repo, so the installer refuses it before
writing anything; linked worktrees without one install normally.

## What each hook runs

Pre-commit is read-only (`fix = false`, `stage = false`, `stash = "none"`):
the partial-index guard gates Clippy and the file budget, which run once it
passes. The commit-msg hook runs the message adapter with the hook's own `{{commit_msg_file}}` argument and
identity from `git var`; while git records a merge (`MERGE_HEAD` is set, including the `git commit` that
concludes a conflicted one), the adapter judges the message by the merge policy CI applies to a merge commit.
There is no pre-push hook and no formatter or autofix.

## Partial-staging refusal

Clippy reads the working tree while the hook stashes nothing, so a change that is
not fully staged to a gate or compile input (`src/`, `ui/`, `tests/`, `tools/`,
`Cargo.*`, `hk.pkl`, `keys.toml`, `AGENTS.md`) would be judged instead of the
staged commit. That covers untracked inputs too: staged code naming a module the
index does not hold would not compile as committed. Ignored inputs are scanned
as well, since the compiler reads them; only `target/` and `.flea-local/` are
runtime artifacts and stay skipped. Build configuration counts
as a compile input as well: `build.rs`, `rust-toolchain*`, and `.cargo/`. The guard refuses that commit
in one sentence rather than validating other bytes:

```
flea-hook-check: refused: <file> is not fully staged, so the hook would judge
working-tree bytes rather than the staged commit; stage it, or remove or restore
it, and retry
```

It never stages, unstashes, or rewrites the index, and it runs only in pre-commit:
`hk check` stays usable with ordinary modified files. Every git failure fails
closed with exit 2. The exact archived-head CI lane remains required.

## Clippy policy

`cargo clippy --locked --all-targets --message-format=json -- -W clippy::correctness`.
Every `correctness` or `suspicious` finding fails, every rustc warning fails, and
every other lint is a per-lint ratchet against the baseline: a risen count fails,
a fallen count prints LOWER so the baseline can drop. Unknown lints or groups,
garbage or malformed stream lines, a missing build-finished or unchecked flea
target, and a nonzero cargo exit are ERROR (exit 2), never green. Only cargo's own
reason and level names are read; note, help and failure-note are skipped, and
build-script records carry no verdict. A span-less warning keys on its text, so
distinct findings never collapse into one ratchet count, and every finding prints
with no FAIL line capped. Exit 0 pass, 1 fail, 2 unjudgeable. Concurrent calls on
one OUTDIR serialize on an exclusive lock covering write, run, parse and publish;
the cargo target cache is untouched, only OUTDIR verdict and log artifacts are fenced. Whole-group allows and MSRV suppression are not
offered: `incompatible_msrv` and `zombie_processes` stay denied.

The baseline is seeded from staging `92e34c1690bda2169160b5744fadd42680a2b502`
under Clippy 0.1.98, recorded at the top of `tools/clippy-baseline.txt`. A baseline
that is not UTF-8, or names a lint outside the driver's own inventory, is ERROR.
A missing `clippy-driver` is ERROR too. Ratchet
updates after Rust fixes land sit with the external CI authority, beside the lane adoption.

## Commit-message policy

GM as author and committer, a conventional subject of at most 60 chars with a
description as short as one nonspace char, a body of
2 to 4 lines after one blank line, and no AI attribution, em dash, or emoji. At most one
trailing `(cherry picked from commit ...)` line is excluded; further ones count
as body. Attribution is phrase-scoped on complete word tokens: generated with, by or
using beside a provider name fails, while billing nouns, API credit displays and other
technical mentions of CLAUDE.md, Claude, Anthropic, Codex or provider
tools in ordinary prose pass, and a human name that merely contains one stays human.
Dotted filename spans are masked before scanning, and credit names split on any
non-word run plus underscore, so hyphen and underscore qualifiers cannot smuggle a brand past while non-Latin
human names pass untouched. A credit naming a machine brand fails whatever qualifier follows it: ChatGPT and Codex
alone, the Claude Code and OpenAI Codex compounds, one exact AI name, or two AI
tokens; a company email address never classifies its holder, and a human word
beside one AI token stays human. Emoji follows frozen Unicode 17 Emoji_Presentation
ranges, so the watch fails while supplementary CJK, digits, `#` and `*` stay text. Old
history has no exception. Trailing `Co-authored-by: Name <email>` lines are human
credit and sit outside the 2 to 4 prose lines; a misshapen, AI, or bot credit
fails, as does credit anywhere but the trailers. The range log holds seven-field NUL-terminated records
with no empties, judged 0 pass, 1 fail, 2 unjudgeable. The adapter reduces the file
to one canonical form whatever the repo cleanup says: comment lines and the exact
scissors section go, space trims, edge blanks go, and only then is it judged. Comment
precedence is git's: `core.commentString` wins over `core.commentChar`, including a
multi-character prefix git honors; empty, newline-carrying and `auto` values refuse.
`commit.cleanup=default` is accepted as git accepts it. Modes
are validated but never select a normalization; unknown modes refuse rather
than guess. A passed message is written back to the file, so the bytes git stores
under any CLI cleanup are exactly the bytes judged; a refused message is left untouched. The CLI cleanup
flag is invisible to the hook, so this fixed point is what keeps `git commit -F`,
editing, commentChar overrides, scissors sections, and no-edit amends agreeing
with the range checker.

## CI authority

The external CI lane adopts `tools/flea-clippy` and `tools/clippy-baseline.txt`
after acceptance instead of carrying a second implementation. An archive carries
no `.git`, so the commit-range lane stays beside the hooks. The checkers change
no tracked file: each writes only its own private OUTDIR build and log artifacts.
`tests/hook-gate.sh` proves the parsers through stub transports (marked STUB) and
the git/hk wiring where the pinned hk exists (marked REAL); the real image Clippy
run with seeded reds runs outside this suite.
