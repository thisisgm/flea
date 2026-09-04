// mallopt(3) M_MMAP_THRESHOLD, the value at /usr/include/malloc.h line 136 on this box.
const M_MMAP_THRESHOLD: i32 = -3;
// A 100,000-name arena and its span vector both cross 128 KiB, so both are served by mmap and both go back to the OS on free.
const THRESHOLD_BYTES: i32 = 131072;

// std already links the system libc, so the one symbol is declared here rather than taking a crate.
extern "C" {
    fn mallopt(param: i32, value: i32) -> i32;
}

// Setting M_MMAP_THRESHOLD, M_TRIM_THRESHOLD or M_MMAP_MAX sets glibc's one no_dyn_threshold flag, so this stops the mmap ratchet that doubles the backend on a second large listing and freezes trim_threshold with it, the favourable direction; see AGENTS.md "The listing arena returns to the OS".
pub fn pin_mmap_threshold() {
    // corner: glibc 2.44 answers 1 for every parameter number here, including undefined ones, so there is no failure to report; see AGENTS.md.
    let _ = unsafe { mallopt(M_MMAP_THRESHOLD, THRESHOLD_BYTES) };
}

#[cfg(test)]
mod tests {
    use super::*;

    // The probe runs in a child behind this marker, because 127 other tests share this process's heap.
    const CHILD_MARKER: &str = "FLEA_HEAP_PROBE";
    const CHILD_TEST: &str = "heap::tests::a_freed_large_block_does_not_ratchet_the_mmap_threshold";
    // Larger than any threshold glibc adopts on its own, so freeing it is what would raise the threshold unpinned.
    const RATCHET_BYTES: usize = 8 * 1024 * 1024;
    // Above the pinned threshold even before the live heap size is added.
    const PROBE_FLOOR_BYTES: usize = 1024 * 1024;
    // Past every free chunk in the current heap, so only the threshold decides where the probe lands.
    const PROBE_MARGIN_BYTES: usize = 1024 * 1024;
    // Below the pinned threshold, so it must come from the arena that [heap] describes.
    const ARENA_BYTES: usize = 65536;

    // Sample input, one line of /proc/self/maps: "5601f0a21000-5601f0a42000 rw-p 00000000 00:00 0 [heap]"
    fn heap_span() -> Option<(usize, usize)> {
        let maps = std::fs::read_to_string("/proc/self/maps").ok()?;
        let line = maps.lines().find(|l| l.ends_with("[heap]"))?;
        let (lo, hi) = line.split_whitespace().next()?.split_once('-')?;
        Some((
            usize::from_str_radix(lo, 16).ok()?,
            usize::from_str_radix(hi, 16).ok()?,
        ))
    }

    fn in_heap(addr: usize) -> bool {
        match heap_span() {
            Some((lo, hi)) => addr >= lo && addr < hi,
            None => false,
        }
    }

    fn probe_bytes(heap_bytes: usize) -> usize {
        let probe_bytes = heap_bytes
            .checked_add(PROBE_MARGIN_BYTES)
            .expect("the heap size must leave room for a probe")
            .max(PROBE_FLOOR_BYTES);
        assert!(
            probe_bytes < RATCHET_BYTES,
            "the {} byte heap leaves no probe size below the {} byte ratchet",
            heap_bytes,
            RATCHET_BYTES
        );
        probe_bytes
    }

    #[test]
    fn probe_size_stays_between_the_live_heap_and_ratchet() {
        for heap_bytes in (ARENA_BYTES..RATCHET_BYTES - PROBE_MARGIN_BYTES).step_by(PROBE_MARGIN_BYTES) {
            let probe_bytes = probe_bytes(heap_bytes);
            assert!(probe_bytes > heap_bytes);
            assert!(probe_bytes < RATCHET_BYTES);
        }
    }

    // The guard itself: ratchet the threshold the way a first large listing does, then read where the probe landed.
    fn probe() {
        pin_mmap_threshold();
        let arena = vec![0u8; ARENA_BYTES];
        assert!(
            in_heap(arena.as_ptr() as usize),
            "a {} byte allocation at {:x} is outside [heap] {:x?}, so this thread is not on the main arena and the check below could not fail",
            ARENA_BYTES,
            arena.as_ptr() as usize,
            heap_span()
        );
        // glibc raises its mmap threshold to the size of any mmapped block it frees, unless a mallopt froze it.
        drop(std::hint::black_box(vec![0u8; RATCHET_BYTES]));
        let (lo, hi) = heap_span().expect("the probe needs a main heap");
        let probe_bytes = probe_bytes(hi - lo);
        let probed = vec![0u8; probe_bytes];
        assert!(
            !in_heap(probed.as_ptr() as usize),
            "a {} byte allocation came from [heap], so the mmap threshold ratcheted past it",
            probe_bytes
        );
        drop(arena);
        drop(probed);
    }

    // corner: this cannot redden for M_TRIM_THRESHOLD or M_MMAP_MAX, which freeze the ratchet by accident at the same default.
    #[test]
    fn a_freed_large_block_does_not_ratchet_the_mmap_threshold() {
        if std::env::var_os(CHILD_MARKER).is_some() {
            probe();
            return;
        }
        let exe = std::env::current_exe().expect("a test binary knows its own path");
        let out = std::process::Command::new(exe)
            .args(["--exact", "--test-threads=1", "--nocapture", CHILD_TEST])
            .env(CHILD_MARKER, "1")
            // libtest runs a test on a spawned thread, which glibc would hand its own mmapped arena, and nothing that arena serves is ever in [heap].
            .env("MALLOC_ARENA_MAX", "1")
            .output()
            .expect("the test binary re-executes");
        let report = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.status.success(), "the probe child failed: {}", report);
        // Renaming the test would filter the child to nothing and exit 0, so it has to say it ran one.
        assert!(
            report.contains("1 passed"),
            "the probe child ran no test, so CHILD_TEST no longer names this one: {}",
            report
        );
    }
}
