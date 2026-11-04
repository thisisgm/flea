#!/usr/bin/python3
"""Prepare an external CI patch and exercise its exact payload in private fixtures."""
import argparse
import difflib
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys


HELPER = r'''#!/usr/bin/python3
"""Fail-closed workspace evidence checks for the staged Flea CI runner."""
import hashlib
import json
from pathlib import Path
import re
import sys


def require(condition, message):
    if not condition:
        raise ValueError(message)


def binaries(work, profile):
    fields = []
    for name, suffix in (("flea", ""), ("flea-gui", "_gui")):
        path = Path(work) / "target" / profile / name
        require(path.is_file() and path.stat().st_mode & 0o111, f"missing executable {path}")
        size = path.stat().st_size
        require(size, f"empty executable {path}")
        digest = hashlib.sha256()
        with path.open("rb") as stream:
            while chunk := stream.read(1024 * 1024):
                digest.update(chunk)
        fields += [f"{profile}{suffix}_sha256={digest.hexdigest()}",
                   f"{profile}{suffix}_bytes={size}"]
    print(" ".join(fields))


def dependencies(path):
    lines = [line.strip() for line in Path(path).read_text().splitlines() if line.strip()]
    # Sample tree line: flea v0.4.0 (/work/flea)
    require(len(lines) == 1 and re.fullmatch(r"flea v\S+ \(.+\)", lines[0]),
            "core tree must contain exactly flea and no dependencies")
    print("core-dependencies: PASS packages=1 edges=0")


def tests(metadata_path, log_path):
    # Sample metadata: {"workspace_members":["flea"],"packages":[{"id":"flea","name":"flea","targets":[]}]}
    metadata = json.loads(Path(metadata_path).read_text())
    packages = [p for p in metadata["packages"] if p["id"] in metadata["workspace_members"]]
    require({p["name"] for p in packages} == {"flea", "flea-gui"}, "unexpected workspace packages")
    expected = set()
    owners = {}
    for package in packages:
        parent = Path(package["manifest_path"]).parent
        for target in package["targets"]:
            crate = target["name"].replace("-", "_")
            kinds = set(target["kind"])
            if target["test"] and kinds.intersection({"lib", "bin", "test", "example", "proc-macro"}):
                source = str(Path(target["src_path"]).relative_to(parent))
                key = ("run", crate, source)
                require(key not in expected, f"ambiguous test target {key}")
                expected.add(key)
                owners[key] = package["name"]
            if target["doctest"] and kinds.intersection({"lib", "proc-macro"}):
                key = ("doc", crate, "")
                require(key not in expected, f"ambiguous doc target {key}")
                expected.add(key)
                owners[key] = package["name"]
    require(expected and {owners[k] for k in expected} == {"flea", "flea-gui"}, "missing workspace test targets")
    text = Path(log_path).read_text()
    # Sample failing child output: child: FAIL failed operation
    require(not re.search(r"(^|[: ])FAIL[: ]|^test result: FAILED\.|^SKIP ", text, re.M), "failing or skipped child output")
    seen = set()
    passed = {"flea": 0, "flea-gui": 0}
    active = None
    count = None
    last = None

    def finish():
        if active is None:
            return
        require(last is not None and count is not None, f"no own totals for {active}")
        require(active not in seen, f"duplicate test target {active}")
        seen.add(active)
        passed[owners[active]] += last

    for line in text.splitlines():
        # Sample cargo header:      Running unittests src/lib.rs (target/debug/deps/flea-abc123)
        run = re.match(r"^\s+Running (?:unittests )?(\S+) \(([^()]+)\)$", line)
        # Sample doc-test header:    Doc-tests flea_gui
        doc = re.match(r"^\s+Doc-tests (\S+)$", line)
        if run or doc:
            finish()
            if doc:
                active = ("doc", doc[1], "")
            else:
                binary = Path(run[2]).name
                # Sample target pair: src/lib.rs and flea_gui-abc123 from target/debug/deps/flea_gui-abc123
                matches = [key for key in expected if key[0] == "run" and key[2] == run[1]
                           and re.fullmatch(re.escape(key[1]) + r"-[0-9a-f]+", binary)]
                require(len(matches) == 1, f"unexpected or ambiguous target header {line}")
                active = matches[0]
            require(active in expected, f"unexpected target {active}")
            count = None
            last = None
        elif active is not None:
            # Sample test count: running 1 test
            running = re.fullmatch(r"running ([0-9]+) tests?", line)
            if running and count is None:
                count = int(running[1])
            # Sample totals: test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
            totals = re.fullmatch(r"test result: ok\. ([0-9]+) passed; ([0-9]+) failed; ([0-9]+) ignored; ([0-9]+) measured; ([0-9]+) filtered out; finished in [0-9.]+s", line)
            if totals:
                values = list(map(int, totals.groups()))
                if count is not None and sum(values[:4]) == count:
                    require(values[1] == values[4] == 0, f"failed or filtered target {active}")
                    last = values[0]
    finish()
    require(seen == expected, f"missing test targets {sorted(expected - seen)}")
    require(all(passed.values()), f"zero executed tests for a workspace package: {passed}")
    print(f"workspace-tests: PASS targets={len(seen)} flea={passed['flea']} flea-gui={passed['flea-gui']}")


def main():
    command, *args = sys.argv[1:]
    try:
        if command == "binaries":
            binaries(*args)
        elif command == "dependencies":
            dependencies(*args)
        elif command == "tests":
            tests(*args)
        else:
            raise ValueError(f"unknown command {command}")
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(f"FAIL workspace evidence: {error}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
'''


LANE_FUNCTIONS = {
    "step": r'''step() {
  local name=$1 t0
  shift
  t0=$(date +%s%N)
  ( cd "$work" && "$@" ) > "$out/$name.log" 2>&1
  STEP_CHILD_RC=$?
  STEP_RC=$STEP_CHILD_RC
  if [ "$STEP_RC" = 0 ] && grep -aqE '(^|[: ])FAIL[: ]' "$out/$name.log"; then STEP_RC=1; fi
  STEP_SECS=$(secs_since "$t0")
}''',
    "step_data": r'''step_data() {
  local name=$1 t0
  shift
  t0=$(date +%s%N)
  ( cd "$work" && "$@" ) > "$out/$name.stdout" 2> "$out/$name.stderr"
  STEP_CHILD_RC=$?
  STEP_RC=$STEP_CHILD_RC
  cat "$out/$name.stdout" "$out/$name.stderr" > "$out/$name.log" || STEP_RC=$?
  if [ "$STEP_RC" = 0 ] && grep -aqE '(^|[: ])FAIL[: ]' "$out/$name.log"; then STEP_RC=1; fi
  STEP_SECS=$(secs_since "$t0")
}''',
    "cargo_build": r'''cargo_build() {
  local name=$1
  shift
  step "$name" cargo build --locked --workspace "$@"
  result "$name rc=$STEP_RC child_rc=$STEP_CHILD_RC secs=$STEP_SECS warnings=$(warnings_in "$name") compiled=$(grep -c '^ *Compiling flea v' "$out/$name.log") gui_compiled=$(grep -c '^ *Compiling flea-gui v' "$out/$name.log")"
}''',
    "lane_prep": r'''lane_prep() {
  extract_fresh
  cargo_build build-debug
  cargo_build build-release --release
  local debug release
  debug=$(python3 /in/flea-ci-workspace.py binaries "$work" debug) || fail "$debug"
  release=$(python3 /in/flea-ci-workspace.py binaries "$work" release) || fail "$release"
  result "binaries $debug $release"
  if grep -q '^RESULT build-debug rc=0 .*warnings=0 .*compiled=1 gui_compiled=1$' "$out/results" \
    && grep -q '^RESULT build-release rc=0 .*warnings=0 .*compiled=1 gui_compiled=1$' "$out/results"; then
    touch /work/prep.done
  fi
}''',
    "lane_cargo_test": r'''lane_cargo_test() {
  local name=cargo-test profile=() audit_rc=0
  if [ "${1:-}" = --release ]; then name=cargo-test-release; profile=(--release); fi
  extract_fresh
  step_data metadata cargo metadata --locked --no-deps --format-version=1
  [ "$STEP_RC" = 0 ] || fail "workspace metadata failed; see metadata.log"
  step "$name" cargo test --locked --workspace "${profile[@]}" -- --nocapture
  python3 /in/flea-ci-workspace.py tests "$out/metadata.stdout" "$out/$name.log" > "$out/$name-audit.log" 2>&1 || audit_rc=$?
  result "$name rc=$STEP_RC child_rc=$STEP_CHILD_RC secs=$STEP_SECS warnings=$(warnings_in "$name") $(test_totals "$name") skipped=$(skipped_tests "$name") compiled=$(grep -c '^ *Compiling flea' "$out/$name.log") workspace_audit=$audit_rc failed_tests=$(failed_tests "$name")"
}''',
    "deny_offline": r'''# cargo-deny 0.20.2 opens https://github.com/rustsec/advisory-db at advisory-db-3157b0e258782691 under its db root, so the image's pinned snapshot is linked there.
deny_offline() {
  local snapshot=/usr/share/flea-ci/advisory-db root=${CARGO_HOME:-$HOME/.cargo}/advisory-dbs
  [ -d "$snapshot/.git" ] || { echo "deny: FAIL no advisory-db snapshot at $snapshot in the image"; return 2; }
  mkdir -p "$root" && ln -sfn "$snapshot" "$root/advisory-db-3157b0e258782691" || { echo "deny: FAIL cannot link $snapshot under $root"; return 2; }
  cargo deny --offline --locked check
}''',
    "lane_gpui_checks": r'''lane_gpui_checks() {
  extract_fresh
  local name
  for name in fmt strict-clippy core-dependencies deny gpui-lint; do
    case $name in
      fmt) step "$name" cargo fmt --all -- --check ;;
      strict-clippy) step "$name" cargo clippy --locked --workspace --all-targets -- -D warnings ;;
      core-dependencies) step_data "$name" cargo tree --locked -p flea --prefix none --edges normal,build
        local tree_rc=$STEP_RC child_rc=$STEP_CHILD_RC
        python3 /in/flea-ci-workspace.py dependencies "$out/$name.stdout" > "$out/core-dependencies-audit.log" 2>&1 || tree_rc=1
        STEP_RC=$tree_rc; STEP_CHILD_RC=$child_rc ;;
      deny) step "$name" deny_offline ;;
      gpui-lint) step "$name" tools/flea-gpui-lint ;;
    esac
    result "$name rc=$STEP_RC child_rc=$STEP_CHILD_RC secs=$STEP_SECS last=$(tail -1 "$out/$name.log" | tr ' ' '_' | cut -c1-120)"
  done
}''',
    "lane_suites": r'''lane_suites() {
  [ "$#" -gt 0 ] || fail "suites needs at least one suite name"
  copy_prep
  shard_runall
  local before after debug_before release_before debug_gui release_gui
  debug_gui=$(python3 /in/flea-ci-workspace.py binaries "$work" debug) || fail "$debug_gui"
  release_gui=$(python3 /in/flea-ci-workspace.py binaries "$work" release) || fail "$release_gui"
  before="$debug_gui $release_gui"
  debug_before=$(sha_of "$work/target/debug/flea")
  release_before=$(sha_of "$work/target/release/flea")
  export FLEA_CI_SUITES="$*"
  step run-all tests/run-all.sh
  debug_gui=$(python3 /in/flea-ci-workspace.py binaries "$work" debug) || fail "$debug_gui"
  release_gui=$(python3 /in/flea-ci-workspace.py binaries "$work" release) || fail "$release_gui"
  after="$debug_gui $release_gui"
  local tally
  tally=$(grep -E '^run-all: [0-9]+ suite\(s\) run, [0-9]+ failed$' "$out/run-all.log" | tail -1)
  result "run-all rc=$STEP_RC child_rc=$STEP_CHILD_RC secs=$STEP_SECS asked=$# ran=$(echo "$tally" | cut -d' ' -f2) failed=$(echo "$tally" | cut -d' ' -f5) debug_same=$([ "$(sha_of "$work/target/debug/flea")" = "$debug_before" ] && echo yes || echo NO) release_same=$([ "$(sha_of "$work/target/release/flea")" = "$release_before" ] && echo yes || echo NO) workspace_same=$([ "$before" = "$after" ] && echo yes || echo NO)"
  grep -E '^  \S+ +(ok|FAIL|SKIP) ' "$out/run-all.log" | sed 's/^  /SUITE /' >> "$out/results"
}''',
}


HOST_FUNCTIONS = {
    "heavy_lane": r'''heavy_lane() {
  case ${lane_cmd[$1]%% *} in protocol|suites|calibrate|cargo-test|gpui-checks|clippy) return 0 ;; *) return 1 ;; esac
}''',
    "release_waiting": r'''release_waiting() {
  local key prep host prep_ok running
  local ready
  for host in $hosts; do
    prep=$host/prep
    running=0
    for key in "${lane_names[@]}"; do
      [ "${lane_host[$key]}" = "$host" ] && heavy_lane "$key" || continue
      case ${state[$key]} in running|attempted) running=$((running + 1)) ;; esac
    done
    ready=()
    prep_ok=unknown
    for key in "${lane_names[@]}"; do
      [ "${lane_host[$key]}" = "$host" ] && [ "${state[$key]}" = waiting ] || continue
      if needs_prep "$key"; then
        [ "${state[$prep]:-}" = done ] || continue
        if [ "$prep_ok" = unknown ]; then
          prep_ok=no
          [ "${lane_rc[$prep]}" = 0 ] && on "$host" "test -f '$run_dir/lanes/prep/work/prep.done'" && prep_ok=yes
        fi
        if [ "$prep_ok" = no ]; then
          state[$key]=notrun
          echo "  $(date +%T) NOT RUN $key: the prep lane on $host failed"
          continue
        fi
      fi
      if heavy_lane "$key"; then
        [ "$running" -lt "$suite_lanes_per_host" ] || continue
        running=$((running + 1))
      fi
      ready+=("$key")
    done
    [ "${#ready[@]}" -eq 0 ] || launch_on "$host" "${ready[@]}"
  done
}''',
    "verdict_build": r'''verdict_build() {
  local check=$1 files f detail="" ok=PASS profile binary key host count
  profile=${check#build-}
  files=$(results_with "$check")
  [ -n "$files" ] || { verdict_line "$check" "NOT RUN" "no lane reported it"; return; }
  for host in $hosts; do
    count=0
    for f in $files; do case $(where "$f") in "$host/"*) count=$((count + 1)) ;; esac; done
    [ "$count" = 1 ] || ok=FAIL
  done
  for f in $files; do
    [ "$(field "$f" "$check" rc)" = 0 ] && [ "$(field "$f" "$check" warnings)" = 0 ] \
      && [ "$(field "$f" "$check" compiled)" = 1 ] && [ "$(field "$f" "$check" gui_compiled)" = 1 ] || ok=FAIL
    detail="$detail $(where "$f"): rc=$(field "$f" "$check" rc) warnings=$(field "$f" "$check" warnings);"
  done
  for binary in '' _gui; do
    local sums="" sum bytes
    key=${profile}${binary}
    for f in $files; do
      sum=$(field "$f" binaries "${key}_sha256"); bytes=$(field "$f" binaries "${key}_bytes")
      [[ $sum =~ ^[0-9a-f]{64}$ ]] && [ "$(num "$bytes")" -gt 0 ] || ok=FAIL
      sums="$sums $sum"
    done
    [ "$(echo $sums | tr ' ' '\n' | sort -u | wc -l)" = 1 ] || ok=FAIL
    detail="$detail $key sha256=$sum bytes=$bytes;"
  done
  verdict_line "$check" "$ok" "$detail"
}''',
    "stage_receipt": r'''stage_receipt() {
  local f sum
  : > "$stage/in/worker-files.sha256" || die "cannot open the worker receipt"
  for f in flea-ci-lane flea-ci-unit flea-ci-status flea-ci-deps flea-ci-workspace.py flea-commit-rules suite-evidence.txt minipc-only.txt dri-suites.txt layout tree.id image.id; do
    [ -s "$stage/in/$f" ] || die "staged input $f is missing or empty"
    sum=$(sha256sum "$stage/in/$f" | cut -d' ' -f1) || die "cannot hash $f"
    echo "$sum  $f" >> "$stage/in/worker-files.sha256" || die "cannot record $f"
  done
}''',
}


PRESERVED_IMAGES = r'''check_images() {
  local host have
  image=$(grep '^id=' "$image_id_file" | cut -d= -f2)
  [ -n "$image" ] || die "no id= line in $image_id_file"
  for host in $hosts; do
    have=$(on "$host" "podman image inspect $image --format '{{.Id}}' 2>/dev/null")
    [ "$have" = "$image" ] || die "$host does not hold flea-ci $image_label $image (run $ship_hint)"
  done
}'''


PRESERVED_TOOL = r'''verdict_tool() {
  local check=$1 files f rc ok=PASS detail=""
  files=$(results_with "$check")
  [ -n "$files" ] || { verdict_line "$check" "NOT RUN" "no lane reported it"; return; }
  for f in $files; do
    rc=$(field "$f" "$check" rc)
    [ "$rc" = 0 ] || ok=FAIL
    detail="$detail$(where "$f"): rc=$rc secs=$(field "$f" "$check" secs) last: $(field "$f" "$check" last | tr '_' ' '); "
  done
  verdict_line "$check" "$ok" "$detail"
}'''


def replace_function(source, name, replacement):
    pattern = rf"^{re.escape(name)}\(\) \{{\n.*?^\}}"
    changed, count = re.subn(pattern, lambda _: replacement, source, flags=re.M | re.S)
    if count != 1:
        raise ValueError(f"expected exactly one {name} function, found {count}")
    return changed


def replace_once(source, old, new):
    if source.count(old) != 1:
        raise ValueError(f"expected one patch anchor: {old!r}")
    return source.replace(old, new, 1)


def transform(files):
    files = dict(files)
    lane = files["bin/flea-ci-lane"]
    for name, body in LANE_FUNCTIONS.items():
        if name in ("step_data", "deny_offline", "lane_gpui_checks"):
            lane = replace_once(lane, "lane_tool() {", body + "\n\nlane_tool() {")
        else:
            lane = replace_function(lane, name, body)
    lane = replace_once(lane, "  start_session\n", '  (cd /in && sha256sum -c worker-files.sha256) > "$out/input-receipt.log" 2>&1 || fail "staged inputs changed"\n  start_session\n')
    lane = replace_once(lane, "    qmllint-gate) lane_tool qmllint-gate tools/flea-qmllint-gate ;;", "    gpui-checks) lane_gpui_checks ;;")
    lane = replace_once(lane, "qmllint-gate|file-budget", "gpui-checks|file-budget")
    lane = replace_once(lane, "qmllint-gate | file-budget", "gpui-checks | file-budget")
    host = files["bin/flea-ci"]
    if PRESERVED_IMAGES not in host:
        raise ValueError("image check changed; refresh its fixture from the reviewed input")
    if PRESERVED_TOOL not in host:
        raise ValueError("tool verdict changed; refresh its fixture from the reviewed input")
    for name, body in HOST_FUNCTIONS.items():
        host = replace_function(host, name, body)
    host = replace_once(host, '    sk=$(field "$f" "$check" skipped); c=$(field "$f" "$check" compiled)', '    [ "$(field "$f" "$check" workspace_audit)" = 0 ] || ok=FAIL\n    sk=$(field "$f" "$check" skipped); c=$(field "$f" "$check" compiled)')
    host = replace_once(host, '  verdict_tool qmllint-gate', '  local gpui_check\n  for gpui_check in fmt strict-clippy core-dependencies deny gpui-lint; do verdict_tool "$gpui_check"; done')
    host = replace_once(host, '    [ "$(field "$f" run-all debug_same)" = yes ] && [ "$(field "$f" run-all release_same)" = yes ]', '    [ "$(field "$f" run-all workspace_same)" = yes ] && [ "$(field "$f" run-all debug_same)" = yes ] && [ "$(field "$f" run-all release_same)" = yes ]')
    host = replace_once(host, '  stage_checker\n', '  cp "$ci_home/bin/flea-ci-workspace.py" "$stage/in/" || die "cannot stage workspace checker"\n  cp "$evidence_rules_file" "$stage/in/suite-evidence.txt" || die "cannot stage evidence rules"\n  cp "$minipc_only_file" "$stage/in/minipc-only.txt" || die "cannot stage native dispositions"\n  cp "$ci_home/dri-suites.txt" "$stage/in/dri-suites.txt" || die "cannot stage DRM inventory"\n  cp "$layout" "$stage/in/layout" || die "cannot stage layout"\n  cp "$stage/in/suite-evidence.txt" "$stage/in/minipc-only.txt" "$stage/in/layout" "$evidence/" || die "cannot retain suite inputs"\n  evidence_rules_file=$evidence/suite-evidence.txt\n  minipc_only_file=$evidence/minipc-only.txt\n  stage_checker\n')
    host = replace_once(host, 'for f in flea-ci-lane flea-ci-unit flea-commit-rules; do', 'for f in flea-ci-lane flea-ci-unit flea-ci-workspace.py flea-commit-rules suite-evidence.txt minipc-only.txt dri-suites.txt layout; do')
    host = replace_once(host, 'cat "$d/flea-ci-lane" "$d/flea-ci-unit" "$d/flea-commit-rules"', 'cat "$d/flea-ci-lane" "$d/flea-ci-unit" "$d/flea-ci-workspace.py" "$d/flea-commit-rules" "$d/suite-evidence.txt" "$d/minipc-only.txt" "$d/dri-suites.txt" "$d/layout"')
    host = replace_once(host, '--unit=flea-ci-$run-$lane --slice=', '--unit=flea-ci-$run-$lane --working-directory=\'$run_dir\' --slice=')
    unit = files["bin/flea-ci-unit"]
    unit = replace_once(unit, 'image=$(cat "$run_dir/in/image.id")', '(cd "$run_dir/in" && sha256sum -c worker-files.sha256) || { echo "flea-ci-unit: staged inputs changed" >&2; exit 125; }\nimage=$(cat "$run_dir/in/image.id")')
    # Sample inventory operand: /usr/bin/grep -qx -- "$s" /srv/ci/dri-suites.txt 2>/dev/null
    inventory = r'''(?m)(/usr/bin/grep -qx -- "\$s" )(?:"[^"\n]*/dri-suites\.txt"|'[^'\n]*/dri-suites\.txt'|[^\s"'`;]+/dri-suites\.txt)(?= 2>/dev/null)'''
    unit, count = re.subn(inventory, lambda match: match[1] + '"$run_dir/in/dri-suites.txt"', unit)
    if count != 1:
        raise ValueError(f"expected one DRM inventory operand, found {count}")
    unit = replace_once(unit, '"$image" setpriv --inh-caps=-all --ambient-caps=-all -- /bin/bash /in/flea-ci-lane "$@" &', '"$image" setpriv --inh-caps=-all --ambient-caps=-all -- env -i PATH=/usr/bin:/bin HOME=/home/gm USER=gm LOGNAME=gm LANG=en_US.UTF-8 \\\n    setsid -w /bin/bash /in/flea-ci-lane "$@" < /dev/null &')
    files.update({"bin/flea-ci": host, "bin/flea-ci-lane": lane, "bin/flea-ci-unit": unit,
                  "bin/flea-ci-workspace.py": HELPER,
                  "layouts/default.layout": replace_once(files["layouts/default.layout"], "west qml qmllint-gate", "west gpui gpui-checks")})
    return files


def prepare(preimage, output):
    output.mkdir(parents=True, exist_ok=True)
    names = ["bin/flea-ci", "bin/flea-ci-lane", "bin/flea-ci-unit", "layouts/default.layout"]
    originals = {name: (preimage / name).read_text() for name in names}
    candidates = transform(originals)
    patch = []
    hashes = {}
    for name, text in candidates.items():
        target = output / "candidate" / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text)
        old = originals.get(name, "")
        hashes[name] = {"before": hashlib.sha256(old.encode()).hexdigest() if name in originals else None,
                        "after": hashlib.sha256(text.encode()).hexdigest()}
        for line in difflib.unified_diff(old.splitlines(True), text.splitlines(True),
                                         fromfile="a/" + name if name in originals else "/dev/null", tofile="b/" + name):
            patch.append(line if line.endswith("\n") else line + "\n\\ No newline at end of file\n")
    (output / "ops.patch").write_text("".join(patch))
    (output / "patch-hashes.json").write_text(json.dumps(hashes, indent=2) + "\n")


SHELL_BASICS = r'''
set -uo pipefail
fail() { echo "fixture refused: $*"; exit 2; }
result() { echo "RESULT $*" >> "$out/results"; }
secs_since() { echo 0.0; }
warnings_in() { grep -c '^warning' "$out/$1.log"; }
extract_fresh() { :; }
num() { case $1 in ''|*[!0-9]*) echo 0 ;; *) echo "$1" ;; esac; }
# Sample result line: RESULT build-debug rc=0 warnings=0 compiled=1 gui_compiled=1
field() { grep -m1 "^RESULT $2 " "$1" | tr ' ' '\n' | grep "^$3=" | cut -d= -f2-; }
results_with() { grep -l "^RESULT $1 " "$evidence"/*/*/out/results; }
where() { local f=${1#"$evidence"/}; echo "${f%%/out/results}"; }
verdict_line() { echo "$1 $2 $3"; }
'''


def fixtures(root):
    if not root.is_absolute() or not (root / ".flea-test-sandbox").is_file():
        raise ValueError("fixtures need an absolute marked root")
    helper = root / "flea-ci-workspace.py"
    helper.write_text(HELPER)
    checks = []

    def run(name, command, accepted=True, contains=None):
        completed = subprocess.run(command, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                   stderr=subprocess.STDOUT, text=True, timeout=30)
        (root / f"{name}.log").write_text(completed.stdout)
        good = (completed.returncode == 0) == accepted
        if contains is not None:
            good = good and contains in completed.stdout
        checks.append({"name": name, "passed": good, "rc": completed.returncode,
                       "log": f"{name}.log"})
        print(f"{'ok' if good else 'FAIL'} {name} rc={completed.returncode}")
        return completed

    def helper_run(name, args, accepted=True, contains=None):
        return run(name, [sys.executable, "-B", str(helper), *map(str, args)], accepted, contains)

    def shell(name, body, accepted=True, contains=None):
        path = root / f"{name}.sh"
        path.write_text(SHELL_BASICS + body)
        return run(name, ["/bin/bash", str(path)], accepted, contains)

    work = root / "work"
    for profile in ("debug", "release"):
        for binary in ("flea", "flea-gui"):
            path = work / "target" / profile / binary
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(f"fixture {binary} {profile}\n")
            path.chmod(0o755)
        helper_run(f"binaries-{profile}", ["binaries", work, profile])
    gui = work / "target/debug/flea-gui"
    gui.rename(gui.with_suffix(".saved"))
    helper_run("missing-gui-build", ["binaries", work, "debug"], False, "missing executable")
    gui.with_suffix(".saved").rename(gui)
    tree = root / "tree.txt"
    tree.write_text("flea v0.4.0 (/work/flea)\n")
    helper_run("core-no-dependencies", ["dependencies", tree])
    for name, content in (("core-dependency", "flea v0.4.0 (/work/flea)\nother v1.0\n"),
                          ("empty-core-tree", ""), ("wrong-core-tree", "flea-gui v0.4.0 (/work/flea)\n")):
        tree.write_text(content)
        helper_run(name, ["dependencies", tree], False)

    metadata = {"packages": [], "workspace_members": ["flea", "flea-gui"]}
    sections = []
    for package in metadata["workspace_members"]:
        crate = package.replace("-", "_")
        targets = []
        for kind, source in (("lib", "src/lib.rs"), ("bin", "src/main.rs")):
            targets.append({"name": package, "kind": [kind], "test": True, "doctest": kind == "lib",
                            "src_path": f"/work/{package}/{source}"})
            sections.append((package, f"     Running unittests {source} (target/debug/deps/{crate}-abc123)\nrunning 1 test\ntest works ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n"))
        metadata["packages"].append({"id": package, "name": package,
                                     "manifest_path": f"/work/{package}/Cargo.toml", "targets": targets})
    for package in metadata["workspace_members"]:
        sections.append((package, f"   Doc-tests {package.replace('-', '_')}\nrunning 0 tests\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n"))
    meta = root / "metadata.json"
    meta.write_text(json.dumps(metadata))
    log = root / "cargo-test.log"
    valid = "".join(content for _, content in sections)
    cases = {
        "workspace-tests": valid,
        "missing-gui-test-target": "".join(content for package, content in sections if package == "flea"),
        "missing-doc-test": "".join(content for _, content in sections[:-1]),
        "missing-unit-target": "".join(content for _, content in sections[1:]),
        "zero-tests": valid.replace("running 1 test", "running 0 tests").replace("ok. 1 passed", "ok. 0 passed"),
        "hidden-failing-child": valid.replace("test works ... ok", "child: FAIL failed operation", 1),
        "hidden-rust-child": valid.replace("test works ... ok", "test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s", 1),
        "filtered-tests": valid.replace("0 filtered out", "1 filtered out", 1),
        "missing-totals": valid.replace("test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n", "", 1),
        "skipped-child": valid + "SKIP no tool\n",
        "duplicate-target": valid + sections[0][1],
    }
    for name, text in cases.items():
        log.write_text(text)
        helper_run(name, ["tests", meta, log], name == "workspace-tests")
    log.write_text(valid)

    evidence = root / "evidence"
    good_lines = "RESULT build-debug rc=0 warnings=0 compiled=1 gui_compiled=1\nRESULT binaries " + " ".join(
        f"debug{suffix}_sha256={'a' * 64} debug{suffix}_bytes=100" for suffix in ("", "_gui")) + "\n"
    for host in ("east", "west"):
        path = evidence / host / "prep/out/results"
        path.parent.mkdir(parents=True)
        path.write_text(good_lines)
    judge = f"evidence={str(evidence)!r}\nhosts='east west'\n" + HOST_FUNCTIONS["verdict_build"] + "\nverdict_build build-debug\n"
    shell("both-host-identities", judge, contains="build-debug PASS")
    west_results = evidence / "west/prep/out/results"
    for name, content in (
        ("host-image-binary-drift", good_lines.replace("debug_gui_sha256=" + "a" * 64, "debug_gui_sha256=" + "b" * 64)),
        ("missing-gui-compilation", good_lines.replace("gui_compiled=1", "gui_compiled=0")),
        ("missing-binary-hash", good_lines.replace("debug_gui_sha256=" + "a" * 64, "debug_gui_sha256=")),
        ("warning-build", good_lines.replace("warnings=0", "warnings=1")),
    ):
        west_results.write_text(content)
        shell(name, judge, contains="build-debug FAIL")
    west_results.rename(west_results.with_suffix(".saved"))
    shell("missing-host-build", judge, contains="build-debug FAIL")
    west_results.with_suffix(".saved").rename(west_results)
    west_results.write_text(good_lines)

    out = root / "out"
    out.mkdir()
    step = f"work={str(work)!r}\nout={str(out)!r}\n" + LANE_FUNCTIONS["step"] + "\n" + LANE_FUNCTIONS["step_data"]
    shell("child-status", step + '\nstep hidden /bin/bash -c "exit 7"\n[ "$STEP_CHILD_RC" = 7 ] && [ "$STEP_RC" = 7 ]\n')
    shell("hidden-fail-output", step + '\nstep hidden /bin/bash -c "echo child: FAIL broken; exit 0"\n[ "$STEP_CHILD_RC" = 0 ] && [ "$STEP_RC" = 1 ]\n')

    staged = root / "stage"
    staged_in = staged / "in"
    staged_in.mkdir(parents=True)
    receipt_names = "flea-ci-lane flea-ci-unit flea-ci-status flea-ci-deps flea-ci-workspace.py flea-commit-rules suite-evidence.txt minipc-only.txt dri-suites.txt layout tree.id image.id".split()
    for name in receipt_names:
        (staged_in / name).write_text(f"fixture {name}\n")
    shell("stage-receipt", f"stage={str(staged)!r}\ndie() {{ exit 2; }}\n" + HOST_FUNCTIONS["stage_receipt"] + '\nstage_receipt\ncd "$stage/in" && sha256sum -c worker-files.sha256\n')
    for name in ("flea-ci-lane", "suite-evidence.txt", "minipc-only.txt", "dri-suites.txt", "layout", "image.id"):
        path = staged_in / name
        old = path.read_text()
        path.write_text(old + "changed\n")
        shell("stale-" + name.replace(".", "-"), f"cd {str(staged_in)!r}\nsha256sum -c worker-files.sha256\n", False)
        path.write_text(old)

    fake_bin = root / "bin"
    fake_bin.mkdir()
    fake_cargo = fake_bin / "cargo"
    fake_cargo.write_text(r'''#!/bin/bash
printf '%s\n' "$*" >> "$COMMAND_LOG"
case $* in
  'tree --locked -p flea --prefix none --edges normal,build') echo 'flea v0.4.0 (/work/flea)' ;;
  'deny --offline --locked check') [ "${MISSING_DENY:-0}" = 0 ] || { echo 'error: no such command: deny'; exit 101; }
    [ -d "$CARGO_HOME/advisory-dbs/advisory-db-3157b0e258782691/.git" ] || { echo 'error: failed to load advisory database'; exit 1; } ;;
  build*) echo '   Compiling flea v0.4.0 (/work/flea)'
    case " $* " in *' --workspace '*) echo '   Compiling flea-gui v0.4.0 (/work/flea/crates/flea-gui)' ;; esac ;;
  metadata*) cat "$FAKE_META" ;;
  test*) cat "$FAKE_TEST"; exit "${FAKE_TEST_RC:-0}" ;;
esac
''')
    fake_cargo.chmod(0o755)
    tools = work / "tools"
    tools.mkdir()
    lint = tools / "flea-gpui-lint"
    lint.write_text('#!/bin/bash\necho gpui-lint >> "$COMMAND_LOG"\n')
    lint.chmod(0o755)
    command_log = root / "commands.log"
    cargo_body = step + f"\nexport PATH={str(fake_bin)!r}:$PATH COMMAND_LOG={str(command_log)!r}\n"
    cargo_body += LANE_FUNCTIONS["cargo_build"] + '\n: > "$out/results"\ncargo_build build-debug\ncargo_build build-release --release\ncat "$out/results"\n'
    shell("workspace-builds", cargo_body, contains="compiled=1 gui_compiled=1")
    build_commands = command_log.read_text().splitlines()
    expected_builds = ["build --locked --workspace", "build --locked --workspace --release"]
    checks.append({"name": "both-build-profiles-executed", "passed": build_commands == expected_builds,
                   "rc": 0, "log": "workspace-builds.log"})
    print(f"{'ok' if build_commands == expected_builds else 'FAIL'} both-build-profiles-executed")
    shell("missing-workspace-build", cargo_body.replace("cargo build --locked --workspace", "cargo build --locked -p flea"), contains="compiled=1 gui_compiled=0")
    prep_done = root / "prep.done"
    prep_body = step + f"\nexport PATH={str(fake_bin)!r}:$PATH COMMAND_LOG={str(command_log)!r}\n"
    prep_body += LANE_FUNCTIONS["cargo_build"] + "\n" + LANE_FUNCTIONS["lane_prep"].replace("/in/flea-ci-workspace.py", str(helper)).replace("/work/prep.done", str(prep_done))
    prep_body += '\n: > "$out/results"\nlane_prep\ncat "$out/results"\n'
    shell("prep-publishes-complete-workspace", prep_body + f"\n[ -f {str(prep_done)!r} ]\n")
    if prep_done.exists():
        prep_done.unlink()
    incomplete = prep_body.replace("cargo build --locked --workspace", "cargo build --locked -p flea")
    shell("prep-refuses-incomplete-workspace", incomplete + f"\n[ ! -e {str(prep_done)!r} ]\n", contains="gui_compiled=0")
    test_body = step + f"\nexport PATH={str(fake_bin)!r}:$PATH COMMAND_LOG={str(command_log)!r} FAKE_META={str(meta)!r} FAKE_TEST={str(log)!r}\n"
    test_body += 'test_totals() { echo "passed=4 failed=0 ignored=0 result_lines=6 targets=6 filtered_out=0"; }\nskipped_tests() { echo 0; }\nfailed_tests() { echo none; }\n'
    test_body += LANE_FUNCTIONS["lane_cargo_test"].replace("/in/flea-ci-workspace.py", str(helper))
    command_log.write_text("")
    test_body += '\n: > "$out/results"\nlane_cargo_test\nlane_cargo_test --release\ncat "$out/results"\n'
    profile_run = shell("test-both-profiles", test_body)
    # Sample audit result: RESULT cargo-test rc=0 child_rc=0 secs=0.1 warnings=0 compiled=1 workspace_audit=0 failed_tests=none
    audits = re.findall(r"^RESULT (cargo-test(?:-release)?) .*workspace_audit=(\d+)", profile_run.stdout, re.M)
    correct_audits = audits == [("cargo-test", "0"), ("cargo-test-release", "0")]
    checks.append({"name": "both-profile-audits", "passed": correct_audits, "rc": profile_run.returncode,
                   "log": "test-both-profiles.log"})
    print(f"{'ok' if correct_audits else 'FAIL'} both-profile-audits")
    test_commands = command_log.read_text().splitlines()
    expected_tests = ["metadata --locked --no-deps --format-version=1", "test --locked --workspace -- --nocapture",
                      "metadata --locked --no-deps --format-version=1", "test --locked --workspace --release -- --nocapture"]
    checks.append({"name": "both-test-profiles-executed", "passed": test_commands == expected_tests,
                   "rc": 0, "log": "test-both-profiles.log"})
    print(f"{'ok' if test_commands == expected_tests else 'FAIL'} both-test-profiles-executed")
    shell("test-child-status", "export FAKE_TEST_RC=5\n" + test_body, contains="rc=5 child_rc=5")
    command_log.write_text("")
    snapshot = root / "advisory-db"
    (snapshot / ".git").mkdir(parents=True)
    lint_body = step + "\n" + LANE_FUNCTIONS["deny_offline"].replace("/usr/share/flea-ci/advisory-db", str(snapshot))
    lint_body += "\n" + LANE_FUNCTIONS["lane_gpui_checks"].replace("/in/flea-ci-workspace.py", str(helper))
    lint_body += f"\nexport PATH={str(fake_bin)!r}:$PATH COMMAND_LOG={str(command_log)!r} CARGO_HOME={str(root / 'cargo-home')!r}\n: > \"$out/results\"\nlane_gpui_checks\ncat \"$out/results\"\n"
    lint_run = shell("lint-execution", lint_body)
    # Sample lint result: RESULT deny rc=0 child_rc=0 secs=0.0 last=ok
    lint_statuses = re.findall(r"^RESULT (\S+) rc=(\d+) child_rc=(\d+)", lint_run.stdout, re.M)
    expected_statuses = [(name, "0", "0") for name in ("fmt", "strict-clippy", "core-dependencies", "deny", "gpui-lint")]
    lint_success = lint_statuses == expected_statuses
    checks.append({"name": "all-lint-results-successful", "passed": lint_success,
                   "rc": lint_run.returncode, "log": "lint-execution.log"})
    print(f"{'ok' if lint_success else 'FAIL'} all-lint-results-successful")
    expected_commands = ["fmt --all -- --check", "clippy --locked --workspace --all-targets -- -D warnings",
                         "tree --locked -p flea --prefix none --edges normal,build", "deny --offline --locked check", "gpui-lint"]
    actual = command_log.read_text().splitlines()
    checks.append({"name": "all-lints-executed", "passed": actual == expected_commands,
                   "rc": 0, "log": "commands.log"})
    print(f"{'ok' if actual == expected_commands else 'FAIL'} all-lints-executed")
    shell("missing-image-tool", "export MISSING_DENY=1\n" + lint_body, contains="RESULT deny rc=101 child_rc=101")
    (snapshot / ".git").rename(snapshot / "git.saved")
    shell("missing-advisory-db", lint_body, contains="RESULT deny rc=2 child_rc=2")
    (snapshot / "git.saved").rename(snapshot / ".git")
    shutil.rmtree(root / "cargo-home")
    shell("wrong-advisory-db-name", lint_body.replace("advisory-db-3157b0e258782691\"", "advisory-db-0000000000000000\""), contains="RESULT deny rc=1 child_rc=1")
    lint.rename(lint.with_suffix(".saved"))
    shell("unexecuted-lint", lint_body, contains="RESULT gpui-lint rc=127 child_rc=127")
    lint.with_suffix(".saved").rename(lint)
    tool_out = evidence / "east/gpui/out"
    tool_out.mkdir(parents=True)
    tool_results = tool_out / "results"
    tool_judge = f"evidence={str(evidence)!r}\n" + PRESERVED_TOOL + "\nverdict_tool gpui-lint\n"
    tool_results.write_text("RESULT fmt rc=0 secs=0.0 last=ok\n")
    shell("missing-lint-result", tool_judge, contains="gpui-lint NOT RUN")
    tool_results.write_text("RESULT gpui-lint rc=127 secs=0.0 last=missing\n")
    shell("failed-lint-result", tool_judge, contains="gpui-lint FAIL")

    image = root / "image.id"
    image.write_text("id=fixture-pinned-image\n")
    image_body = f'image_id_file={str(image)!r}\nhosts="east west"\nimage_label=image\nship_hint=fixture\ndie() {{ fail "$@"; }}\n'
    shell("image-match", image_body + 'on() { echo fixture-pinned-image; }\n' + PRESERVED_IMAGES + "\ncheck_images\n")
    shell("image-mismatch", image_body + 'on() { echo different-image; }\n' + PRESERVED_IMAGES + "\ncheck_images\n", False)

    scheduler = r'''
declare -A lane_host lane_cmd state lane_rc
hosts=east
suite_lanes_per_host=3
lane_names=(east/s1 east/s2 east/test east/gpui east/clippy)
lane_cmd[east/s1]='suites fixture'
lane_cmd[east/s2]='suites fixture'
lane_cmd[east/test]=cargo-test
lane_cmd[east/gpui]=gpui-checks
lane_cmd[east/clippy]=clippy
for key in "${lane_names[@]}"; do lane_host[$key]=east; state[$key]=running; done
state[east/gpui]=waiting
state[east/clippy]=waiting
needs_prep() { return 1; }
launch_on() { printf 'launched %s\n' "$*"; }
'''
    scheduler += HOST_FUNCTIONS["heavy_lane"] + "\n" + HOST_FUNCTIONS["release_waiting"]
    shell("heavy-quota-full", scheduler + '\n[ -z "$(release_waiting)" ]\n')
    shell("heavy-quota-one-slot", scheduler + '\nstate[east/test]=done\n[ "$(release_waiting)" = "launched east east/gpui" ]\n')
    scheduler_run = root / "scheduler-run"
    (scheduler_run / "lanes/prep/work").mkdir(parents=True)
    prep_scheduler = scheduler + f"\nrun_dir={str(scheduler_run)!r}\n" + r'''
lane_names=(east/prep east/s1)
lane_host[east/prep]=east
lane_cmd[east/prep]=prep
state[east/prep]=done
lane_rc[east/prep]=1
state[east/s1]=waiting
needs_prep() { [ "$1" = east/s1 ]; }
on() { [ "$1" = east ] && /bin/bash -c "$2"; }
launch_on() { fail "failed prep released $*"; }
'''
    shell("failed-prep-lane-notrun", prep_scheduler + '\nrelease_waiting\n[ "${state[east/s1]}" = notrun ]\n', contains="NOT RUN east/s1")
    shell("missing-prep-marker-notrun", prep_scheduler + '\nlane_rc[east/prep]=0\nrelease_waiting\n[ "${state[east/s1]}" = notrun ]\n', contains="NOT RUN east/s1")
    shell("unfinished-prep-keeps-waiters", prep_scheduler + '\nstate[east/prep]=running\nrelease_waiting\n[ "${state[east/s1]}" = waiting ]\n')
    shell("successful-prep-releases-waiters", prep_scheduler + '\nlane_rc[east/prep]=0\ntouch "$run_dir/lanes/prep/work/prep.done"\nlaunch_on() { printf "launched %s\\n" "$*"; }\nrelease_waiting\n', contains="launched east east/s1")

    failed = sum(not check["passed"] for check in checks)
    (root / "checks.json").write_text(json.dumps(checks, indent=2) + "\n")
    print(f"fixture artifacts: {root}")
    print(f"PROVE_02_PASS checks={len(checks)} failed={failed}" if not failed else f"FAIL PROVE_02 checks={len(checks)} failed={failed}")
    return int(bool(failed))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixtures", type=Path)
    parser.add_argument("--prepare", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.prepare:
        if not args.output:
            parser.error("--prepare needs --output")
        prepare(args.prepare, args.output)
        return 0
    if args.fixtures:
        return fixtures(args.fixtures)
    parser.error("choose --prepare or --fixtures")


if __name__ == "__main__":
    sys.exit(main())
