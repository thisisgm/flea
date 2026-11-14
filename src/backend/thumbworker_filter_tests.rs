use super::*;

// Where the kernel's own numbers live, Arch's first, then Debian and Ubuntu's multiarch copy, so the table is checked against something it did not write.
#[cfg(target_arch = "x86_64")]
const UNISTD: &[&str] = &[
    "/usr/include/asm/unistd_64.h",
    "/usr/include/x86_64-linux-gnu/asm/unistd_64.h",
];
// arm64 headers before Linux 6.11 generate no unistd_64.h; arm64 uses the generic table, so asm-generic holds its numbers.
#[cfg(target_arch = "aarch64")]
const UNISTD: &[&str] = &[
    "/usr/include/asm/unistd_64.h",
    "/usr/include/aarch64-linux-gnu/asm/unistd_64.h",
    "/usr/include/asm-generic/unistd.h",
];
// The compat 32-bit arch this kernel could also run, 32-bit x86 or 32-bit ARM, which the filter must kill.
#[cfg(target_arch = "x86_64")]
const AUDIT_ARCH_FOREIGN: u32 = 0x4000_0003;
#[cfg(target_arch = "aarch64")]
const AUDIT_ARCH_FOREIGN: u32 = 0x4000_0028;
// Where the clone flag bits live, so the thread bit the filter tests is checked against the kernel's too.
const SCHED_H: &str = "/usr/include/linux/sched.h";
// The exit signal glibc's fork() and posix_spawn put in clone's low byte, from asm/signal.h.
const SIGCHLD: u32 = 17;
// Every call a job must never make, named independently of the table under test.
#[cfg(target_arch = "x86_64")]
const MUST_REFUSE: &[&str] = &[
    "ioctl",
    "chmod",
    "fchmod",
    "chown",
    "fchown",
    "lchown",
    "utime",
    "setxattr",
    "lsetxattr",
    "fsetxattr",
    "removexattr",
    "lremovexattr",
    "fremovexattr",
    "utimes",
    "fchownat",
    "futimesat",
    "fchmodat",
    "utimensat",
    "io_uring_setup",
    "fchmodat2",
    "setxattrat",
    "removexattrat",
    "file_setattr",
    "fork",
    "vfork",
];
// The same calls on aarch64, which has no chmod, chown, lchown, utime, utimes, futimesat, fork or vfork to refuse.
#[cfg(target_arch = "aarch64")]
const MUST_REFUSE: &[&str] = &[
    "ioctl",
    "fchmod",
    "fchown",
    "setxattr",
    "lsetxattr",
    "fsetxattr",
    "removexattr",
    "lremovexattr",
    "fremovexattr",
    "fchownat",
    "fchmodat",
    "utimensat",
    "io_uring_setup",
    "fchmodat2",
    "setxattrat",
    "removexattrat",
    "file_setattr",
];

// The newest call the tables name (Linux 6.17); headers without it predate the tables and cannot check them.
const NEWEST_CALL: &str = "file_setattr";

// Sample input line: "#define __NR_fchmod 91".
fn kernel_numbers() -> (&'static str, std::collections::HashMap<String, u32>) {
    let (path, header) = UNISTD
        .iter()
        .find_map(|path| match std::fs::read_to_string(path) {
            Ok(text) => Some((*path, text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => panic!("{} did not read: {}", path, e),
        })
        .unwrap_or_else(|| {
            panic!(
                "no kernel header at {}: install linux-api-headers or linux-libc-dev",
                UNISTD.join(" or ")
            )
        });
    let mut numbers = std::collections::HashMap::new();
    for line in header.lines() {
        let mut words = line.split_whitespace();
        if let (Some("#define"), Some(name), Some(number)) =
            (words.next(), words.next(), words.next())
        {
            if let (Some(call), Ok(number)) = (name.strip_prefix("__NR_"), number.parse()) {
                numbers.insert(call.to_string(), number);
            }
        }
    }
    // Every kernel names read, so a header that yields no read is a parse failure, never an old kernel.
    assert!(
        numbers.contains_key("read"),
        "{} yielded no __NR_read, so the parse failed",
        path
    );
    (path, numbers)
}

// Sample input line: "#define CLONE_THREAD\t0x00010000\t/* Same thread group? */"
fn clone_bits() -> std::collections::HashMap<String, u32> {
    let header = std::fs::read_to_string(SCHED_H)
        .unwrap_or_else(|e| panic!("{} did not read: {}", SCHED_H, e));
    let mut bits = std::collections::HashMap::new();
    for line in header.lines() {
        let mut words = line.split_whitespace();
        if let (Some("#define"), Some(name), Some(value)) =
            (words.next(), words.next(), words.next())
        {
            if let (true, Some(Ok(value))) = (
                name.starts_with("CLONE_"),
                value
                    .strip_prefix("0x")
                    .map(|hex| u32::from_str_radix(hex, 16)),
            ) {
                bits.insert(name.to_string(), value);
            }
        }
    }
    bits
}

// Runs the filter the way the kernel does for one call and returns what it answers, so no call is ever made.
fn verdict_of(program: &[SockFilter], arch: u32, number: u32, first_argument: u32) -> u32 {
    let mut at = 0;
    let mut loaded = 0;
    loop {
        let step = &program[at];
        let here = at;
        let jump = |taken: bool| here + 1 + usize::from(if taken { step.jt } else { step.jf });
        at = match step.code {
            BPF_LD_W_ABS => {
                loaded = match step.k {
                    SECCOMP_DATA_ARCH => arch,
                    SECCOMP_DATA_NR => number,
                    SECCOMP_DATA_ARG0_LOW => first_argument,
                    other => panic!(
                        "the filter loads offset {}, which this walk does not model",
                        other
                    ),
                };
                here + 1
            }
            BPF_JEQ_K => jump(loaded == step.k),
            BPF_JGE_K => jump(loaded >= step.k),
            BPF_JSET_K => jump(loaded & step.k != 0),
            BPF_RET_K => return step.k,
            other => panic!(
                "the filter uses opcode {:#x}, which this walk does not model",
                other
            ),
        };
    }
}

// Headers naming the newest call must name every call, so a name they lack is a typo; older ones leave the newer calls unchecked (Ok(None)).
fn kernel_number(
    numbers: &std::collections::HashMap<String, u32>,
    header: &str,
    call: &str,
) -> Result<Option<u32>, String> {
    match numbers.get(call) {
        Some(number) => Ok(Some(*number)),
        None if numbers.contains_key(NEWEST_CALL) => {
            Err(format!("{} has no __NR_{}", header, call))
        }
        None => Ok(None),
    }
}

#[test]
fn older_headers_leave_newer_calls_unchecked_and_current_ones_fail_a_missing_name() {
    assert!(
        MUST_REFUSE.contains(&NEWEST_CALL),
        "the newest call is one the filter must refuse"
    );
    let current: std::collections::HashMap<String, u32> =
        [(NEWEST_CALL.to_string(), 1), ("fchmod".to_string(), 2)].into();
    let older: std::collections::HashMap<String, u32> = [("fchmod".to_string(), 2)].into();
    assert_eq!(kernel_number(&current, "h", "fchmod"), Ok(Some(2)));
    assert_eq!(
        kernel_number(&older, "h", "fchmod"),
        Ok(Some(2)),
        "older headers still check the calls they name"
    );
    assert_eq!(kernel_number(&older, "h", "setxattrat"), Ok(None));
    assert_eq!(
        kernel_number(&current, "h", "fchmdo"),
        Err("h has no __NR_fchmdo".to_string()),
        "a typo against current headers fails"
    );
}

#[test]
fn every_call_that_changes_a_file_or_starts_a_process_is_refused() {
    const THIS: &str = "backend::thumbworker::tests::every_call_that_changes_a_file_or_starts_a_process_is_refused";
    let (header, numbers) = kernel_numbers();
    let program = call_filter().expect("the filter did not build");
    let known =
        |call: &str| kernel_number(&numbers, header, call).unwrap_or_else(|why| panic!("{}", why));
    let number =
        |call: &str| known(call).unwrap_or_else(|| panic!("{} has no __NR_{}", header, call));
    let refused = SECCOMP_RET_ERRNO | EPERM;
    let mut unchecked = Vec::new();
    for (call, listed) in METADATA_WRITES.iter().chain(NEW_PROCESSES) {
        match known(call) {
            Some(kernel) => assert_eq!(
                *listed, kernel,
                "the table gives {} the number {}",
                call, listed
            ),
            None => unchecked.push(*call),
        }
    }
    for &call in MUST_REFUSE {
        match known(call) {
            Some(kernel) => assert_eq!(
                verdict_of(&program, AUDIT_ARCH_NATIVE, kernel, 0),
                refused,
                "{} is not refused",
                call
            ),
            None => unchecked.push(call),
        }
    }
    if !unchecked.is_empty() {
        unchecked.sort_unstable();
        unchecked.dedup();
        std::io::stderr()
            .write_all(
                format!(
                    "PARTIAL {}: {} predates __NR_{}, so {} went unchecked\n",
                    THIS,
                    header,
                    NEWEST_CALL,
                    unchecked.join(", ")
                )
                .as_bytes(),
            )
            .ok();
    }
    let bits = clone_bits();
    let flags = |names: &[&str]| {
        names
            .iter()
            .map(|name| {
                *bits
                    .get(*name)
                    .unwrap_or_else(|| panic!("{} has no {}", SCHED_H, name))
            })
            .fold(0, |all, bit| all | bit)
    };
    assert_eq!(
        CLONE_THREAD,
        flags(&["CLONE_THREAD"]),
        "the filter's thread bit is not the kernel's"
    );
    // The flags glibc's pthread_create, fork() and posix_spawn pass to clone on this box.
    let thread = flags(&[
        "CLONE_VM",
        "CLONE_FS",
        "CLONE_FILES",
        "CLONE_SIGHAND",
        "CLONE_THREAD",
        "CLONE_SYSVSEM",
        "CLONE_SETTLS",
        "CLONE_PARENT_SETTID",
        "CLONE_CHILD_CLEARTID",
    ]);
    let fork = flags(&["CLONE_CHILD_SETTID", "CLONE_CHILD_CLEARTID"]) | SIGCHLD;
    let spawn = flags(&["CLONE_VM", "CLONE_VFORK"]) | SIGCHLD;
    assert_eq!(
        verdict_of(&program, AUDIT_ARCH_NATIVE, number("clone"), thread),
        SECCOMP_RET_ALLOW,
        "a thread must start"
    );
    assert_eq!(
        verdict_of(&program, AUDIT_ARCH_NATIVE, number("clone"), fork),
        refused,
        "a forked process must not"
    );
    assert_eq!(
        verdict_of(&program, AUDIT_ARCH_NATIVE, number("clone"), spawn),
        refused,
        "a spawned process must not"
    );
    assert_eq!(
        verdict_of(&program, AUDIT_ARCH_NATIVE, number("clone3"), 0),
        SECCOMP_RET_ERRNO | ENOSYS
    );
    assert_eq!(
        verdict_of(&program, AUDIT_ARCH_NATIVE, number("read"), 0),
        SECCOMP_RET_ALLOW,
        "an ordinary call must run"
    );
    assert_eq!(
        verdict_of(
            &program,
            AUDIT_ARCH_NATIVE,
            number("read") | X32_SYSCALL_BIT,
            0
        ),
        refused,
        "an x32 call must not"
    );
    assert_eq!(
        verdict_of(&program, AUDIT_ARCH_FOREIGN, number("read"), 0),
        SECCOMP_RET_KILL_PROCESS
    );
}
