#!/usr/bin/env python3
"""Bounded native ARM evidence collection. Never edits production Rust or treats skips as proof."""
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shlex
import signal
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
IDENTITIES = json.loads((HERE / "arm-statx-source-identities.json").read_text())
PREFIX = "backend::flashdirect_tests::"
CASES = [
    "b3_a_piped_flash_lands_whole_on_an_o_direct_target",
    "b3_a_verify_reads_an_o_direct_target_through_aligned_buffers",
    "b3_a_verify_still_catches_a_flipped_byte_on_an_o_direct_target",
    "b3_a_wipe_zeroes_both_ends_of_an_o_direct_target",
    "b4_a_raw_image_with_a_ragged_tail_lands_on_an_o_direct_target",
]
SYSCALLS = "%file,%process,%desc,%signal,futex"
DIRECT_ALIGN = 4096
DIOALIGN_BIT = 0x2000


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def members(group):
    found = []
    for entry in Path("/proc").iterdir():
        if entry.name.isdigit():
            try:
                pid = int(entry.name)
                if os.getpgid(pid) == group and os.getsid(pid) == group:
                    found.append(pid)
            except ProcessLookupError:
                pass
    return found


def snapshot(group, target):
    target.mkdir(exist_ok=True)
    owned = members(group)
    write_json(target / "pids.json", owned)
    for pid in owned:
        for task in (Path("/proc") / str(pid) / "task").glob("*"):
            for name in ["status", "wchan", "syscall", "stack"]:
                try:
                    data = (task / name).read_text()
                except OSError as error:
                    data = repr(error)
                (target / f"{pid}-{task.name}-{name}.txt").write_text(data)


def signal_group(group, signum):
    try:
        os.killpg(group, signum)
    except ProcessLookupError:
        pass


def run(out, label, argv, seconds, cwd):
    record = {"label": label, "argv": argv, "shell_display": shlex.join(argv),
              "cwd": str(cwd), "limit_seconds": seconds, "start_epoch": time.time()}
    write_json(out / f"{label}.json", record)
    print(f"PHASE BEGIN {label}: {shlex.join(argv)}", flush=True)
    process = None
    with (out / f"{label}.log").open("wb", buffering=0) as log:
        try:
            process = subprocess.Popen(argv, cwd=cwd, stdout=log, stderr=subprocess.STDOUT,
                                       start_new_session=True)
            record["pid"] = process.pid
            write_json(out / f"{label}.json", record)
            try:
                record["returncode"] = process.wait(timeout=seconds)
                record["timed_out"] = False
            except subprocess.TimeoutExpired:
                record["timed_out"] = True
                record["returncode"] = 124
                snapshot(process.pid, out / f"{label}-timeout-proc")
        finally:
            if process is not None:
                alive = members(process.pid)
                record["cleanup_pids_before"] = alive
                if alive:
                    signal_group(process.pid, signal.SIGTERM)
                    try:
                        process.wait(timeout=2)
                    except subprocess.TimeoutExpired:
                        pass
                    if members(process.pid):
                        signal_group(process.pid, signal.SIGKILL)
                try:
                    process.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    record["unkillable_process"] = True
                record["cleanup_pids_remaining"] = members(process.pid)
                if record["cleanup_pids_remaining"]:
                    snapshot(process.pid, out / f"{label}-remaining-proc")
            record["end_epoch"] = time.time()
            write_json(out / f"{label}.json", record)
    print(f"PHASE END {label}: {json.dumps(record)}", flush=True)
    return record


def traced(out, label, command, seconds, source):
    return run(out, label, ["strace", "-ff", "-ttt", "-T", "-yy", "-v", "-s", "96",
                           "-o", str(out / f"{label}.strace"), "-e", f"trace={SYSCALLS}",
                           "--", *command], seconds, source)



def trace_evidence(out, label):
    facts = {"capabilities": 0, "aligned_reads": 0, "aligned_writes": 0, "flushes": 0}
    with (out / f"{label}.phases.jsonl").open("w") as phases:
        for trace in sorted(out.glob(f"{label}.strace.*")):
            pending = None
            for number, line in enumerate(trace.open(errors="replace"), 1):
                # Sample pair: pread64(5</root/drive.img>, <unfinished ...> and <... pread64 resumed>"bytes", 4096, 0) = 4096
                if "<unfinished ...>" in line:
                    call_name = re.search(r"\b([a-zA-Z0-9_]+)\(", line)
                    pending = (call_name[1], line.split("<unfinished ...>", 1)[0]) if call_name else None
                    continue
                if "resumed>" in line:
                    resumed = re.search(r"<\.\.\. ([a-zA-Z0-9_]+) resumed>", line)
                    if pending is None or not resumed or resumed[1] != pending[0]:
                        continue
                    line = pending[1] + line.split("resumed>", 1)[1]
                    pending = None
                if "statx(" in line and "/probe>" in line and re.search(r"\)\s*=\s*0\s", line):
                    # Sample: stx_mask=STATX_BASIC_STATS|STATX_DIOALIGN, stx_dio_mem_align=64
                    mask = re.search(r"stx_mask=([^,}]+)", line)
                    memory = re.search(r"stx_dio_mem_align=(\d+)", line)
                    if mask and memory and int(memory[1]) > 1:
                        reported = "STATX_DIOALIGN" in mask[1].split("|")
                        if not reported:
                            try:
                                reported = bool(int(mask[1], 0) & DIOALIGN_BIT)
                            except ValueError:
                                pass
                        facts["capabilities"] += int(reported)
                if "/drive.img>" in line:
                    # Sample: pwrite64(5</root/drive.img>, "bytes", 4096, 0) = 4096 <0.001>
                    call = re.search(r"\b(pread64|pwrite64)\(.*,\s*(\d+),\s*(\d+)\s*\)\s*=\s*(\d+)\s", line)
                    if call and int(call[2]) >= DIRECT_ALIGN and int(call[2]) % DIRECT_ALIGN == 0 and int(call[3]) % DIRECT_ALIGN == 0 and int(call[4]) > 0:
                        facts["aligned_reads" if call[1] == "pread64" else "aligned_writes"] += 1
                    if "fsync(" in line and re.search(r"\)\s*=\s*0\s", line):
                        facts["flushes"] += 1
                if any(word in line for word in ["statx(", "/probe>", "/drive.img>", "fcntl(", "execve(", "wait4(", "futex("]):
                    phases.write(json.dumps({"trace": trace.name, "line": number, "operation": line.rstrip()}) + "\n")
    return facts


def successful(record):
    return (record.get("returncode") == 0 and not record.get("timed_out", True)
            and not record.get("cleanup_pids_remaining", [True]))


def operation_proof(evidence, capabilities, reads, writes, flushes):
    return (evidence["capabilities"] == capabilities
            and (not reads or evidence["aligned_reads"] > 0)
            and (not writes or evidence["aligned_writes"] > 0)
            and (not flushes or evidence["flushes"] > 0))


def main(source, out):
    out.mkdir(parents=True, exist_ok=True)
    if (out / "result.json").exists():
        raise RuntimeError("refusing to overwrite an earlier diagnostic receipt")
    results = {"status": "INCOMPLETE", "identities": IDENTITIES, "machine": platform.machine(),
               "control_commit": os.environ.get("GITHUB_SHA"),
               "runner_image": {key: os.environ.get(key) for key in ["ImageOS", "ImageVersion", "RUNNER_ARCH"]},
               "control_sha256": {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in HERE.iterdir() if p.is_file()}}
    write_json(out / "result.json", results)
    try:
        facts = ["uname -a", "getconf PAGE_SIZE", "getconf LONG_BIT", "id",
                 "findmnt -T . -o TARGET,SOURCE,FSTYPE,OPTIONS", "stat -f .",
                 "cat /proc/self/mountinfo", "cat /proc/meminfo", "df -h .",
                 "rustc -vV", "cargo -V", "cc --version", "strace -V"]
        for index, command in enumerate(facts):
            run(out, f"fact-{index:02}", ["bash", "-c", command], 10, source)
        if platform.machine() != "aarch64":
            raise RuntimeError("native aarch64 required; x86/emulation is not ARM evidence")
        expected = os.environ.get("ARM_DIRECT_SOURCE_SHA", "")
        if not re.fullmatch(r"[0-9a-f]{40}", expected):
            raise RuntimeError("missing immutable diagnostic-parent source SHA")
        head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=source, text=True).strip()
        tree = subprocess.check_output(["git", "rev-parse", "HEAD^{tree}"], cwd=source, text=True).strip()
        dirty = subprocess.check_output(["git", "status", "--porcelain"], cwd=source, text=True)
        results["actual_source"] = {"expected_commit": expected, "commit": head, "tree": tree, "status": dirty}
        if head != expected or dirty:
            raise RuntimeError("source identity or cleanliness mismatch")
        for name, wanted in IDENTITIES["source_sha256"].items():
            if hashlib.sha256((source / name).read_bytes()).hexdigest() != wanted:
                raise RuntimeError(f"reviewed source hash mismatch: {name}")
        base = source / ".superpowers/tmp"
        base.mkdir(parents=True, exist_ok=True)
        control = Path(tempfile.mkdtemp(prefix="flea-test-direct-statx-isolated-", dir=base))
        (control / ".flea-test-sandbox").write_text("statx/aligned regular-file control\n")
        results["isolated_root"] = str(control)
        run(out, "fixture-mount", ["findmnt", "-T", str(control), "-o", "TARGET,SOURCE,FSTYPE,OPTIONS"], 10, source)
        results["fixture_statvfs"] = list(os.statvfs(control))
        compile_c = run(out, "compile-control", ["cc", "-std=c11", "-O0", "-g", "-Wall", "-Wextra", "-Werror",
                                                str(HERE / "direct-statx-control.c"), "-o", str(out / "direct-statx-control")], 30, source)
        results["compile_control"] = compile_c
        if not successful(compile_c):
            raise RuntimeError("safe isolated control compilation failed")
        isolated = traced(out, "isolated", [str(out / "direct-statx-control"), str(control)], 30, source)
        isolated["evidence"] = trace_evidence(out, "isolated")
        isolated["strict_proof"] = successful(isolated) and operation_proof(isolated["evidence"], 1, True, True, True)
        results["isolated"] = isolated
        if not isolated["strict_proof"]:
            raise RuntimeError("isolated statx/aligned control did not prove safe completed I/O; backend tests were not launched")
        build = run(out, "compile-rust", ["cargo", "test", "--locked", "-p", "flea", "--bin", "flea",
                                         "--no-run", "--message-format=json"], 600, source)
        results["compile_rust"] = build
        if not successful(build):
            raise RuntimeError("bounded Rust compilation failed; no test verdict")
        executables = []
        # Sample: {"reason":"compiler-artifact","profile":{"test":true},"target":{"name":"flea"},"executable":"/x/flea-test"}
        for line in (out / "compile-rust.log").read_text().splitlines():
            try:
                item = json.loads(line)
            except ValueError:
                continue
            if (item.get("reason") == "compiler-artifact" and item.get("profile", {}).get("test")
                    and item.get("target", {}).get("name") == "flea" and item.get("executable")):
                executables.append(item["executable"])
        if len(executables) != 1:
            raise RuntimeError(f"expected one Cargo backend test executable, got {executables}")
        binary = executables[0]
        results["binary_sha256"] = hashlib.sha256(Path(binary).read_bytes()).hexdigest()
        groups = [("parallel-four", [PREFIX + case for case in CASES[:4]], 4, 90, 6, True, True, True)]
        for index, case in enumerate(CASES):
            groups.append((f"single-{index}", [PREFIX + case], 1, 60, 3 if index == 3 else 1,
                           index in [1, 2], index in [0, 3, 4], index in [0, 3, 4]))
        contracts = [PREFIX + "arm_oflags_flash_recognizes_the_target_direct_io_bit",
                     "backend::flashfake::statx_layout_matches_linux_direct_io_contract",
                     "backend::flashfake::statx_alignment_fails_closed_without_a_successful_usable_capability"]
        groups.append(("native-contracts", contracts, 1, 30, 0, False, False, False))
        results["tests"] = []
        for label, cases, threads, limit, capabilities, reads, writes, flushes in groups:
            record = traced(out, label, [binary, "--exact", *cases, "--nocapture", f"--test-threads={threads}"], limit, source)
            text = (out / f"{label}.log").read_text(errors="replace")
            record["evidence"] = trace_evidence(out, label)
            record["no_silent_skip"] = "so this test proves nothing here" not in text
            record["all_expected_tests_passed"] = all(re.search(r"^test " + re.escape(case) + r" \.\.\. ok$", text, re.M) for case in cases)
            record["strict_proof"] = bool(successful(record) and record["no_silent_skip"]
                                          and record["all_expected_tests_passed"]
                                          and operation_proof(record["evidence"], capabilities, reads, writes, flushes))
            results["tests"].append(record)
            write_json(out / "result.json", results)
        results["status"] = ("ARM_ALIGNED_CONTROL_COMPLETE" if isolated["strict_proof"]
                             and all(item["strict_proof"] for item in results["tests"]) else "DIAGNOSTIC_NONPASS")
    except BaseException as error:
        results["error"] = repr(error)
        results["status"] = "DIAGNOSTIC_NONPASS"
    finally:
        # Retain raw receipts and marked failed fixtures, including any unkillable aligned-I/O wait.
        inventory = []
        base = source / ".superpowers/tmp"
        if base.exists():
            for root in sorted(base.glob("flea-test-direct-*")):
                inventory.append({"root": str(root), "marked": (root / ".flea-test-sandbox").is_file(),
                                  "files": [{"name": p.name, "size": p.stat().st_size} for p in root.iterdir() if p.is_file()]})
        results["retained_fixture_inventory"] = inventory
        results["source_diff"] = subprocess.run(["git", "diff", "--stat"], cwd=source, capture_output=True, text=True, timeout=10).stdout
        write_json(out / "result.json", results)
        hashes = [f"{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.relative_to(out)}" for p in sorted(out.rglob("*")) if p.is_file() and p.name != "SHA256SUMS"]
        (out / "SHA256SUMS").write_text("\n".join(hashes) + "\n")
        print(json.dumps(results, indent=2), flush=True)
    return 0 if results["status"] == "ARM_ALIGNED_CONTROL_COMPLETE" else 1


if __name__ == "__main__":
    def cancelled(signum, frame):
        raise RuntimeError(f"diagnostic interrupted by signal {signum}")
    signal.signal(signal.SIGTERM, cancelled)
    signal.signal(signal.SIGINT, cancelled)
    sys.exit(main(Path(sys.argv[1]).resolve(strict=True), Path(sys.argv[2]).resolve()))
