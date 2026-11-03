#!/bin/bash
# Gates the shared hook checkers through marked-root fixtures.
# Cases marked STUB use a stub transport; cases marked REAL execute git and hk where present.
set -u
# The cd comes first: the trap below deletes a path under TMPDIR, never the caller's tree.
cd "$(dirname "$0")/.." || exit 1
repo=$PWD

root=$(mktemp -d "${TMPDIR:-/tmp}/flea-hook-gate.XXXXXX") || exit 1
# Every delete in this suite is checked absolute and inside this root before the trap is installed.
case $root in
  /*/flea-hook-gate.?*) ;;
  *) echo "FAIL: mktemp -d gave '$root', refusing to own it"; exit 1 ;;
esac
touch "$root/.flea-hook-gate-root" || exit 1
trap 'case "$root" in /*/flea-hook-gate.?*) rm -rf -- "$root" ;; esac' EXIT

# A private HOME so git and hk never read or write the operator's own config, state or hooks.
export HOME="$root/home"
mkdir -p "$HOME" "$root/out" "$root/fix"
export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null
export HK_PKL_OFFLINE=1
# The adapter reads identity from git var, which honours these four over any config file.
export GIT_AUTHOR_NAME=GM GIT_AUTHOR_EMAIL=gianmarcomorales@icloud.com
export GIT_COMMITTER_NAME=GM GIT_COMMITTER_EMAIL=gianmarcomorales@icloud.com

fail=0
check() {
  local label="$1" expected="$2" actual="$3"
  if [ "$expected" != "$actual" ]; then
    echo "FAIL $label: expected exit $expected, got $actual"
    fail=1
  else
    echo "ok   $label"
  fi
}
expect_grep() {
  local label="$1" file="$2" pattern="$3"
  if grep -qF "$pattern" "$file"; then
    echo "ok   $label"
  else
    echo "FAIL $label: no line '$pattern' in $file"
    fail=1
  fi
}

# One COMMITS_Z record out of shell-quoted fields; %B is passed through verbatim.
zrec() {
  python3 -c 'import sys; sys.stdout.buffer.write(b"\x1f".join(a.encode() for a in sys.argv[1:]) + b"\0")' "$@"
}

SHA=$(python3 -c 'print("a" * 40)')
GOOD_B=$'fix(hooks): gate the shared checkers through one hook\n\nFirst body line.\nSecond body line.'

# Commit rules over a staged log, the same shape the CI lane stages.
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$GOOD_B" > "$root/fix/good.z"
"$repo/tools/flea-commit-rules" "$root/fix/good.z" "$root/out" > "$root/fix/good.out" 2>&1
check "commit-rules accepts a good GM commit" 0 $?
expect_grep "commit-rules summary is PASS" "$root/fix/good.out" "commit-rules: PASS fails=0 commits=1"

zrec "$SHA" Someone x@y.z GM gianmarcomorales@icloud.com "" "$GOOD_B" > "$root/fix/ident.z"
"$repo/tools/flea-commit-rules" "$root/fix/ident.z" "$root/out" > "$root/fix/ident.out" 2>&1
check "commit-rules rejects a wrong identity" 1 $?
expect_grep "commit-rules names the identity" "$root/fix/ident.out" "identity an=Someone <x@y.z>"

LONG_B=$'fix(hooks): a subject that runs past sixty characters and keeps going\n\nFirst body line.\nSecond body line.'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$LONG_B" > "$root/fix/long.z"
"$repo/tools/flea-commit-rules" "$root/fix/long.z" "$root/out" > "$root/fix/long.out" 2>&1
check "commit-rules rejects a long subject" 1 $?
expect_grep "commit-rules counts the subject" "$root/fix/long.out" "want at most 60"

zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "fix(hooks): one line and no body" > "$root/fix/body.z"
"$repo/tools/flea-commit-rules" "$root/fix/body.z" "$root/out" > "$root/fix/body.out" 2>&1
check "commit-rules rejects a missing body" 1 $?
expect_grep "commit-rules wants the blank line" "$root/fix/body.out" "body needs one blank line after the subject"

# A copy of the checker reads its own .known beside it, so these cases never touch the recorded allowlist.
mkdir -p "$root/known" && cp "$repo/tools/flea-commit-rules" "$root/known/flea-commit-rules"
printf '%s\tbody needs one blank line after the subject\n' "$SHA" > "$root/known/flea-commit-rules.known"
"$root/known/flea-commit-rules" "$root/fix/body.z" "$root/out" > "$root/fix/known.out" 2>&1
check "commit-rules waives a recorded failure" 0 $?
expect_grep "commit-rules names the waived failure" "$root/fix/known.out" "KNOWN aaaaaaaaaaaa body needs one blank line after the subject"
"$root/known/flea-commit-rules" "$root/fix/long.z" "$root/out" > "$root/fix/known-other.out" 2>&1
check "commit-rules still judges a known commit's other failures" 1 $?
expect_grep "commit-rules keeps the unrecorded failure" "$root/fix/known-other.out" "want at most 60"
expect_grep "commit-rules flags a recorded failure that is gone" "$root/fix/known-other.out" "known line no longer fails: body needs one blank line after the subject"
printf '%s body needs one blank line after the subject\n' "$SHA" > "$root/known/flea-commit-rules.known"
"$root/known/flea-commit-rules" "$root/fix/body.z" "$root/out" > "$root/fix/known-bad.out" 2>&1
check "commit-rules refuses a malformed known line" 2 $?
# Sample input (malformed known lines): an uppercase sha, the same line twice, a sha with a blank rule.
UPPER=${SHA^^}
for bad in upper twice blank; do
  case $bad in
    upper) printf '%s\tbody holds a blank line\n' "$UPPER" ;;
    twice) printf '%s\tbody holds a blank line\n%s\tbody holds a blank line\n' "$SHA" "$SHA" ;;
    blank) printf '%s\t \n' "$SHA" ;;
  esac > "$root/known/flea-commit-rules.known"
  "$root/known/flea-commit-rules" "$root/fix/good.z" "$root/out" > "$root/fix/known-$bad.out" 2>&1
  check "commit-rules refuses a known line: $bad" 2 $?
  expect_grep "commit-rules names the $bad known line" "$root/fix/known-$bad.out" "want one unique 40-hex sha, a tab and the failure it records"
done
mkdir -p "$root/bare" && cp "$repo/tools/flea-commit-rules" "$root/bare/flea-commit-rules"
"$root/bare/flea-commit-rules" "$root/fix/good.z" "$root/out" > "$root/fix/bare.out" 2>&1
check "commit-rules judges without an allowlist" 0 $?
expect_grep "commit-rules says the allowlist is absent" "$root/fix/bare.out" "NOTE no recorded allowlist at $root/bare/flea-commit-rules.known"
# The shipped checker reads the shipped allowlist: a recorded sha with a clean message reports its line stale.
shipped=$(grep -m 1 -v -e '^#' -e '^$' "$repo/tools/flea-commit-rules.known")
zrec "${shipped%%$'\t'*}" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$GOOD_B" > "$root/fix/shipped.z"
"$repo/tools/flea-commit-rules" "$root/fix/shipped.z" "$root/out" > "$root/fix/shipped.out" 2>&1
check "commit-rules reads the allowlist shipped beside it" 1 $?
expect_grep "commit-rules names the shipped known line" "$root/fix/shipped.out" "known line no longer fails: ${shipped#*$'\t'}"

ATTR_B=$'fix(hooks): a fine subject\n\nFirst body line.\nGenerated with Claude.\n'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$ATTR_B" > "$root/fix/attr.z"
"$repo/tools/flea-commit-rules" "$root/fix/attr.z" "$root/out" > "$root/fix/attr.out" 2>&1
check "commit-rules rejects AI attribution" 1 $?
expect_grep "commit-rules names the attribution" "$root/fix/attr.out" "attribution 'claude'"

CREDIT_B=$'fix(hooks): gate human credit\n\nFirst body line.\nSecond body line.\n\nCo-authored-by: Ada Lovelace <ada@example.com>'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$CREDIT_B" > "$root/fix/credit.z"
"$repo/tools/flea-commit-rules" "$root/fix/credit.z" "$root/out" > "$root/fix/credit.out" 2>&1
check "commit-rules accepts a human credit trailer" 0 $?
BADCREDIT_B=$'fix(hooks): gate human credit\n\nFirst body line.\nSecond body line.\nCo-authored-by: ada'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$BADCREDIT_B" > "$root/fix/badcredit.z"
"$repo/tools/flea-commit-rules" "$root/fix/badcredit.z" "$root/out" > "$root/fix/badcredit.out" 2>&1
check "commit-rules rejects a malformed credit trailer" 1 $?
expect_grep "commit-rules names the malformed trailer" "$root/fix/badcredit.out" "malformed co-authored-by trailer"
MIDBODY_B=$'fix(hooks): gate human credit\n\nCo-authored-by: Ada Lovelace <ada@example.com>\nSecond body line.'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$MIDBODY_B" > "$root/fix/midbody.z"
"$repo/tools/flea-commit-rules" "$root/fix/midbody.z" "$root/out" > "$root/fix/midbody.out" 2>&1
check "commit-rules rejects a misplaced credit line" 1 $?
expect_grep "commit-rules names the misplaced line" "$root/fix/midbody.out" "misplaced co-authored-by line"
AICREDIT_B=$'fix(hooks): gate human credit\n\nFirst body line.\nSecond body line.\nCo-authored-by: Claude <noreply@anthropic.com>'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$AICREDIT_B" > "$root/fix/aicredit.z"
"$repo/tools/flea-commit-rules" "$root/fix/aicredit.z" "$root/out" > "$root/fix/aicredit.out" 2>&1
check "commit-rules rejects an AI credit trailer" 1 $?
expect_grep "commit-rules names the AI credit" "$root/fix/aicredit.out" "names an AI author"

EM_B=$'fix(hooks): a fine subject\n\nFirst body \u2014 line.\nSecond body line.'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$EM_B" > "$root/fix/em.z"
"$repo/tools/flea-commit-rules" "$root/fix/em.z" "$root/out" > "$root/fix/em.out" 2>&1
check "commit-rules rejects an em dash" 1 $?
expect_grep "commit-rules names the em dash" "$root/fix/em.out" "em dash U+2014"

TRAIL_B=$'fix(hooks): a fine subject\n\nFirst body line.\nSecond body line.\n\n(cherry picked from commit 876774988e2051f26c26e7237617e3ae9ad77a67)'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$TRAIL_B" > "$root/fix/trail.z"
"$repo/tools/flea-commit-rules" "$root/fix/trail.z" "$root/out" > "$root/fix/trail.out" 2>&1
check "commit-rules excludes the cherry-pick trailer" 0 $?

DOUBLECP_B=$'fix(hooks): a fine subject\n\nFirst body line.\n(cherry picked from commit 876774988e2051f26c26e7237617e3ae9ad77a67)\n(cherry picked from commit 876774988e2051f26c26e7237617e3ae9ad77a67)'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$DOUBLECP_B" > "$root/fix/doublecp.z"
"$repo/tools/flea-commit-rules" "$root/fix/doublecp.z" "$root/out" > "$root/fix/doublecp.out" 2>&1
check "commit-rules counts a second cherry-pick line as body" 0 $?

CLAUDEMD_B=$'fix(hooks): mention the workflow\n\nSee CLAUDE.md for the workflow.\nSecond body line.'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$CLAUDEMD_B" > "$root/fix/claudemd.z"
"$repo/tools/flea-commit-rules" "$root/fix/claudemd.z" "$root/out" > "$root/fix/claudemd.out" 2>&1
check "commit-rules passes a CLAUDE.md mention" 0 $?

CODEX_B=$'fix(hooks): mention the tool\n\nMentioned Codex in the design notes.\nSecond body line.'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$CODEX_B" > "$root/fix/codex.z"
"$repo/tools/flea-commit-rules" "$root/fix/codex.z" "$root/out" > "$root/fix/codex.out" 2>&1
check "commit-rules passes a Codex mention" 0 $?

PAIR_B=$'fix(hooks): pair work\n\nPair-programmed with Codex on this.\nSecond body line.'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$PAIR_B" > "$root/fix/pair.z"
"$repo/tools/flea-commit-rules" "$root/fix/pair.z" "$root/out" > "$root/fix/pair.out" 2>&1
check "commit-rules rejects verb-scoped AI credit" 1 $?
expect_grep "commit-rules names the provider" "$root/fix/pair.out" "attribution 'codex'"

REPAIR_B=$'fix(hooks): repair Codex integration\n\nFirst body line.\nSecond body line.'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$REPAIR_B" > "$root/fix/repair.z"
"$repo/tools/flea-commit-rules" "$root/fix/repair.z" "$root/out" > "$root/fix/repair.out" 2>&1
check "commit-rules passes repair beside a provider name" 0 $?

SHANNON_B=$'fix(hooks): gate human credit\n\nFirst body line.\nSecond body line.\n\nCo-authored-by: Claude Shannon <claude@example.com>'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$SHANNON_B" > "$root/fix/shannon.z"
"$repo/tools/flea-commit-rules" "$root/fix/shannon.z" "$root/out" > "$root/fix/shannon.out" 2>&1
check "commit-rules accepts a human Claude credit" 0 $?

GPTCREDIT_B=$'fix(hooks): gate human credit\n\nFirst body line.\nSecond body line.\nCo-authored-by: ChatGPT <noreply@openai.com>'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$GPTCREDIT_B" > "$root/fix/gptcredit.z"
"$repo/tools/flea-commit-rules" "$root/fix/gptcredit.z" "$root/out" > "$root/fix/gptcredit.out" 2>&1
check "commit-rules rejects a ChatGPT credit trailer" 1 $?
expect_grep "commit-rules names the ChatGPT credit" "$root/fix/gptcredit.out" "names an AI author"

GPTPROSE_B=$'fix(hooks): gate credit\n\nGenerated with ChatGPT.\nSecond body line.'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$GPTPROSE_B" > "$root/fix/gptprose.z"
"$repo/tools/flea-commit-rules" "$root/fix/gptprose.z" "$root/out" > "$root/fix/gptprose.out" 2>&1
check "commit-rules rejects ChatGPT prose credit" 1 $?
expect_grep "commit-rules names the ChatGPT prose" "$root/fix/gptprose.out" "attribution 'chatgpt'"

FILENAME_B=$'docs: update CLAUDE.md author rules\n\nKeep the shared hook message contract.\nCheck the exact stored commit bytes.'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$FILENAME_B" > "$root/fix/filename.z"
"$repo/tools/flea-commit-rules" "$root/fix/filename.z" "$root/out" > "$root/fix/filename.out" 2>&1
check "commit-rules passes a filename beside policy prose" 0 $?

LIMING_B=$'fix(hooks): keep credit policy\n\nKeep the shared hook message contract.\nCheck the exact stored commit bytes.\n\nCo-authored-by: \u674e\u660e <li@example.com>'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$LIMING_B" > "$root/fix/liming.z"
"$repo/tools/flea-commit-rules" "$root/fix/liming.z" "$root/out" > "$root/fix/liming.out" 2>&1
check "commit-rules accepts a non-Latin human credit" 0 $?

HYPHENAI_B=$'fix(hooks): keep credit policy\n\nKeep the shared hook message contract.\nCheck the exact stored commit bytes.\n\nCo-authored-by: ChatGPT-4o <noreply@openai.com>'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$HYPHENAI_B" > "$root/fix/hyphenai.z"
"$repo/tools/flea-commit-rules" "$root/fix/hyphenai.z" "$root/out" > "$root/fix/hyphenai.out" 2>&1
check "commit-rules rejects a hyphen-qualified AI credit" 1 $?
expect_grep "commit-rules names the hyphenated credit" "$root/fix/hyphenai.out" "names an AI author"

USCOREAI_B=$'fix(hooks): keep credit policy\n\nKeep the shared hook message contract.\nCheck the exact stored commit bytes.\n\nCo-authored-by: ChatGPT_4o <noreply@openai.com>'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$USCOREAI_B" > "$root/fix/uscoreai.z"
"$repo/tools/flea-commit-rules" "$root/fix/uscoreai.z" "$root/out" > "$root/fix/uscoreai.out" 2>&1
check "commit-rules rejects an underscore-qualified AI credit" 1 $?
expect_grep "commit-rules names the underscore credit" "$root/fix/uscoreai.out" "names an AI author"

OAICREDIT_B=$'fix(hooks): gate human credit\n\nFirst body line.\nSecond body line.\nCo-authored-by: OpenAI Codex <noreply@openai.com>'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$OAICREDIT_B" > "$root/fix/oaicredit.z"
"$repo/tools/flea-commit-rules" "$root/fix/oaicredit.z" "$root/out" > "$root/fix/oaicredit.out" 2>&1
check "commit-rules rejects a compound AI credit" 1 $?
expect_grep "commit-rules names the compound credit" "$root/fix/oaicredit.out" "names an AI author"

GENBY_B=$'fix(hooks): gate credit\n\nGenerated by ChatGPT.\nSecond body line.'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$GENBY_B" > "$root/fix/genby.z"
"$repo/tools/flea-commit-rules" "$root/fix/genby.z" "$root/out" > "$root/fix/genby.out" 2>&1
check "commit-rules rejects generated-by prose" 1 $?
expect_grep "commit-rules names the generated-by prose" "$root/fix/genby.out" "attribution 'chatgpt'"

GENUSING_B=$'fix(hooks): gate credit\n\nGenerated using Codex.\nSecond body line.'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$GENUSING_B" > "$root/fix/genusing.z"
"$repo/tools/flea-commit-rules" "$root/fix/genusing.z" "$root/out" > "$root/fix/genusing.out" 2>&1
check "commit-rules rejects generated-using prose" 1 $?
expect_grep "commit-rules names the generated-using prose" "$root/fix/genusing.out" "attribution 'codex'"

BILLING_B=$'fix(openai): display remaining API credits\n\nFirst body line.\nSecond body line.'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$BILLING_B" > "$root/fix/billing.z"
"$repo/tools/flea-commit-rules" "$root/fix/billing.z" "$root/out" > "$root/fix/billing.out" 2>&1
check "commit-rules passes billing prose" 0 $?

CLAUDETTE_B=$'fix(hooks): validate Claudette author identities\n\nFirst body line.\nSecond body line.'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$CLAUDETTE_B" > "$root/fix/claudette.z"
"$repo/tools/flea-commit-rules" "$root/fix/claudette.z" "$root/out" > "$root/fix/claudette.out" 2>&1
check "commit-rules passes a Claudette subject" 0 $?

PAREN_B=$'fix(hooks): gate human credit\n\nFirst body line.\nSecond body line.\nCo-authored-by: ChatGPT (OpenAI) <noreply@openai.com>'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$PAREN_B" > "$root/fix/paren.z"
"$repo/tools/flea-commit-rules" "$root/fix/paren.z" "$root/out" > "$root/fix/paren.out" 2>&1
check "commit-rules rejects a parenthesized AI credit" 1 $?
expect_grep "commit-rules names the parenthesized credit" "$root/fix/paren.out" "names an AI author"

CLICREDIT_B=$'fix(hooks): gate human credit\n\nFirst body line.\nSecond body line.\nCo-authored-by: OpenAI Codex CLI <noreply@openai.com>'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$CLICREDIT_B" > "$root/fix/clicredit.z"
"$repo/tools/flea-commit-rules" "$root/fix/clicredit.z" "$root/out" > "$root/fix/clicredit.out" 2>&1
check "commit-rules rejects a qualified machine credit" 1 $?
expect_grep "commit-rules names the machine credit" "$root/fix/clicredit.out" "names an AI author"

for brand in "ChatGPT CLI" "Codex CLI" "Claude Code"; do
  slug=$(printf '%s' "$brand" | tr 'A-Z ' 'a-z-')
  brand_b=$(printf 'fix(hooks): gate human credit\n\nFirst body line.\nSecond body line.\nCo-authored-by: %s <bot@example.com>\n' "$brand")
  zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$brand_b" > "$root/fix/brand-$slug.z"
  "$repo/tools/flea-commit-rules" "$root/fix/brand-$slug.z" "$root/out" > "$root/fix/brand-$slug.out" 2>&1
  check "commit-rules rejects a $brand credit" 1 $?
  expect_grep "commit-rules names the $brand credit" "$root/fix/brand-$slug.out" "names an AI author"
done

JANE_B=$'fix(hooks): gate human credit\n\nFirst body line.\nSecond body line.\n\nCo-authored-by: Jane Doe <jane@anthropic.com>'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$JANE_B" > "$root/fix/jane.z"
"$repo/tools/flea-commit-rules" "$root/fix/jane.z" "$root/out" > "$root/fix/jane.out" 2>&1
check "commit-rules accepts a company-email human credit" 0 $?

MACHINE_B=$'fix(hooks): gate credit\n\nBuilt with OpenAI Codex CLI.\nSecond body line.'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$MACHINE_B" > "$root/fix/machine.z"
"$repo/tools/flea-commit-rules" "$root/fix/machine.z" "$root/out" > "$root/fix/machine.out" 2>&1
check "commit-rules rejects qualified machine prose" 1 $?

BUILTIN_B=$'fix(hooks): gate credit\n\nShipped built-in Codex support.\nSecond body line.'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$BUILTIN_B" > "$root/fix/builtin.z"
"$repo/tools/flea-commit-rules" "$root/fix/builtin.z" "$root/out" > "$root/fix/builtin.out" 2>&1
check "commit-rules passes built-in provider prose" 0 $?

BRACKET_B=$'fix(hooks): gate human credit\n\nFirst body line.\nSecond body line.\nCo-authored-by: Ada <ada@example.com>>'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$BRACKET_B" > "$root/fix/bracket.z"
"$repo/tools/flea-commit-rules" "$root/fix/bracket.z" "$root/out" > "$root/fix/bracket.out" 2>&1
check "commit-rules rejects an extra email bracket" 1 $?
expect_grep "commit-rules names the bracket" "$root/fix/bracket.out" "malformed co-authored-by trailer"

ONECHAR_B=$'fix: x\n\nBody one.\nBody two.'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$ONECHAR_B" > "$root/fix/onechar.z"
"$repo/tools/flea-commit-rules" "$root/fix/onechar.z" "$root/out" > "$root/fix/onechar.out" 2>&1
check "commit-rules accepts a one-character description" 0 $?

WATCH_B=$'fix(hooks): watch test\n\nSaw \U0000231A today.\nSecond body line.'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$WATCH_B" > "$root/fix/watch.z"
"$repo/tools/flea-commit-rules" "$root/fix/watch.z" "$root/out" > "$root/fix/watch.out" 2>&1
check "commit-rules rejects the watch emoji" 1 $?
expect_grep "commit-rules names the watch" "$root/fix/watch.out" "emoji U+231A"

CJK_B=$'fix(hooks): cjk test\n\nSaw \U00020BB7 today.\nSecond body line.'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$CJK_B" > "$root/fix/cjk.z"
"$repo/tools/flea-commit-rules" "$root/fix/cjk.z" "$root/out" > "$root/fix/cjk.out" 2>&1
check "commit-rules passes supplementary CJK" 0 $?

PLAIN_B=$'fix(hooks): plain test\n\nFixed #42 and 100 items.\nSecond body line.'
zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$PLAIN_B" > "$root/fix/plain.z"
"$repo/tools/flea-commit-rules" "$root/fix/plain.z" "$root/out" > "$root/fix/plain.out" 2>&1
check "commit-rules passes digits and hash as text" 0 $?

zrec "$SHA" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" > "$root/fix/short.z"
"$repo/tools/flea-commit-rules" "$root/fix/short.z" "$root/out" > "$root/fix/short.out" 2>&1
check "commit-rules refuses a malformed record" 2 $?
expect_grep "commit-rules says ERROR" "$root/fix/short.out" "commit-rules: ERROR"

(cd "$root/fix" && python3 -c 'open("bin.z", "wb").write(b"\xff\xfe bad \x1f bytes\0")')
"$repo/tools/flea-commit-rules" "$root/fix/bin.z" "$root/out" > "$root/fix/bin.out" 2>&1
check "commit-rules refuses a non-UTF8 log" 2 $?
expect_grep "commit-rules says UTF-8" "$root/fix/bin.out" "not valid UTF-8"

: > "$root/fix/empty.z"
"$repo/tools/flea-commit-rules" "$root/fix/empty.z" "$root/out" > "$root/fix/empty.out" 2>&1
check "commit-rules refuses an empty log" 2 $?
expect_grep "commit-rules says ERROR" "$root/fix/empty.out" "holds no commit records to judge"
python3 -c 'import sys; sys.stdout.buffer.write(open(sys.argv[1], "rb").read().rstrip(b"\0"))' "$root/fix/good.z" > "$root/fix/nonul.z"
"$repo/tools/flea-commit-rules" "$root/fix/nonul.z" "$root/out" > "$root/fix/nonul.out" 2>&1
check "commit-rules refuses a stream missing its final NUL" 2 $?
expect_grep "commit-rules says ERROR" "$root/fix/nonul.out" "does not end in a record NUL"
python3 -c 'import sys; d = open(sys.argv[1], "rb").read(); open(sys.argv[2], "wb").write(d + b"\0" + d)' "$root/fix/good.z" "$root/fix/hollow.z"
"$repo/tools/flea-commit-rules" "$root/fix/hollow.z" "$root/out" > "$root/fix/hollow.out" 2>&1
check "commit-rules refuses an interior empty record" 2 $?
expect_grep "commit-rules says ERROR" "$root/fix/hollow.out" "holds an empty record"
(cd "$repo" && git log -z --format='%H%x1f%an%x1f%ae%x1f%cn%x1f%ce%x1f%P%x1f%B' -1 HEAD > "$root/fix/real.z")
"$repo/tools/flea-commit-rules" "$root/fix/real.z" "$root/out" > "$root/fix/real.out" 2>&1
if [ $? -eq 2 ]; then echo "FAIL commit-rules errored on a real git log stream"; fail=1; else echo "ok   commit-rules judges a real git log stream"; fi
if grep -q "could not judge" "$root/fix/real.out"; then echo "FAIL commit-rules refused a real git log stream"; fail=1; else echo "ok   commit-rules parsed the real stream"; fi

# Contributor history through GM merges: the PR #229 shape passes whole.
CR_B=$(python3 -c 'print("1" * 40)')
CR_G1=$(python3 -c 'print("2" * 40)')
CR_MID=$(python3 -c 'print("3" * 40)')
CR_G2=$(python3 -c 'print("4" * 40)')
CR_C1=$(python3 -c 'print("5" * 40)')
CR_C2=$(python3 -c 'print("6" * 40)')
CR_C3=$(python3 -c 'print("7" * 40)')
CR_M1=$(python3 -c 'print("8" * 40)')
CR_M2=$(python3 -c 'print("9" * 40)')
CR_T=$(python3 -c 'print("b" * 40)')
CR_M1B=$'Merge PR #229 from alextakitani: middle click opens a folder in a new tab\n\nMerged onto the 0.3.8 integration head; conflicts with the click,\ntab and Recent ports resolved by keeping both sides.'
CR_M2B='Merge the PR #229 middle-click branch into 0.3.8'
CR_C1B=$'feat: a middle click on a folder opens it in a new tab\n\nFirst line with \u2014 dash.\nSecond body line.'
CR_C2B='test(ui): return to the first tab with click_tab, not the tui-only digit'
CR_C3B=$'feat: a middle click on a folder opens it in a new tab\n\nLine one.\nLine two.\nLine three.\nLine four.\nLine five.'
zrec "$CR_T" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "$CR_M2" "$GOOD_B" > "$root/fix/contrib.z"
zrec "$CR_M2" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "$CR_G2 $CR_M1" "$CR_M2B" >> "$root/fix/contrib.z"
zrec "$CR_G2" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "$CR_MID" "$GOOD_B" >> "$root/fix/contrib.z"
zrec "$CR_MID" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "$CR_G1" "$GOOD_B" >> "$root/fix/contrib.z"
zrec "$CR_M1" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "$CR_G1 $CR_C3" "$CR_M1B" >> "$root/fix/contrib.z"
zrec "$CR_G1" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "$CR_B" "$GOOD_B" >> "$root/fix/contrib.z"
zrec "$CR_C3" "Alex Takitani" aftakitani@gmail.com "Alex Takitani" aftakitani@gmail.com "$CR_C2" "$CR_C3B" >> "$root/fix/contrib.z"
zrec "$CR_C2" "Alex Takitani" aftakitani@gmail.com "Alex Takitani" aftakitani@gmail.com "$CR_C1" "$CR_C2B" >> "$root/fix/contrib.z"
zrec "$CR_C1" "Alex Takitani" aftakitani@gmail.com "Alex Takitani" aftakitani@gmail.com "$CR_B" "$CR_C1B" >> "$root/fix/contrib.z"
zrec "$CR_B" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$GOOD_B" >> "$root/fix/contrib.z"
"$repo/tools/flea-commit-rules" "$root/fix/contrib.z" "$root/out" > "$root/fix/contrib.out" 2>&1
check "commit-rules accepts merged contributor history" 0 $?
expect_grep "commit-rules summary is PASS" "$root/fix/contrib.out" "commit-rules: PASS fails=0 commits=10"
expect_grep "commit-rules waives contributor history" "$root/out/commits-findings.tsv" "accepted contributor history"

# An AI-attributed contributor commit still fails.
AT_B=$(python3 -c 'print("c" * 40)')
AT_G1=$(python3 -c 'print("d" * 40)')
AT_C1=$(python3 -c 'print("e" * 40)')
AT_M=$(python3 -c 'print("f" * 40)')
AT_T=$(python3 -c 'print("0" * 40)')
AT_C1B=$'feat: contributor work\n\nGenerated with Claude.\nSecond body line.'
AT_MB=$'Merge side work into the release\n\nFirst body line.\nSecond body line.'
zrec "$AT_T" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "$AT_M" "$GOOD_B" > "$root/fix/contrib-attr.z"
zrec "$AT_M" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "$AT_G1 $AT_C1" "$AT_MB" >> "$root/fix/contrib-attr.z"
zrec "$AT_G1" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "$AT_B" "$GOOD_B" >> "$root/fix/contrib-attr.z"
zrec "$AT_C1" "Alex Takitani" aftakitani@gmail.com "Alex Takitani" aftakitani@gmail.com "$AT_G1" "$AT_C1B" >> "$root/fix/contrib-attr.z"
zrec "$AT_B" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$GOOD_B" >> "$root/fix/contrib-attr.z"
"$repo/tools/flea-commit-rules" "$root/fix/contrib-attr.z" "$root/out" > "$root/fix/contrib-attr.out" 2>&1
check "commit-rules rejects an AI-attributed contributor" 1 $?
expect_grep "commit-rules names the contributor attribution" "$root/fix/contrib-attr.out" "attribution 'claude'"

# A non-GM commit on the first-parent chain still fails identity, even when a GM merge reaches it.
CH_B=$(python3 -c 'print("1" * 39 + "2")')
CH_G1=$(python3 -c 'print("1" * 39 + "4")')
CH_M=$(python3 -c 'print("1" * 39 + "5")')
CH_T=$(python3 -c 'print("1" * 39 + "3")')
CH_MB=$'Merge side work into the release\n\nFirst body line.\nSecond body line.'
zrec "$CH_T" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "$CH_M" "$GOOD_B" > "$root/fix/chain.z"
zrec "$CH_M" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "$CH_G1 $CH_B" "$CH_MB" >> "$root/fix/chain.z"
zrec "$CH_G1" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "$CH_B" "$GOOD_B" >> "$root/fix/chain.z"
zrec "$CH_B" Someone x@y.z GM gianmarcomorales@icloud.com "" "$GOOD_B" >> "$root/fix/chain.z"
"$repo/tools/flea-commit-rules" "$root/fix/chain.z" "$root/out" > "$root/fix/chain.out" 2>&1
check "commit-rules rejects a non-GM commit on the chain" 1 $?
expect_grep "commit-rules names the chain identity" "$root/fix/chain.out" "identity an=Someone <x@y.z>"

# A GM merge with an 81-character subject fails.
MG_B=$(python3 -c 'print("2" * 39 + "0")')
MG_G1=$(python3 -c 'print("2" * 39 + "1")')
MG_G2=$(python3 -c 'print("2" * 39 + "2")')
MG_M=$(python3 -c 'print("2" * 39 + "3")')
MG_81=$(python3 -c 'print("Merge " + "x" * 75)')
MG_81B=$(printf '%s\n\nFirst body line.\nSecond body line.' "$MG_81")
zrec "$MG_M" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "$MG_G1 $MG_G2" "$MG_81B" > "$root/fix/merge81.z"
zrec "$MG_G1" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "$MG_B" "$GOOD_B" >> "$root/fix/merge81.z"
zrec "$MG_G2" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "$MG_B" "$GOOD_B" >> "$root/fix/merge81.z"
zrec "$MG_B" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$GOOD_B" >> "$root/fix/merge81.z"
"$repo/tools/flea-commit-rules" "$root/fix/merge81.z" "$root/out" > "$root/fix/merge81.out" 2>&1
check "commit-rules rejects a GM merge past eighty" 1 $?
expect_grep "commit-rules counts the merge subject" "$root/fix/merge81.out" "want at most 80"

# A GM non-merge on a side branch keeps the sixty-character rule.
SB_B=$(python3 -c 'print("3" * 39 + "0")')
SB_G1=$(python3 -c 'print("3" * 39 + "1")')
SB_S=$(python3 -c 'print("3" * 39 + "2")')
SB_M=$(python3 -c 'print("3" * 39 + "3")')
SB_T=$(python3 -c 'print("3" * 39 + "4")')
SB_61=$(python3 -c 'print("fix(hooks): " + "x" * 49)')
SB_61B=$(printf '%s\n\nFirst body line.\nSecond body line.' "$SB_61")
SB_MB=$'Merge side work into the release\n\nFirst body line.\nSecond body line.'
zrec "$SB_T" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "$SB_M" "$GOOD_B" > "$root/fix/side61.z"
zrec "$SB_M" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "$SB_G1 $SB_S" "$SB_MB" >> "$root/fix/side61.z"
zrec "$SB_G1" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "$SB_B" "$GOOD_B" >> "$root/fix/side61.z"
zrec "$SB_S" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "$SB_B" "$SB_61B" >> "$root/fix/side61.z"
zrec "$SB_B" GM gianmarcomorales@icloud.com GM gianmarcomorales@icloud.com "" "$GOOD_B" >> "$root/fix/side61.z"
"$repo/tools/flea-commit-rules" "$root/fix/side61.z" "$root/out" > "$root/fix/side61.out" 2>&1
check "commit-rules rejects a long GM subject on a side branch" 1 $?
expect_grep "commit-rules counts the side subject" "$root/fix/side61.out" "want at most 60"

# The commit-msg adapter reads the invoking repository's merge state, so its cases run in a plain fixture.
mkdir -p "$root/plain" && (cd "$root/plain" && git init -q -b main . && git config user.name GM && git config user.email gianmarcomorales@icloud.com)
msg_adapter() { (cd "$root/plain" && "$repo/tools/flea-commit-msg" "$@"); }
printf 'fix(hooks): judge the proposed message\n\nFirst body line.\nSecond body line.\n# a git comment line\n' > "$root/fix/msg.txt"
msg_adapter "$root/fix/msg.txt" > "$root/fix/msg.out" 2>&1
check "commit-msg accepts a good message" 0 $?
expect_grep "commit-msg summary is PASS" "$root/fix/msg.out" "commit-msg: PASS fails=0"

printf 'fix(hooks): judge the proposed message\n\nFirst body line.\nSecond body line.\n' > "$root/fix/odd; mesg.txt"
msg_adapter "$root/fix/odd; mesg.txt" > "$root/fix/odd.out" 2>&1
check "commit-msg takes a spaced name literally" 0 $?

# A metacharacter name must reach the adapter unchanged and execute nothing.
printf 'fix(hooks): judge the proposed message\n\nFirst body line.\nSecond body line.\n' > "$root/fix/we ird;\$(touch MARKER)\`x\`.txt"
msg_adapter "$root/fix/we ird;\$(touch MARKER)\`x\`.txt" > "$root/fix/meta.out" 2>&1
check "commit-msg takes a metacharacter name literally" 0 $?
[ -e "$root/plain/MARKER" ] && { echo "FAIL commit-msg executed a metacharacter name"; fail=1; } || echo "ok   commit-msg executed nothing"

mkdir -p "$root/norm" && (cd "$root/norm" && git init -q -b main . && git config user.name GM && git config user.email gianmarcomorales@icloud.com)
agree() {
  (cd "$root/norm" && git log -1 -z --format='%H%x1f%an%x1f%ae%x1f%cn%x1f%ce%x1f%P%x1f%B' > "$root/fix/agree.z")
  "$repo/tools/flea-commit-rules" "$root/fix/agree.z" "$root/out" > "$root/fix/agree.out" 2>&1
  echo $?
}
# Leading and trailing comments normalize away before judging or storing.
(cd "$root/norm" && printf '# template\n\nfix: x\n\nBody one.\nBody two.\n# trail\n' > lead.txt && "$repo/tools/flea-commit-msg" lead.txt > "$root/fix/lead.out" 2>&1)
check "commit-msg accepts comments around canonical prose" 0 $?
if grep -q "^#" "$root/norm/lead.txt"; then echo "FAIL commit-msg left edge comments behind"; fail=1; else echo "ok   commit-msg left no edge comments"; fi
(cd "$root/norm" && echo lead >> fmatrix.txt && git add fmatrix.txt && git commit -qF lead.txt && [ "$(agree)" = 0 ]) && echo "ok   stored edge-trimmed bytes agree" || { echo "FAIL stored edge-trimmed bytes disagree"; fail=1; }
(cd "$root/norm" && printf 'fix(hooks): comment char override\n\nFirst body line.\nSecond body line.\n# Generated with Claude\n' > msg.txt && "$repo/tools/flea-commit-msg" msg.txt > "$root/fix/n0.out" 2>&1)
check "commit-msg strips hash comments by default" 0 $?
(cd "$root/norm" && git config core.commentChar ";" && printf 'fix(hooks): comment char override\n\nFirst body line.\nSecond body line.\n# Generated with Claude\n' > msg-n1.txt && cp msg-n1.txt "$root/fix/n1-before.txt" && "$repo/tools/flea-commit-msg" msg-n1.txt > "$root/fix/n1.out" 2>&1)
check "commit-msg keeps a hash line under semicolon comments" 1 $?
expect_grep "commit-msg judges committed bytes" "$root/fix/n1.out" "attribution 'claude'"
if cmp -s "$root/norm/msg-n1.txt" "$root/fix/n1-before.txt"; then echo "ok   commit-msg leaves a refused file untouched"; else echo "FAIL commit-msg rewrote a refused file"; fail=1; fi
# The repair: a passed message is written back, so default Git-F stores exactly what was judged.
(cd "$root/norm" && git config --unset core.commentChar && printf 'fix(hooks): stored agreement\n\nFirst body line.\nSecond body line.\n# Please enter the commit message.\n' > agree-msg.txt && "$repo/tools/flea-commit-msg" agree-msg.txt > "$root/fix/n0b.out" 2>&1)
check "commit-msg passes a commented good message" 0 $?
if grep -q "^#" "$root/norm/agree-msg.txt"; then echo "FAIL commit-msg left the comment in the file"; fail=1; else echo "ok   commit-msg wrote validated bytes back"; fi
(cd "$root/norm" && echo z > f2.txt && git add f2.txt && git commit -qF agree-msg.txt && [ "$(agree)" = 0 ]) && echo "ok   stored message agrees with the adapter" || { echo "FAIL stored message disagrees with the adapter"; fail=1; }
(cd "$root/norm" && printf 'fix(hooks): trailer writeback\n\nFirst body line.\nSecond body line.\n# a comment\n\nCo-authored-by: Ada Lovelace <ada@example.com>\n' > trail-msg.txt && "$repo/tools/flea-commit-msg" trail-msg.txt > "$root/fix/n0c.out" 2>&1)
check "commit-msg passes comments beside a human trailer" 0 $?
expect_grep "commit-msg keeps the trailer" "$root/norm/trail-msg.txt" "Co-authored-by: Ada Lovelace <ada@example.com>"
(cd "$root/norm" && echo w > f3.txt && git add f3.txt && git commit -qF trail-msg.txt && [ "$(agree)" = 0 ]) && echo "ok   stored trailer agrees with the adapter" || { echo "FAIL stored trailer disagrees with the adapter"; fail=1; }
(cd "$root/norm" && echo x > f.txt && git add f.txt && git commit -qF msg-n1.txt) > /dev/null 2>&1 || true
[ "$(agree)" = 1 ] && echo "ok   range checker agrees on the committed bytes" || { echo "FAIL range checker disagrees with the adapter"; fail=1; }
(cd "$root/norm" && git config core.commentChar ";" && printf 'fix(hooks): semicolon note\n\nFirst body line.\nSecond body line.\n; internal note\n' > msg2.txt && "$repo/tools/flea-commit-msg" msg2.txt > "$root/fix/n2.out" 2>&1)
check "commit-msg strips the configured comment char" 0 $?
if grep -q "^; internal note" "$root/norm/msg2.txt"; then echo "FAIL semicolon note survived the write-back"; fail=1; else echo "ok   semicolon note left the file"; fi
(cd "$root/norm" && git add f.txt 2>/dev/null; echo y >> f.txt && git add f.txt && git commit -qF msg2.txt && [ "$(agree)" = 0 ]) && echo "ok   range checker agrees on the stripped note" || { echo "FAIL range checker disagrees on the stripped note"; fail=1; }
if grep -q "; internal note" "$root/fix/agree.z"; then echo "FAIL stored bytes kept the semicolon note"; fail=1; else echo "ok   stored bytes lack the semicolon note"; fi
# commentString wins over commentChar, including a multi-character prefix git honors.
(cd "$root/norm" && git config --unset core.commentChar 2>/dev/null || true; git config core.commentString ";" && printf 'fix(hooks): string second line\n\nFirst body line.\n;Second body line.\n' > cs1.txt && "$repo/tools/flea-commit-msg" cs1.txt > "$root/fix/cs1.out" 2>&1)
check "commentString refuses what commentChar would keep" 1 $?
(cd "$root/norm" && echo cs1 >> fmatrix.txt && git add fmatrix.txt && git commit -q --cleanup=strip -F cs1.txt && [ "$(agree)" = 1 ]) && echo "ok   stored bytes agree on the refusal" || { echo "FAIL stored bytes disagree on the refusal"; fail=1; }
(cd "$root/norm" && printf 'fix(hooks): string note\n\nFirst body line.\nSecond body line.\n; a note\n' > cs2.txt && "$repo/tools/flea-commit-msg" cs2.txt > "$root/fix/cs2.out" 2>&1)
check "commentString strips its own notes" 0 $?
if grep -q "^; a note" "$root/norm/cs2.txt"; then echo "FAIL commentString note survived"; fail=1; else echo "ok   commentString note left the file"; fi
(cd "$root/norm" && echo cs2 >> fmatrix.txt && git add fmatrix.txt && git commit -qF cs2.txt && [ "$(agree)" = 0 ]) && echo "ok   stored string-stripped bytes agree" || { echo "FAIL stored string-stripped bytes disagree"; fail=1; }
(cd "$root/norm" && git config core.commentString "//" && printf 'fix(hooks): slash note\n\nFirst body line.\nSecond body line.\n// a note\n' > cs3.txt && "$repo/tools/flea-commit-msg" cs3.txt > "$root/fix/cs3.out" 2>&1)
check "commentString honors a multi-character prefix" 0 $?
if grep -q "^// a note" "$root/norm/cs3.txt"; then echo "FAIL slash note survived"; fail=1; else echo "ok   slash note left the file"; fi
(cd "$root/norm" && echo cs3 >> fmatrix.txt && git add fmatrix.txt && git commit -qF cs3.txt && [ "$(agree)" = 0 ]) && echo "ok   stored slash-stripped bytes agree" || { echo "FAIL stored slash-stripped bytes disagree"; fail=1; }
(cd "$root/norm" && git config core.commentChar "#" && git config core.commentString ";" && printf 'fix(hooks): precedence note\n\nFirst body line.\n# hash kept\n; semi gone\n' > cs4.txt && "$repo/tools/flea-commit-msg" cs4.txt > "$root/fix/cs4.out" 2>&1)
check "commentString outranks commentChar" 0 $?
if grep -q "^; semi gone" "$root/norm/cs4.txt" || ! grep -q "^# hash kept" "$root/norm/cs4.txt"; then echo "FAIL precedence wrote the wrong bytes"; fail=1; else echo "ok   precedence kept hash and dropped semi"; fi
(cd "$root/norm" && echo cs4 >> fmatrix.txt && git add fmatrix.txt && git commit -qF cs4.txt && [ "$(agree)" = 0 ]) && echo "ok   stored precedence bytes agree" || { echo "FAIL stored precedence bytes disagree"; fail=1; }
(cd "$root/norm" && git config --unset core.commentString && git config --unset core.commentChar 2>/dev/null || true)
(cd "$root/norm" && git config commit.cleanup default && printf 'fix(hooks): default mode note\n\nFirst body line.\nSecond body line.\n' > msg-def.txt && "$repo/tools/flea-commit-msg" msg-def.txt > "$root/fix/ndef.out" 2>&1)
check "commit-msg accepts cleanup default" 0 $?
(cd "$root/norm" && git config commit.cleanup verbatim && printf 'fix(hooks): verbatim note\n\nFirst body line.\nSecond body line.\n# kept note\n' > msg3.txt && "$repo/tools/flea-commit-msg" msg3.txt > "$root/fix/n3.out" 2>&1)
check "commit-msg canonicalizes verbatim bytes too" 0 $?
if grep -q "^#" "$root/norm/msg3.txt"; then echo "FAIL commit-msg left the comment for verbatim"; fail=1; else echo "ok   commit-msg wrote canonical bytes back"; fi
(cd "$root/norm" && echo v3 > f3.txt && git add f3.txt && git commit -q --cleanup=verbatim -F msg3.txt && [ "$(agree)" = 0 ]) && echo "ok   stored verbatim bytes agree" || { echo "FAIL stored verbatim bytes disagree"; fail=1; }
(cd "$root/norm" && git config commit.cleanup whitespace && printf 'fix(hooks): whitespace note\n\nFirst body line.\nSecond body line.\n# kept line\n' > msg4.txt && "$repo/tools/flea-commit-msg" msg4.txt > "$root/fix/n4w.out" 2>&1)
check "commit-msg keeps hash lines under whitespace cleanup" 0 $?
(cd "$root/norm" && echo v > f4.txt && git add f4.txt && git commit -qF msg4.txt && [ "$(agree)" = 0 ]) && echo "ok   stored whitespace bytes agree" || { echo "FAIL stored whitespace bytes disagree"; fail=1; }
# The root residual: repo verbatim, two body lines with a hash second, CLI strip.
for repo_mode in verbatim whitespace default; do
  (cd "$root/norm" && git config --unset commit.cleanup 2>/dev/null || true
   [ "$repo_mode" = default ] || git config commit.cleanup "$repo_mode"
   printf 'fix(hooks): root residual\n\nFirst body line.\n# second is a comment\n' > root-ex.txt
   cp root-ex.txt "$root/fix/root-before.txt"
   "$repo/tools/flea-commit-msg" root-ex.txt > "$root/fix/root-$repo_mode.out" 2>&1)
  check "commit-msg refuses the root residual under repo $repo_mode" 1 $?
done
if cmp -s "$root/norm/root-ex.txt" "$root/fix/root-before.txt"; then echo "ok   commit-msg leaves the refused residual untouched"; else echo "FAIL commit-msg rewrote a refused residual"; fail=1; fi
for cli_mode in strip scissors verbatim default; do
  (cd "$root/norm" && git config --unset commit.cleanup 2>/dev/null || true
   printf 'fix(hooks): matrix valid\n\nFirst body line.\nSecond body line.\n# a comment\n' > matrix.txt
   "$repo/tools/flea-commit-msg" matrix.txt > "$root/fix/matrix-$cli_mode.out" 2>&1)
  check "commit-msg passes the matrix message for CLI $cli_mode" 0 $?
  (cd "$root/norm" && echo "$cli_mode" >> fmatrix.txt && git add fmatrix.txt && if [ "$cli_mode" = default ]; then git commit -qF matrix.txt; else git commit -q --cleanup="$cli_mode" -F matrix.txt; fi && [ "$(agree)" = 0 ]) && echo "ok   CLI $cli_mode stores agreed bytes" || { echo "FAIL CLI $cli_mode stored disagreed bytes"; fail=1; }
done
(cd "$root/norm" && git config --unset commit.cleanup 2>/dev/null || true; git config core.commentChar auto && "$repo/tools/flea-commit-msg" msg.txt > "$root/fix/n4.out" 2>&1)
check "commit-msg refuses an auto comment char" 2 $?
expect_grep "commit-msg will not guess" "$root/fix/n4.out" "cannot be applied safely"
(cd "$root/norm" && git config --unset core.commentChar) || true

printf 'wip\n' > "$root/fix/wip.txt"
msg_adapter "$root/fix/wip.txt" > "$root/fix/wip.out" 2>&1
check "commit-msg rejects a bad message" 1 $?
expect_grep "commit-msg names the subject" "$root/fix/wip.out" "FAIL proposed subject not conventional: wip"

printf 'fix(hooks): judge the proposed message\n\nFirst body line.\nSecond body line.\n\nCo-authored-by: Ada Lovelace <ada@example.com>\n' > "$root/fix/credit-msg.txt"
msg_adapter "$root/fix/credit-msg.txt" > "$root/fix/credit-msg.out" 2>&1
check "commit-msg accepts a human credit trailer" 0 $?
printf 'fix(hooks): judge the proposed message\n\nFirst body line.\nSecond body line.\n\nCo-authored-by: \u674e\u660e <li@example.com>\n' > "$root/fix/liming-msg.txt"
msg_adapter "$root/fix/liming-msg.txt" > "$root/fix/liming-msg.out" 2>&1
check "commit-msg accepts a non-Latin human credit" 0 $?
printf 'fix(hooks): judge the proposed message\n\nFirst body line.\nSecond body line.\n\nCo-authored-by: ChatGPT_4o <noreply@openai.com>\n' > "$root/fix/uscoreai-msg.txt"
msg_adapter "$root/fix/uscoreai-msg.txt" > "$root/fix/uscoreai-msg.out" 2>&1
check "commit-msg rejects an underscore-qualified AI credit" 1 $?
printf 'fix(hooks): judge the proposed message\n\nFirst body line.\nSecond body line.\nCo-authored-by: ada\n' > "$root/fix/badcredit-msg.txt"
msg_adapter "$root/fix/badcredit-msg.txt" > "$root/fix/badcredit-msg.out" 2>&1
check "commit-msg rejects a malformed credit trailer" 1 $?
printf 'fix(hooks): judge the proposed message\n\nFirst body line.\nSecond body line.\nCo-authored-by: Claude <noreply@anthropic.com>\n' > "$root/fix/aicredit-msg.txt"
msg_adapter "$root/fix/aicredit-msg.txt" > "$root/fix/aicredit-msg.out" 2>&1
check "commit-msg rejects an AI credit trailer" 1 $?

msg_adapter "$root/fix/no-such-file.txt" > "$root/fix/missing.out" 2>&1
check "commit-msg refuses a missing file" 2 $?
expect_grep "commit-msg says ERROR" "$root/fix/missing.out" "commit-msg: ERROR"

# A merge held open by --no-commit: the adapter judges the message as that merge, with or without hk.
mkdir -p "$root/mid" && (cd "$root/mid" && git init -q -b main . && git config user.name GM && git config user.email gianmarcomorales@icloud.com && echo a > a.txt && git add a.txt && git commit -qm base && git checkout -qb side && echo b > b.txt && git add b.txt && git commit -qm side && git checkout -q main && git merge -q --no-ff --no-commit side) > "$root/fix/mid.out" 2>&1
check "an open merge fixture holds MERGE_HEAD" 0 $?
printf 'Merge side: judged while the merge is open\n\nFirst body line.\nSecond body line.\n' > "$root/fix/mid-msg.txt"
(cd "$root/mid" && "$repo/tools/flea-commit-msg" "$root/fix/mid-msg.txt") > "$root/fix/mid-msg.out" 2>&1
check "commit-msg accepts a GM merge while MERGE_HEAD is set" 0 $?
(cd "$root/mid" && "$repo/tools/flea-commit-msg" "$root/fix/msg.txt") > "$root/fix/mid-plain.out" 2>&1
check "commit-msg judges a conventional subject as a merge while MERGE_HEAD is set" 1 $?
expect_grep "commit-msg names the merge subject rule" "$root/fix/mid-plain.out" "FAIL proposed merge subject must start with 'Merge ': fix(hooks): judge the proposed message"

# A merge of two branches holds two MERGE_HEAD lines; the adapter judges every parent.
mkdir -p "$root/oct" && (cd "$root/oct" && git init -q -b main . && git config user.name GM && git config user.email gianmarcomorales@icloud.com && echo a > a.txt && git add a.txt && git commit -qm base && git checkout -qb s1 && echo b > b.txt && git add b.txt && git commit -qm s1 && git checkout -q main && git checkout -qb s2 && echo c > c.txt && git add c.txt && git commit -qm s2 && git checkout -q main && git merge -q --no-ff --no-commit s1 s2) > "$root/fix/oct.out" 2>&1
check "a two-branch merge fixture holds MERGE_HEAD" 0 $?
[ "$(wc -l < "$root/oct/.git/MERGE_HEAD")" = 2 ] && echo "ok   MERGE_HEAD holds two parents" || { echo "FAIL MERGE_HEAD holds $(wc -l < "$root/oct/.git/MERGE_HEAD") line(s), want 2"; fail=1; }
printf 'Merge sides: judged while the octopus merge is open\n\nFirst body line.\nSecond body line.\n' > "$root/fix/oct-msg.txt"
(cd "$root/oct" && "$repo/tools/flea-commit-msg" "$root/fix/oct-msg.txt") > "$root/fix/oct-msg.out" 2>&1
check "commit-msg accepts a GM merge of two branches" 0 $?
(cd "$root/oct" && "$repo/tools/flea-commit-msg" "$root/fix/msg.txt") > "$root/fix/oct-plain.out" 2>&1
check "commit-msg judges a conventional subject as a two-branch merge" 1 $?
expect_grep "commit-msg names the two-branch merge rule" "$root/fix/oct-plain.out" "FAIL proposed merge subject must start with 'Merge ': fix(hooks): judge the proposed message"

# A bad second MERGE_HEAD line: only reading every line refuses, naming the line.
OCT_FIRST=$(head -n 1 "$root/oct/.git/MERGE_HEAD")
printf '%s\nnot-a-40-hex-parent\n' "$OCT_FIRST" > "$root/oct/.git/MERGE_HEAD"
(cd "$root/oct" && "$repo/tools/flea-commit-msg" "$root/fix/oct-msg.txt") > "$root/fix/oct-bad2.out" 2>&1
check "commit-msg refuses a non-hex second MERGE_HEAD line" 2 $?
expect_grep "commit-msg names the bad line" "$root/fix/oct-bad2.out" "not-a-40-hex-parent"
# A 40-hex MERGE_HEAD line naming no object: only verifying every line as a commit refuses.
printf '%s\n0000000000000000000000000000000000000000\n' "$OCT_FIRST" > "$root/oct/.git/MERGE_HEAD"
(cd "$root/oct" && "$repo/tools/flea-commit-msg" "$root/fix/oct-msg.txt") > "$root/fix/oct-null.out" 2>&1
check "commit-msg refuses a MERGE_HEAD line naming no commit" 2 $?
expect_grep "commit-msg names the missing line" "$root/fix/oct-null.out" "0000000000000000000000000000000000000000"
# git itself failing on a valid line (exit 128) is named as git's failure, not as a missing commit.
OCT_SECOND=$(git -C "$root/oct" rev-parse HEAD)
printf '%s\n%s\n' "$OCT_FIRST" "$OCT_SECOND" > "$root/oct/.git/MERGE_HEAD"
mkdir -p "$root/fakegit" && real_git=$(command -v git)
printf '#!/bin/sh\nif [ "$1 $2 $3" = "rev-parse -q --verify" ]; then echo "fatal: bad object store" >&2; exit 128; fi\nexec "%s" "$@"\n' "$real_git" > "$root/fakegit/git" && chmod +x "$root/fakegit/git"
(cd "$root/oct" && PATH="$root/fakegit:$PATH" "$repo/tools/flea-commit-msg" "$root/fix/oct-msg.txt") > "$root/fix/oct-git128.out" 2>&1
check "commit-msg refuses when git rev-parse fails on a valid line" 2 $?
expect_grep "commit-msg names git's exit status" "$root/fix/oct-git128.out" "exited 128: fatal: bad object store"

# The partial-index guard in a real tiny git fixture. Covered inputs live under the
# same shapes the guard watches (src/, AGENTS.md); notes.txt matches no pattern.
mkdir -p "$root/guard/src" && (cd "$root/guard" && git init -q -b main . && git config user.name GM && git config user.email gianmarcomorales@icloud.com)
(cd "$root/guard" && echo 'fn main(){}' > src/a.rs && echo x > notes.txt && echo doc > AGENTS.md && git add src/a.rs notes.txt AGENTS.md && git commit -qm 'test(hooks): seed the guard fixture

First body line.
Second body line.')
(cd "$root/guard" && "$repo/tools/flea-hook-check" guard) > "$root/fix/g0.out" 2>&1
check "guard passes a clean tree" 0 $?
(cd "$root/guard" && echo '// staged' >> src/a.rs && git add src/a.rs && "$repo/tools/flea-hook-check" guard) > "$root/fix/g1.out" 2>&1
check "guard passes staged-only changes" 0 $?
(cd "$root/guard" && echo '// unstaged' >> src/a.rs && "$repo/tools/flea-hook-check" guard) > "$root/fix/g2.out" 2>&1
check "guard refuses staged bytes masked by unstaged ones" 1 $?
expect_grep "guard explains in one sentence" "$root/fix/g2.out" "would judge working-tree bytes rather than the staged commit"
(cd "$root/guard" && git reset -q && git checkout -q -- src/a.rs && echo y >> notes.txt && git add notes.txt && echo z >> notes.txt && "$repo/tools/flea-hook-check" guard) > "$root/fix/g3.out" 2>&1
check "guard passes when only irrelevant names are dirty" 0 $?
(cd "$root/guard" && git reset -q && git checkout -q -- notes.txt && echo doc2 >> AGENTS.md && echo y >> notes.txt && git add notes.txt && "$repo/tools/flea-hook-check" guard) > "$root/fix/g4.out" 2>&1
check "guard refuses a masked AGENTS.md" 1 $?
(cd "$root/guard" && git reset -q && git checkout -q -- AGENTS.md notes.txt && echo y >> notes.txt && git add notes.txt && touch "src/we
ird.rs" && "$repo/tools/flea-hook-check" guard) > "$root/fix/g5.out" 2>&1
check "guard refuses an unstaged newline path" 1 $?
expect_grep "guard explains the newline path" "$root/fix/g5.out" "would judge working-tree bytes rather than the staged commit"
(cd "$root/guard" && rm -f "src/we
ird.rs" && echo 'fn new(){}' > src/newmod.rs && "$repo/tools/flea-hook-check" guard) > "$root/fix/g6.out" 2>&1
check "guard refuses an untracked compiled module" 1 $?
expect_grep "guard explains the untracked module" "$root/fix/g6.out" "src/newmod.rs is not fully staged"
(cd "$root/guard" && GIT_DIR="$root/none" "$repo/tools/flea-hook-check" guard) > "$root/fix/g7.out" 2>&1
check "guard fails closed when git fails" 2 $?
expect_grep "guard says ERROR" "$root/fix/g7.out" "flea-hook-check: refused: git "
(cd "$root/guard" && rm -f src/newmod.rs && echo 'fn main(){}' > build.rs && git add build.rs notes.txt && echo '// unstaged' >> build.rs && "$repo/tools/flea-hook-check" guard) > "$root/fix/g8.out" 2>&1
check "guard refuses masked build configuration" 1 $?
expect_grep "guard names build.rs" "$root/fix/g8.out" "build.rs is not fully staged"
(cd "$root/guard" && rm -f build.rs && git reset -q && git checkout -q -- . && echo '// amend shape' >> src/a.rs && "$repo/tools/flea-hook-check" guard) > "$root/fix/g9.out" 2>&1
check "guard refuses with a clean index over a dirty tree" 1 $?
expect_grep "guard explains the amend shape" "$root/fix/g9.out" "would judge working-tree bytes rather than the staged commit"
(cd "$root/guard" && rm -f build.rs && git reset -q && git checkout -q -- . && printf 'src/ignored-mod.rs\n.cargo/config.toml\n' > .gitignore && echo y >> notes.txt && git add notes.txt && echo 'fn ignored(){}' > src/ignored-mod.rs && "$repo/tools/flea-hook-check" guard) > "$root/fix/g10.out" 2>&1
check "guard refuses an ignored src module" 1 $?
expect_grep "guard names the ignored module" "$root/fix/g10.out" "src/ignored-mod.rs is not fully staged"
(cd "$root/guard" && rm -f src/ignored-mod.rs && mkdir -p .cargo && echo '[lint]' > .cargo/config.toml && "$repo/tools/flea-hook-check" guard) > "$root/fix/g11.out" 2>&1
check "guard refuses an ignored cargo config" 1 $?
expect_grep "guard names the cargo config" "$root/fix/g11.out" ".cargo/config.toml is not fully staged"
(cd "$root/guard" && rm -rf .cargo && mkdir -p target .flea-local && echo x > target/x.rmeta && echo y > .flea-local/y && "$repo/tools/flea-hook-check" guard) > "$root/fix/g12.out" 2>&1
check "guard passes ignored runtime artifacts" 0 $?

# The installer in a real tiny git fixture.
mkdir -p "$root/inst" && (cd "$root/inst" && git init -q -b main .)
if command -v hk >/dev/null 2>&1 && [ "$(hk --version)" = "hk 2.4.0" ]; then
  (cd "$root/inst" && "$repo/tools/flea-hooks-install") > "$root/fix/i0.out" 2>&1
  check "installer plants both legacy shims" 0 $?
  [ -x "$root/inst/.git/hooks/pre-commit" ] && [ -x "$root/inst/.git/hooks/commit-msg" ] || { echo "FAIL installer left a shim missing or unexecutable"; fail=1; }
  echo "ok   installer leaves executable shims"
  (cd "$root/inst" && echo '# operator hook' > .git/hooks/pre-commit && "$repo/tools/flea-hooks-install") > "$root/fix/i1.out" 2>&1
  check "installer refuses an unrelated hook" 2 $?
  expect_grep "installer names the file" "$root/fix/i1.out" "is not a flea hook"
  if grep -q '# operator hook' "$root/inst/.git/hooks/pre-commit"; then echo "ok   installer left the operator hook byte-identical"; else echo "FAIL installer touched the operator hook"; fail=1; fi
else
  echo "SKIP installer live cases: no pinned hk 2.4.0 on PATH"
fi
# A wrong hk version refuses without writing, through a stub on PATH only.
mkdir -p "$root/verstub" && printf '#!/bin/bash\necho "hk 9.9.9"\n' > "$root/verstub/hk" && chmod +x "$root/verstub/hk"
mkdir -p "$root/ver" && (cd "$root/ver" && git init -q -b main .)
(cd "$root/ver" && PATH="$root/verstub:$PATH" "$repo/tools/flea-hooks-install") > "$root/fix/i2.out" 2>&1
check "installer refuses an unexpected hk version" 2 $?
expect_grep "installer names the versions" "$root/fix/i2.out" "want 'hk 2.4.0'"
[ -e "$root/ver/.git/hooks/pre-commit" ] && { echo "FAIL installer wrote a shim behind a refused version"; fail=1; } || echo "ok   installer wrote nothing behind a refused version"
# A configured hooks path points outside the repo; the installer refuses before writing anything.
mkdir -p "$root/pinstub" && printf '#!/bin/bash\necho "hk 2.4.0"\n' > "$root/pinstub/hk" && chmod +x "$root/pinstub/hk"
mkdir -p "$root/hcfg/home" "$root/hcfg/repo" && (cd "$root/hcfg/repo" && git init -q -b main .)
printf '[core]\n\thooksPath = /tmp/elsewhere-hooks\n' > "$root/hcfg/home/.gitconfig"
(cd "$root/hcfg/repo" && HOME="$root/hcfg/home" GIT_CONFIG_GLOBAL="$root/hcfg/home/.gitconfig" PATH="$root/pinstub:$PATH" "$repo/tools/flea-hooks-install") > "$root/fix/i3.out" 2>&1
check "installer refuses a configured hooks path" 2 $?
expect_grep "installer names the path" "$root/fix/i3.out" "core.hooksPath is set to '/tmp/elsewhere-hooks'"
[ -e "$root/hcfg/repo/.git/hooks/pre-commit" ] && { echo "FAIL installer wrote behind a foreign hooks path"; fail=1; } || echo "ok   installer wrote nothing behind a foreign hooks path"

# STUB clippy transport: a stub cargo replays a stream, a stub clippy-driver replays a help text.
mkdir -p "$root/stubbin"
cat > "$root/stubbin/cargo" <<'EOF'
#!/bin/bash
cat "$CLIPPY_STREAM"
exit "${CLIPPY_RC:-0}"
EOF
cat > "$root/stubbin/clippy-driver" <<'EOF'
#!/bin/bash
cat "$CLIPPY_HELP"
exit 0
EOF
chmod +x "$root/stubbin/cargo" "$root/stubbin/clippy-driver"
printf 'Lint groups loaded by this crate:\n   clippy::correctness  clippy::approx_constant, clippy::seed_lint\n   clippy::suspicious  clippy::seed_susp\n   clippy::style  clippy::bool_assert_comparison, clippy::seed_spanless\n' > "$root/stub-help.txt"
# The stub inventory is smaller than the real one, so stub runs judge against a stub baseline.
printf 'clippy::bool_assert_comparison|style|10\nclippy::seed_spanless|style|0\n' > "$root/stub-baseline.txt"
stub_clippy() {
  CLIPPY_STREAM="$1" CLIPPY_RC="${2:-0}" CLIPPY_HELP="$root/stub-help.txt" PATH="$root/stubbin:$PATH" \
    "$repo/tools/flea-clippy" "$root/stub-baseline.txt" "$3"
}
: > "$root/stub-empty.json"
stub_clippy "$root/stub-empty.json" 0 "$root/out/s-empty" > "$root/fix/s-empty.out" 2>&1
check "STUB clippy refuses a missing build-finished" 2 $?
printf 'garbage line\n' > "$root/stub-garbage.json"
stub_clippy "$root/stub-garbage.json" 0 "$root/out/s-garbage" > "$root/fix/s-garbage.out" 2>&1
check "STUB clippy refuses non-blank garbage JSON" 2 $?
expect_grep "STUB clippy names the garbage" "$root/fix/s-garbage.out" "were not valid cargo records"
printf '{"reason":"compiler-message"}\n{"reason":"build-finished","success":true}\n' > "$root/stub-noshape.json"
stub_clippy "$root/stub-noshape.json" 0 "$root/out/s-noshape" > "$root/fix/s-noshape.out" 2>&1
check "STUB clippy refuses a malformed record shape" 2 $?
printf '{"reason":"compiler-message","message":{"level":"warning","code":{"code":"clippy::no_such_lint"},"spans":[],"message":"x"}}\n{"reason":"compiler-artifact","target":{"name":"flea"}}\n{"reason":"build-finished","success":true}\n' > "$root/stub-unknown.json"
stub_clippy "$root/stub-unknown.json" 0 "$root/out/s-unknown" > "$root/fix/s-unknown.out" 2>&1
check "STUB clippy refuses an unknown lint" 2 $?
printf '{"reason":"nonsense","anything":"wrong"}\n{"reason":"compiler-artifact","target":{"name":"flea"}}\n{"reason":"build-finished","success":true}\n' > "$root/stub-reason.json"
stub_clippy "$root/stub-reason.json" 0 "$root/out/s-reason" > "$root/fix/s-reason.out" 2>&1
check "STUB clippy refuses an unknown reason" 2 $?
expect_grep "STUB clippy says ERROR" "$root/fix/s-reason.out" "clippy: ERROR"
printf '{"reason":"compiler-message","message":{"level":null}}\n{"reason":"compiler-artifact","target":{"name":"flea"}}\n{"reason":"build-finished","success":true}\n' > "$root/stub-level.json"
stub_clippy "$root/stub-level.json" 0 "$root/out/s-level" > "$root/fix/s-level.out" 2>&1
check "STUB clippy refuses a null level" 2 $?
expect_grep "STUB clippy says ERROR" "$root/fix/s-level.out" "clippy: ERROR"
printf '{"reason":"compiler-message","message":{"level":"nonsense","code":{"code":"clippy::seed_spanless"},"spans":[],"message":"m"}}\n{"reason":"compiler-artifact","target":{"name":"flea"}}\n{"reason":"build-finished","success":true}\n' > "$root/stub-level-s.json"
stub_clippy "$root/stub-level-s.json" 0 "$root/out/s-level-s" > "$root/fix/s-level-s.out" 2>&1
check "STUB clippy refuses an unknown level string" 2 $?
expect_grep "STUB clippy says ERROR" "$root/fix/s-level-s.out" "clippy: ERROR"
python3 - <<PY > "$root/stub-spans.json"
import json
base = {"level": "warning", "code": {"code": "clippy::seed_spanless"}, "message": "m"}
lines = [
    {"reason": "compiler-message", "message": dict(base, spans="x")},
    {"reason": "compiler-artifact", "target": {"name": "flea"}},
    {"reason": "build-finished", "success": True},
]
print("\n".join(json.dumps(l) for l in lines))
PY
stub_clippy "$root/stub-spans.json" 0 "$root/out/s-spans" > "$root/fix/s-spans.out" 2>&1
check "STUB clippy refuses non-list spans" 2 $?
python3 - <<PY > "$root/stub-span-el.json"
import json
lines = [
    {"reason": "compiler-message", "message": {"level": "warning", "code": {"code": "clippy::seed_spanless"},
      "spans": [{"is_primary": True}], "message": "m"}},
    {"reason": "compiler-artifact", "target": {"name": "flea"}},
    {"reason": "build-finished", "success": True},
]
print("\n".join(json.dumps(l) for l in lines))
PY
stub_clippy "$root/stub-span-el.json" 0 "$root/out/s-span-el" > "$root/fix/s-span-el.out" 2>&1
check "STUB clippy refuses a primary span without a location" 2 $?
printf '{"reason":"compiler-message","message":{"level":"warning","code":"clippy::seed_spanless","spans":[],"message":"m"}}\n{"reason":"compiler-artifact","target":{"name":"flea"}}\n{"reason":"build-finished","success":true}\n' > "$root/stub-code.json"
stub_clippy "$root/stub-code.json" 0 "$root/out/s-code" > "$root/fix/s-code.out" 2>&1
check "STUB clippy refuses a non-dict code" 2 $?
printf '{"reason":"compiler-message","message":{"level":"warning","code":{"code":7},"spans":[],"message":"m"}}\n{"reason":"compiler-artifact","target":{"name":"flea"}}\n{"reason":"build-finished","success":true}\n' > "$root/stub-code7.json"
stub_clippy "$root/stub-code7.json" 0 "$root/out/s-code7" > "$root/fix/s-code7.out" 2>&1
check "STUB clippy refuses a non-string code" 2 $?
printf '{"reason":"compiler-artifact","target":{"name":"flea"}}\n{"reason":"build-finished"}\n' > "$root/stub-success.json"
stub_clippy "$root/stub-success.json" 0 "$root/out/s-success" > "$root/fix/s-success.out" 2>&1
check "STUB clippy refuses a success-less build-finished" 2 $?
python3 - <<PY > "$root/stub-nospan.json"
import json
lines = []
for text in ("first span-less warning", "second span-less warning"):
    lines.append({"reason": "compiler-message", "message": {"level": "warning",
      "code": {"code": "clippy::seed_spanless"}, "spans": [], "message": text}})
lines.append({"reason": "compiler-artifact", "target": {"name": "flea"}})
lines.append({"reason": "build-finished", "success": True})
print("\n".join(json.dumps(l) for l in lines))
PY
stub_clippy "$root/stub-nospan.json" 0 "$root/out/s-nospan" > "$root/fix/s-nospan.out" 2>&1
check "STUB clippy keeps distinct span-less warnings apart" 1 $?
expect_grep "STUB clippy counts both findings" "$root/fix/s-nospan.out" "findings=2"
printf '{"reason":"compiler-artifact","target":{"name":"flea"}}\n{"reason":"build-finished","success":true}\n' > "$root/stub-clean.json"
stub_clippy "$root/stub-clean.json" 1 "$root/out/s-rc" > "$root/fix/s-rc.out" 2>&1
check "STUB clippy refuses a nonzero cargo exit" 2 $?
printf '{"reason":"compiler-artifact","target":{"name":"flea"}}\n' > "$root/stub-nofinish.json"
stub_clippy "$root/stub-nofinish.json" 0 "$root/out/s-nofinish" > "$root/fix/s-nofinish.out" 2>&1
check "STUB clippy refuses a checked target with no build-finished" 2 $?
(cd "$root/fix" && python3 -c 'open("bin-base.txt", "wb").write(b"clippy::len_zero|style|\xff\n")')
CLIPPY_STREAM="$root/stub-clean.json" CLIPPY_RC=0 CLIPPY_HELP="$root/stub-help.txt" PATH="$root/stubbin:$PATH" \
  "$repo/tools/flea-clippy" "$root/fix/bin-base.txt" "$root/out/s-binbase" > "$root/fix/s-binbase.out" 2>&1
check "STUB clippy refuses a non-UTF8 baseline" 2 $?
expect_grep "STUB clippy says UTF-8" "$root/fix/s-binbase.out" "not valid UTF-8"
mkdir -p "$root/nodriver" && cp "$root/stubbin/cargo" "$root/nodriver/cargo" && chmod +x "$root/nodriver/cargo"
CLIPPY_STREAM="$root/stub-clean.json" CLIPPY_RC=0 CLIPPY_HELP="$root/stub-help.txt" PATH="$root/nodriver:/usr/bin:/bin" \
  "$repo/tools/flea-clippy" "$repo/tools/clippy-baseline.txt" "$root/out/s-nodriver" > "$root/fix/s-nodriver.out" 2>&1
check "STUB clippy refuses a missing clippy-driver" 2 $?
expect_grep "STUB clippy names the driver" "$root/fix/s-nodriver.out" "clippy-driver did not start"
printf 'clippy::no_such_lint|style|1\n' > "$root/fix/odd-base.txt"
stub_clippy_base() {
  CLIPPY_STREAM="$1" CLIPPY_RC="${2:-0}" CLIPPY_HELP="$root/stub-help.txt" PATH="$root/stubbin:$PATH" \
    "$repo/tools/flea-clippy" "$root/fix/odd-base.txt" "$3"
}
stub_clippy_base "$root/stub-clean.json" 0 "$root/out/s-odd" > "$root/fix/s-odd.out" 2>&1
check "STUB clippy refuses a baseline lint outside the inventory" 2 $?
expect_grep "STUB clippy names the lint" "$root/fix/s-odd.out" "does not list"
# Orchestrated overlap: halves from both cargos must never share one OUTDIR stream.
python3 - <<PY > "$root/stub-deny.json"
import json
lines = []
lines.append({"reason": "compiler-message", "message": {"level": "warning", "code": {"code": "clippy::seed_lint"},
  "spans": [{"is_primary": True, "file_name": "src/a.rs", "line_start": 3, "column_start": 4}], "message": "seed"}})
lines.append({"reason": "compiler-artifact", "target": {"name": "flea"}})
lines.append({"reason": "build-finished", "success": True})
print("\n".join(json.dumps(l) for l in lines))
PY
cat > "$root/stubbin/cargo-half" <<'EOF'
#!/bin/bash
head -n "$CONC_LINES" "$CLIPPY_STREAM"
touch "$CONC_MINE"
for i in $(seq 1 300); do [ -e "$CONC_GO" ] && break; sleep 0.1; done
tail -n +"$((CONC_LINES + 1))" "$CLIPPY_STREAM"
EOF
chmod +x "$root/stubbin/cargo-half"
mkdir -p "$root/concbin" && cp "$root/stubbin/cargo-half" "$root/concbin/cargo" && cp "$root/stubbin/clippy-driver" "$root/concbin/clippy-driver" && chmod +x "$root/concbin/cargo" "$root/concbin/clippy-driver"
half_clippy() {
  CONC_MINE="$root/half-$1" CONC_LINES=1 CONC_GO="$root/half-go" CLIPPY_STREAM="$2" CLIPPY_HELP="$root/stub-help.txt" PATH="$root/concbin:$PATH" \
    "$repo/tools/flea-clippy" "$root/stub-baseline.txt" "$root/out/s-half" > "$root/fix/s-half-$1.out" 2>&1
  echo "$1:$?" > "$root/fix/half-$1.rc"
}
rm -f "$root/half-deny" "$root/half-clean" "$root/half-go"
half_clippy deny "$root/stub-deny.json" &
for i in $(seq 1 150); do [ -e "$root/half-deny" ] && break; sleep 0.1; done
[ -e "$root/half-deny" ] || { echo "FAIL STUB orchestrated run never started"; fail=1; }
half_clippy clean "$root/stub-clean.json" &
for i in $(seq 1 150); do [ -e "$root/half-clean" ] && break; sleep 0.1; done
if [ -e "$root/half-clean" ]; then echo "FAIL STUB lock did not serialize concurrent calls"; fail=1; fi
touch "$root/half-go"
wait
check "STUB serialized denied call exits 1" 1 "$(cut -d: -f2 < "$root/fix/half-deny.rc")"
check "STUB serialized clean call exits 0" 0 "$(cut -d: -f2 < "$root/fix/half-clean.rc")"
expect_grep "STUB serialized denied call stays denied" "$root/fix/s-half-deny.out" "FAIL deny clippy::seed_lint"
expect_grep "STUB serialized clean call stays clean" "$root/fix/s-half-clean.out" "clippy: PASS"
stub_clippy "$root/stub-deny.json" 0 "$root/out/s-deny" > "$root/fix/s-deny.out" 2>&1
check "STUB clippy denies a correctness seed" 1 $?
expect_grep "STUB clippy prints the denied finding" "$root/fix/s-deny.out" "FAIL deny clippy::seed_lint (correctness) src/a.rs:3:4"
python3 - <<PY > "$root/stub-rose.json"
import json
lines = []
for i in range(1, 13):
    lines.append({"reason": "compiler-message", "message": {"level": "warning",
      "code": {"code": "clippy::bool_assert_comparison"},
      "spans": [{"is_primary": True, "file_name": "src/b.rs", "line_start": i, "column_start": 1}], "message": "m"}})
lines.append({"reason": "compiler-artifact", "target": {"name": "flea"}})
lines.append({"reason": "build-finished", "success": True})
print("\n".join(json.dumps(l) for l in lines))
PY
stub_clippy "$root/stub-rose.json" 0 "$root/out/s-rose" > "$root/fix/s-rose.out" 2>&1
check "STUB clippy fails a risen ratchet" 1 $?
expect_grep "STUB clippy prints every risen location" "$root/fix/s-rose.out" "src/b.rs:12:1"
if grep -q "and .* more" "$root/fix/s-rose.out"; then echo "FAIL STUB clippy capped a FAIL line"; fail=1; else echo "ok   STUB clippy caps no FAIL line"; fi
stub_clippy "$root/stub-clean.json" 0 "$root/out/s-pass" > "$root/fix/s-pass.out" 2>&1
check "STUB clippy passes a clean stream" 0 $?
expect_grep "STUB clippy summary is PASS" "$root/fix/s-pass.out" "clippy: PASS"

# REAL hk and git hooks where the pinned hk exists.
if command -v hk >/dev/null 2>&1 && [ "$(hk --version)" = "hk 2.4.0" ]; then
  (cd "$repo" && hk validate) > "$root/fix/hkv.out" 2>&1
  check "REAL hk validates the shipped hk.pkl" 0 $?
  mkdir -p "$root/hook" && (cd "$root/hook" && git init -q -b main . && git config user.name GM && git config user.email gianmarcomorales@icloud.com)
  # The legacy shim resolves tools/ off the fixture top, so the fixture links the tree under test.
  ln -s "$repo/tools" "$root/hook/tools"
  (cd "$root/hook" && cat > hk.pkl <<EOF
amends "package://github.com/jdx/hk/releases/download/v2.4.0/hk@2.4.0#/Config.pkl"
min_hk_version = "2.4.0"
hooks {
  ["pre-commit"] {
    fix = false
    stage = false
    stash = "none"
    steps {
      ["partial-index-guard"] {
        check = "$repo/tools/flea-hook-check guard"
      }
      ["clippy-spy"] {
        depends = "partial-index-guard"
        check = "touch clippy-spy-ran"
      }
    }
  }
  ["commit-msg"] {
    steps {
      ["commit-rules"] {
        check = new Command {
          argv = List("$repo/tools/flea-commit-msg", "{{commit_msg_file}}")
        }
      }
    }
  }
}
EOF
  "$repo/tools/flea-hooks-install" >/dev/null 2>&1)
  cp "$root/hook/hk.pkl" "$root/fix/hk-guard.pkl"
  (cd "$root/hook" && mkdir -p src && echo 'fn main(){}' > src/a.rs && git add src/a.rs hk.pkl && printf 'test(hooks): land through the real hook\n\nFirst body line.\nSecond body line.\n' > msg.txt && git commit -qF msg.txt) > "$root/fix/h0.out" 2>&1
  check "REAL git lands a good commit through both hooks" 0 $?
  (cd "$root/hook" && echo x >> src/a.rs && git add src/a.rs && printf 'wip\n' > msg.txt && git commit -qF msg.txt) > "$root/fix/h1.out" 2>&1
  if [ $? -eq 0 ]; then echo "FAIL REAL git let a bad message through the hook"; fail=1; else echo "ok   REAL git blocks a bad message in the hook"; fi
  (cd "$root/hook" && git log --oneline | wc -l | grep -q '^1$') && echo "ok   REAL git kept one commit" || { echo "FAIL REAL git kept no single commit"; fail=1; }
  (cd "$root/hook" && echo '// unstaged' >> src/a.rs && git commit -qF msg.txt) > "$root/fix/h2.out" 2>&1
  if [ $? -eq 0 ]; then echo "FAIL REAL git let a masked commit through the guard"; fail=1; else echo "ok   REAL git refuses a masked commit in the guard"; fi
  expect_grep "REAL guard explains itself" "$root/fix/h2.out" "would judge working-tree bytes rather than the staged commit"
  (cd "$root/hook" && git reset -q && git checkout -q -- src/a.rs && echo y >> src/a.rs && git add src/a.rs && printf 'test(hooks): an odd path still lands\n\nFirst body line.\nSecond body line.\n' > 'odd msg;$(touch MARKER).txt' && git commit -qF 'odd msg;$(touch MARKER).txt') > "$root/fix/h3.out" 2>&1
  check "REAL git lands a commit filed under a metacharacter name" 0 $?
  [ -e "$root/hook/MARKER" ] && { echo "FAIL REAL hook executed a metacharacter name"; fail=1; } || echo "ok   REAL hook executed nothing"
  (cd "$root/hook" && printf 'test(hooks): hook argv stays literal\n\nFirst body line.\nSecond body line.\n' > 'argv;$(touch MARKER).txt' && hk run commit-msg 'argv;$(touch MARKER).txt') > "$root/fix/h3b.out" 2>&1
  check "REAL hk passes the metacharacter path literally" 0 $?
  [ -e "$root/hook/MARKER" ] && { echo "FAIL REAL hk interpolated hook argv"; fail=1; } || echo "ok   REAL hk interpolated nothing"
  # RED-DEMO, contained: a shell string parses after substitution, so unquoted interpolation executes.
  (cd "$root/hook" && rm -f MARKER && sh -c 'cmd="\"$0\" $1"; eval "$cmd"' "$repo/tools/flea-commit-msg" 'argv;$(touch MARKER).txt') > "$root/fix/reddemo.out" 2>&1 || true
  [ -e "$root/hook/MARKER" ] && echo "ok   RED-DEMO unquoted interpolation executes" || { echo "FAIL RED-DEMO hazard did not reproduce"; fail=1; }
  rm -f "$root/hook/MARKER"
  (cd "$root/hook" && echo '// worktree only' >> src/a.rs && git commit -q --amend --no-edit) > "$root/fix/h4.out" 2>&1
  if [ $? -eq 0 ]; then echo "FAIL REAL amend judged masked working-tree bytes"; fail=1; else echo "ok   REAL amend refuses before Clippy runs"; fi
  expect_grep "REAL amend explains itself" "$root/fix/h4.out" "would judge working-tree bytes rather than the staged commit"
  (cd "$root/hook" && rm -f clippy-spy-ran && git add src/a.rs && printf 'test(hooks): unmasked commit runs the spy\n\nFirst body line.\nSecond body line.\n' > msg.txt && git commit -qF msg.txt) > "$root/fix/h5.out" 2>&1
  check "REAL unmasked commit executes the dependent spy" 0 $?
  [ -e "$root/hook/clippy-spy-ran" ] && echo "ok   REAL spy ran behind the guard" || { echo "FAIL REAL spy never ran unmasked"; fail=1; }
  (cd "$root/hook" && rm -f clippy-spy-ran && echo '// masked' >> src/a.rs && git commit -q --amend --no-edit) > "$root/fix/h6.out" 2>&1
  if [ $? -eq 0 ]; then echo "FAIL REAL masked amend landed"; fail=1; else echo "ok   REAL masked amend refused"; fi
  [ -e "$root/hook/clippy-spy-ran" ] && { echo "FAIL REAL spy ran on masked bytes"; fail=1; } || echo "ok   REAL spy never ran masked"
  (cd "$root/hook" && cat > hk.pkl <<EOF
amends "package://github.com/jdx/hk/releases/download/v2.4.0/hk@2.4.0#/Config.pkl"
min_hk_version = "2.4.0"
hooks {
  ["pre-commit"] {
    fix = false
    stage = false
    stash = "none"
    steps {
      ["clippy-spy"] {
        check = "touch clippy-spy-ran"
      }
    }
  }
  ["commit-msg"] {
    steps {
      ["commit-rules"] {
        check = new Command {
          argv = List("$repo/tools/flea-commit-msg", "{{commit_msg_file}}")
        }
      }
    }
  }
}
EOF
  rm -f clippy-spy-ran && git commit -q --amend --no-edit) > "$root/fix/h7.out" 2>&1
  check "mutation control lands the amend without the guard" 0 $?
  [ -e "$root/hook/clippy-spy-ran" ] && echo "ok   mutation control proves the guard was load-bearing" || { echo "FAIL mutation control ran no spy"; fail=1; }
  # REAL merges: git runs commit-msg for a merge too, so the hook must judge it as the merge it records.
  mkdir -p "$root/mrg" && (cd "$root/mrg" && git init -q -b main . && git config user.name GM && git config user.email gianmarcomorales@icloud.com)
  ln -s "$repo/tools" "$root/mrg/tools"
  cp "$root/fix/hk-guard.pkl" "$root/mrg/hk.pkl"
  printf 'test(hooks): land the merge fixture base\n\nFirst body line.\nSecond body line.\n' > "$root/fix/m0.msg"
  (cd "$root/mrg" && "$repo/tools/flea-hooks-install" && git add hk.pkl && git commit -qF "$root/fix/m0.msg") > "$root/fix/m0.out" 2>&1
  check "REAL merge fixture lands its base through the hooks" 0 $?
  # Sample input (a branch name): "mgood"; one GM commit on a new branch off main, landed through the hooks.
  merge_side() {
    printf 'test(hooks): side work on %s\n\nFirst body line.\nSecond body line.\n' "$1" > "$root/fix/side-$1.msg"
    (cd "$root/mrg" && git checkout -qb "$1" main && echo "$1" > "$1.txt" && git add "$1.txt" && git commit -qF "$root/fix/side-$1.msg" && git checkout -q main) > "$root/fix/side-$1.out" 2>&1 || { echo "FAIL REAL side branch $1 did not land"; fail=1; }
  }
  head_parents() { (cd "$1" && git log -1 --format=%P | wc -w); }
  # A refused merge stays open; settle closes it so the next case starts from a clean head.
  settle() { (cd "$1" && if git rev-parse -q --verify MERGE_HEAD >/dev/null; then git merge --abort; fi); }
  # The hook's reasons for a refused merge equal the range checker's for the same bytes as a two-parent record.
  merge_agree() {
    zrec "$MG_M" "$3" "$4" GM gianmarcomorales@icloud.com "$MG_G1 $MG_G2" "$(cat "$5")" > "$root/fix/magree.z"
    "$repo/tools/flea-commit-rules" "$root/fix/magree.z" "$root/out" > "$root/fix/magree.out" 2>&1
    sed -n 's/^FAIL proposed //p' "$2" | sort > "$root/fix/magree.hook"
    sed -n 's/^FAIL [0-9a-f]\{12\} //p' "$root/fix/magree.out" | sort > "$root/fix/magree.range"
    if [ -s "$root/fix/magree.hook" ] && cmp -s "$root/fix/magree.hook" "$root/fix/magree.range"; then echo "ok   $1"; else echo "FAIL $1"; fail=1; fi
  }
  merge_side mgood
  (cd "$root/mrg" && git merge -q --no-ff mgood -m 'Merge mgood: record a merge through the hooks' -m $'First body line.\nSecond body line.') > "$root/fix/m1.out" 2>&1
  check "REAL git records a good GM merge through the hooks" 0 $?
  [ "$(head_parents "$root/mrg")" = 2 ] && echo "ok   REAL merge kept both parents" || { echo "FAIL REAL merge recorded no two-parent commit"; fail=1; }
  settle "$root/mrg"
  merge_side mlong
  printf '%s\n\nFirst body line.\nSecond body line.\n' "Merge mlong: $(printf '%068d' 0 | tr 0 x)" > "$root/fix/m2.msg"
  (cd "$root/mrg" && git merge -q --no-ff mlong -F "$root/fix/m2.msg") > "$root/fix/m2.out" 2>&1
  if [ $? -eq 0 ]; then echo "FAIL REAL git recorded a GM merge past eighty"; fail=1; else echo "ok   REAL git refuses a GM merge past eighty"; fi
  expect_grep "REAL hook counts the merge subject" "$root/fix/m2.out" "FAIL proposed subject holds 81 char(s), want at most 80"
  merge_agree "REAL hook refuses the long merge as commit-rules does" "$root/fix/m2.out" GM gianmarcomorales@icloud.com "$root/fix/m2.msg"
  settle "$root/mrg"
  merge_side mbody
  printf 'Merge mbody: a merge body past four lines\n\nOne.\nTwo.\nThree.\nFour.\nFive.\n' > "$root/fix/m3.msg"
  (cd "$root/mrg" && git merge -q --no-ff mbody -F "$root/fix/m3.msg") > "$root/fix/m3.out" 2>&1
  if [ $? -eq 0 ]; then echo "FAIL REAL git recorded a five-line merge body"; fail=1; else echo "ok   REAL git refuses a five-line merge body"; fi
  expect_grep "REAL hook counts the merge body" "$root/fix/m3.out" "FAIL proposed body holds 5 line(s), want 2 to 4"
  merge_agree "REAL hook refuses the merge body as commit-rules does" "$root/fix/m3.out" GM gianmarcomorales@icloud.com "$root/fix/m3.msg"
  settle "$root/mrg"
  printf 'test(hooks): clash on a side branch\n\nFirst body line.\nSecond body line.\n' > "$root/fix/m4t.msg"
  printf 'test(hooks): clash on main\n\nFirst body line.\nSecond body line.\n' > "$root/fix/m4o.msg"
  (cd "$root/mrg" && git checkout -qb mclash main && echo theirs > clash.txt && git add clash.txt && git commit -qF "$root/fix/m4t.msg" && git checkout -q main && echo ours > clash.txt && git add clash.txt && git commit -qF "$root/fix/m4o.msg") > "$root/fix/m4s.out" 2>&1
  check "REAL conflict fixture lands both sides" 0 $?
  (cd "$root/mrg" && git merge mclash) > "$root/fix/m4m.out" 2>&1
  check "REAL conflicting merge stops for a resolution" 1 $?
  printf 'wip\n' > "$root/fix/m4w.msg"
  (cd "$root/mrg" && printf 'ours\ntheirs\n' > clash.txt && git add clash.txt && git commit -qF "$root/fix/m4w.msg") > "$root/fix/m4w.out" 2>&1
  if [ $? -eq 0 ]; then echo "FAIL REAL git concluded a merge under a wip subject"; fail=1; else echo "ok   REAL git refuses a wip merge conclusion"; fi
  expect_grep "REAL hook judges the conclusion as a merge" "$root/fix/m4w.out" "FAIL proposed merge subject must start with 'Merge ': wip"
  printf 'Merge mclash: keep both sides of the clash\n\nResolved by keeping both lines.\nSecond body line.\n' > "$root/fix/m4.msg"
  (cd "$root/mrg" && git commit -qF "$root/fix/m4.msg") > "$root/fix/m4.out" 2>&1
  check "REAL git concludes a conflicted GM merge with git commit" 0 $?
  [ "$(head_parents "$root/mrg")" = 2 ] && echo "ok   REAL concluded merge kept both parents" || { echo "FAIL REAL concluded merge recorded no two-parent commit"; fail=1; }
  settle "$root/mrg"
  printf 'Merge nothing: a plain commit\n\nFirst body line.\nSecond body line.\n' > "$root/fix/m5.msg"
  (cd "$root/mrg" && echo plain >> clash.txt && git add clash.txt && git commit -qF "$root/fix/m5.msg") > "$root/fix/m5.out" 2>&1
  if [ $? -eq 0 ]; then echo "FAIL REAL git took a merge subject on a plain commit"; fail=1; else echo "ok   REAL git keeps a plain commit on the non-merge rules"; fi
  expect_grep "REAL hook names the plain subject" "$root/fix/m5.out" "FAIL proposed subject not conventional: Merge nothing: a plain commit"
  (cd "$root/mrg" && git checkout -q HEAD -- clash.txt)
  merge_side mada
  printf 'Merge mada: a merge under another name\n\nFirst body line.\nSecond body line.\n' > "$root/fix/m6.msg"
  (cd "$root/mrg" && GIT_AUTHOR_NAME=Ada GIT_AUTHOR_EMAIL=ada@example.com git merge -q --no-ff mada -F "$root/fix/m6.msg") > "$root/fix/m6.out" 2>&1
  if [ $? -eq 0 ]; then echo "FAIL REAL git recorded a non-GM merge"; fail=1; else echo "ok   REAL git refuses a non-GM merge"; fi
  expect_grep "REAL hook names the non-GM merge identity" "$root/fix/m6.out" "FAIL proposed identity an=Ada <ada@example.com>"
  merge_agree "REAL hook refuses a non-GM merge as commit-rules does" "$root/fix/m6.out" Ada ada@example.com "$root/fix/m6.msg"
  settle "$root/mrg"
  (cd "$root/mrg" && git worktree add -q "$root/mrgwt" -b mwt main) > "$root/fix/m7w.out" 2>&1
  check "REAL merge fixture adds a linked worktree" 0 $?
  ln -s "$repo/tools" "$root/mrgwt/tools"
  merge_side mwtside
  printf 'Merge mwtside: record a merge in a linked worktree\n\nFirst body line.\nSecond body line.\n' > "$root/fix/m7.msg"
  (cd "$root/mrgwt" && git merge -q --no-ff mwtside -F "$root/fix/m7.msg") > "$root/fix/m7.out" 2>&1
  check "REAL git records a GM merge in a linked worktree" 0 $?
  [ "$(head_parents "$root/mrgwt")" = 2 ] && echo "ok   REAL linked merge kept both parents" || { echo "FAIL REAL linked merge recorded no two-parent commit"; fail=1; }
  settle "$root/mrgwt"
  (cd "$root/mrg" && git log -z --format=%H%x1f%an%x1f%ae%x1f%cn%x1f%ce%x1f%P%x1f%B mwt) > "$root/fix/mlog.z"
  "$repo/tools/flea-commit-rules" "$root/fix/mlog.z" "$root/out" > "$root/fix/mlog.out" 2>&1
  check "commit-rules accepts every commit the merge hooks recorded" 0 $?
else
  echo "SKIP REAL hk and git hook cases: no pinned hk 2.4.0 on PATH"
fi

if [ "$fail" -eq 0 ]; then echo "hook-gate: PASS"; else echo "hook-gate: FAIL"; fi
exit "$fail"
