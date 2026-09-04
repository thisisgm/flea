#!/usr/bin/env bash
# Proves that the shipped resource model permits sparse mappings and bounds resident process trees.
set -u
set -o pipefail
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

D=$FIXTURE_ROOT/flea-resources-test-$$
BIN=./target/release/flea
fail=0

check() {
  local label="$1" expected="$2" actual="$3"
  if [ "$expected" != "$actual" ]; then
    printf 'FAIL %s\n  expected: %s\n  actual:   %s\n' "$label" "$expected" "$actual"
    fail=1
  else
    printf 'ok   %s\n' "$label"
  fi
}

sandbox_make "$D"
mkdir -p "$D/cache" "$D/files" "$D/bin"

cc -O2 -x c -o "$D/memory-helper" - <<'EOF'
#include <sys/mman.h>
#include <sys/wait.h>
#include <unistd.h>
#include <stdlib.h>
#include <string.h>
#include <fcntl.h>
static int touch(unsigned long mb) {
  size_t n = mb * 1024UL * 1024UL;
  char *p = mmap(0, n, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
  if (p == MAP_FAILED) return 2;
  for (size_t i = 0; i < n; i += 4096) p[i] = 1;
  return 0;
}
int main(int argc, char **argv) {
  if (argc != 3) return 2;
  if (!strcmp(argv[1], "where")) {
    int in = open("/proc/self/cgroup", O_RDONLY);
    int out = open(argv[2], O_WRONLY | O_TRUNC);
    char buf[4096]; ssize_t n;
    if (in < 0 || out < 0) return 2;
    while ((n = read(in, buf, sizeof(buf))) > 0)
      if (write(out, buf, n) != n) return 2;
    return n < 0;
  }
  unsigned long mb = strtoul(argv[2], 0, 10);
  if (!strcmp(argv[1], "reserve"))
    return mmap(0, mb * 1024UL * 1024UL, PROT_NONE,
                MAP_PRIVATE | MAP_ANONYMOUS | MAP_NORESERVE, -1, 0) == MAP_FAILED;
  if (!strcmp(argv[1], "tree")) {
    pid_t child = fork();
    if (child == 0) _exit(touch(mb));
    int status = 0;
    return waitpid(child, &status, 0) < 0 || !WIFEXITED(status) || WEXITSTATUS(status);
  }
  return touch(mb);
}
EOF

"$BIN" --sandbox-test one-shot "$D/memory-helper" "$D/memory-helper" reserve 1536
check "a sparse 1.5 GiB mapping is allowed" "0" "$?"
low_result=$("$BIN" --sandbox-test disposable-low "$D/memory-helper" "$D/memory-helper" touch 128 2>/dev/null)
check "the disposable low-memory job returns accounting" "0" "$?"
check "resident memory above the hard limit fails" "failed" "$(printf '%s\n' "$low_result" | sed -n 's/.*ran=\([^ ]*\).*/\1/p')"
low_max=$(printf '%s\n' "$low_result" | sed -n 's/.*max=\([0-9]*\).*/\1/p')
low_oom=$(printf '%s\n' "$low_result" | sed -n 's/.*oom=\([0-9]*\).*/\1/p')
low_oom_kill=$(printf '%s\n' "$low_result" | sed -n 's/.*oom_kill=\([0-9]*\).*/\1/p')
: "${low_max:=0}" "${low_oom:=0}" "${low_oom_kill:=0}"
check "a cgroup memory event caused the failure" "yes" \
  "$([ "$((low_max + low_oom + low_oom_kill))" -gt 0 ] && printf yes || printf no)"
tree_result=$("$BIN" --sandbox-test disposable-low "$D/memory-helper" "$D/memory-helper" tree 128 2>/dev/null)
check "a grandchild shares the memory boundary" "failed" "$(printf '%s\n' "$tree_result" | sed -n 's/.*ran=\([^ ]*\).*/\1/p')"

place_result=$("$BIN" --sandbox-test disposable-place-fail "$D/memory-helper" /usr/bin/true 2>/dev/null)
check "forced placement failure returns accounting" "0" "$?"
check "placement failure does not start the job" "not_started" "$(printf '%s\n' "$place_result" | sed -n 's/.*ran=\([^ ]*\).*/\1/p')"
check "placement failure leaves no job leaf" "0" "$(printf '%s\n' "$place_result" | sed -n 's/.*leaves=\([0-9]*\).*/\1/p')"

hostile='${path%/*} file
name'
gate=$(printf x | "$BIN" --sandbox-gate -- /usr/bin/printf %s "$hostile")
check "the gate preserves hostile arguments" "$hostile" "$gate"
printf '' | "$BIN" --sandbox-gate -- /usr/bin/true >/dev/null 2>&1
check "the gate refuses EOF" "nonzero" "$([ "$?" -ne 0 ] && printf nonzero || printf zero)"
printf x | "$BIN" --sandbox-gate -- /definitely/not/here >/dev/null 2>&1
check "a final exec failure is distinct" "nonzero" "$([ "$?" -ne 0 ] && printf nonzero || printf zero)"

printf '' > "$D/wrapped-out"
wrapped_missing=$("$BIN" --sandbox-test broker-wrapped "$D/memory-helper" "$D/wrapped-out" /definitely/not/here 2>/dev/null)
check "the production wrapper reports a missing final decoder" "ran=not_started" "$wrapped_missing"
wrapped_failed=$("$BIN" --sandbox-test broker-wrapped "$D/memory-helper" "$D/wrapped-out" /usr/bin/false 2>/dev/null)
check "the production wrapper keeps a decoder failure distinct" "ran=failed" "$wrapped_failed"
check "the production wrapper removes every exec error file" "0" "$(find "$D" -name '.flea-*.png' -type f | wc -l | tr -d ' ')"

printf '' > "$D/job-path"
"$BIN" --sandbox-test broker-direct 1 "$D/memory-helper" where "$D/job-path" >/dev/null
job_relative=$(sed -n 's/^0:://p' "$D/job-path")
check "the broker placed the helper in a disposable job" "job-0" "${job_relative##*/}"
check "the disposable job is removed after the result" "no" "$([ -d "/sys/fs/cgroup$job_relative" ] && printf yes || printf no)"

ln -s /usr/bin/bwrap "$D/bin/bwrap"
ln -s /usr/bin/prlimit "$D/bin/prlimit"
printf '#!/usr/bin/env sh\nprintf x >> "%s"\nexit 1\n' "$D/scope-count" > "$D/bin/systemd-run"
chmod +x "$D/bin/systemd-run"
magick -size 8x8 'xc:#456789' "$D/files/setup.jpg"
for i in $(seq 0 7); do ln "$D/files/setup.jpg" "$D/files/setup-$i.jpg"; done
rows=$(seq -s, 0 8)
out=$(printf '{"c":"list","path":"%s","first":20}\n{"c":"thumb","rows":[%s]}\n{"c":"quit"}\n' "$D/files" "$rows" | XDG_CACHE_HOME="$D/cache" PATH="$D/bin:/usr/bin" timeout 30 "$BIN" --backend 2>/dev/null)
check "scope setup failures answer every thumbnail" "9" "$(printf '%s\n' "$out" | grep -c '"file":""')"
check "scope setup failures write no markers" "0" "$(find "$D/cache/thumbnails/fail/flea" -type f 2>/dev/null | wc -l | tr -d ' ')"
printf '' > "$D/scope-count"
printf '' > "$D/broker-out"
PATH="$D/bin:/usr/bin" "$BIN" --sandbox-test broker-burst "$D/memory-helper" "$D/broker-out" 12 /usr/bin/true >/dev/null 2>&1
check "one failed setup disables each worker" "4" "$(wc -c < "$D/scope-count" | tr -d ' ')"

printf '#!/usr/bin/env sh\nprintf x >> "%s"\nexec /usr/bin/systemd-run "$@"\n' "$D/scope-count-real" > "$D/bin/systemd-run"
chmod +x "$D/bin/systemd-run"
printf '' > "$D/burst-out"
PATH="$D/bin:/usr/bin" "$BIN" --sandbox-test broker-burst "$D/memory-helper" "$D/burst-out" 12 /usr/bin/true >/dev/null
check "the real broker burst succeeds" "0" "$?"
scope_count=$(wc -c < "$D/scope-count-real" | tr -d ' ')
check "twelve jobs start exactly four scopes" "4" "$scope_count"

sandbox_remove "$D"
[ "$fail" -eq 0 ] && printf 'resources: all checks passed\n'
exit "$fail"
