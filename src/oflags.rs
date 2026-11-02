// open(2) flags this tree declares itself, because it takes no libc crate.

// O_NOFOLLOW is architecture specific: on arm64 the x86_64 value 0o400000 is O_LARGEFILE, a no-op on
// 64-bit, so one hardcoded constant drops a symlink guard without failing anything.
#[cfg(target_arch = "x86_64")]
pub const O_NOFOLLOW: i32 = 0o400000;
#[cfg(target_arch = "aarch64")]
pub const O_NOFOLLOW: i32 = 0o100000;
// A new architecture adds its own value from that target's asm/fcntl.h rather than inheriting one.
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
compile_error!("O_NOFOLLOW needs a verified value for this architecture");

// O_DIRECTORY is architecture specific for the same reason, and it is what refuses everything that is
// not a directory at the open rather than at a stat somebody could race.
#[cfg(target_arch = "x86_64")]
pub const O_DIRECTORY: i32 = 0o200000;
#[cfg(target_arch = "aarch64")]
pub const O_DIRECTORY: i32 = 0o40000;

// O_DIRECT swaps bits with O_DIRECTORY between the two supported Linux architectures.
#[cfg(target_arch = "x86_64")]
pub const O_DIRECT: i32 = 0o40000;
#[cfg(target_arch = "aarch64")]
pub const O_DIRECT: i32 = 0o200000;

// open(2) O_NONBLOCK, 0o4000 on both Linux architectures flea is built for; a FIFO's open returns for the fstat to refuse it.
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
pub const O_NONBLOCK: i32 = 0o4000;
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
compile_error!("O_NONBLOCK needs a verified value for this architecture");

#[cfg(test)]
mod tests {
    use super::*;

    // Evaluated even by a cross-target cargo check, which cannot run target tests.
    const _: () = {
        assert!(O_NOFOLLOW == libc::O_NOFOLLOW);
        assert!(O_DIRECTORY == libc::O_DIRECTORY);
        assert!(O_DIRECT == libc::O_DIRECT);
        assert!(O_NONBLOCK == libc::O_NONBLOCK);
    };

    #[test]
    fn arm_oflags_shared_constants_match_the_build_target_libc() {
        assert_eq!(O_NOFOLLOW, libc::O_NOFOLLOW);
        assert_eq!(O_DIRECTORY, libc::O_DIRECTORY);
        assert_eq!(O_DIRECT, libc::O_DIRECT);
        assert_eq!(O_NONBLOCK, libc::O_NONBLOCK);
    }
}
