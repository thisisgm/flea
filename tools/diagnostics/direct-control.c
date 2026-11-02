/* Isolated regular-file control for DirectDir's alignment probe. No block devices. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>

enum {
    DIRECT_ALIGN = 4096,
    PROBE_SKEW = 1,
    PROBE_LEN = 512,
    BACKING_BLOCKS = 2,
    DRIVE_BYTES = 16000000,
    RAGGED_BYTES = 611,
    PATTERN = 0x5a,
};

static void phase(const char *name, const char *edge, long result, int error) {
    struct timespec now;
    clock_gettime(CLOCK_REALTIME, &now);
    printf("PHASE %ld.%09ld %s %s result=%ld errno=%d (%s)\n",
           now.tv_sec, now.tv_nsec, name, edge, result, error, strerror(error));
    fflush(stdout);
}

#define CALL(name, expression) \
    (phase(name, "BEGIN", 0, 0), errno = 0, \
     result = (expression), error = errno, \
     phase(name, "END", result, error), result)

int main(int argc, char **argv) {
    setvbuf(stdout, NULL, _IONBF, 0);
    if (argc != 2) return 2;
    char *root = realpath(argv[1], NULL);
    if (!root) return 2;
    printf("FACT page_size=%ld direct_align=%d probe_len=%d probe_skew=%d O_DIRECT=%#o root=%s\n",
           sysconf(_SC_PAGESIZE), DIRECT_ALIGN, PROBE_LEN, PROBE_SKEW, O_DIRECT, root);
    int parent = open(root, O_RDONLY | O_DIRECTORY | O_NOFOLLOW);
    free(root);
    if (parent < 0) return 2;
    long result;
    int error;
    int fd = CALL("open-direct", openat(parent, "probe", O_CREAT | O_EXCL | O_RDWR | O_DIRECT | O_NOFOLLOW, 0600));
    if (fd < 0) return (error == EINVAL || error == EOPNOTSUPP) ? 77 : 2;
    struct stat st;
    if (fstat(fd, &st) || !S_ISREG(st.st_mode)) return 2;
    struct statx sx = {0};
    (void)CALL("statx-dio-alignment", statx(fd, "", AT_EMPTY_PATH, STATX_DIOALIGN, &sx));
    printf("FACT statx_dioalign_present=%d dio_mem_align=%u dio_offset_align=%u\n",
           !!(sx.stx_mask & STATX_DIOALIGN), sx.stx_dio_mem_align, sx.stx_dio_offset_align);
    (void)CALL("filesystem-transfer-alignment", fpathconf(fd, _PC_REC_XFER_ALIGN));
    unsigned char *backing = calloc(BACKING_BLOCKS, DIRECT_ALIGN);
    if (!backing) return 2;
    size_t skew = (DIRECT_ALIGN - (uintptr_t)backing % DIRECT_ALIGN) % DIRECT_ALIGN + PROBE_SKEW;
    printf("FACT allocation=%p skew=%zu probe=%p probe_mod4096=%zu\n",
           (void *)backing, skew, (void *)(backing + skew), (uintptr_t)(backing + skew) % DIRECT_ALIGN);
    (void)CALL("misaligned-pwrite-512-offset0", pwrite(fd, backing + skew, PROBE_LEN, 0));
    /* Unsupported or accepted misalignment is a NONPASS, never a successful control. */
    if (result != -1 || error != EINVAL) return 77;
    free(backing);
    void *aligned = NULL;
    if (posix_memalign(&aligned, DIRECT_ALIGN, DIRECT_ALIGN)) return 2;
    memset(aligned, PATTERN, DIRECT_ALIGN);
    printf("FACT aligned=%p aligned_mod4096=%zu bytes=%d\n", aligned, (uintptr_t)aligned % DIRECT_ALIGN, DIRECT_ALIGN);
    if (CALL("truncate-16000000", ftruncate(fd, DRIVE_BYTES)) != 0) return 2;
    if (CALL("aligned-pwrite-4096-offset0", pwrite(fd, aligned, DIRECT_ALIGN, 0)) != DIRECT_ALIGN) return 2;
    if (CALL("aligned-fsync", fsync(fd)) != 0) return 2;
    memset(aligned, 0, DIRECT_ALIGN);
    if (CALL("aligned-pread-4096-offset0", pread(fd, aligned, DIRECT_ALIGN, 0)) != DIRECT_ALIGN) return 2;
    for (size_t i = 0; i < DIRECT_ALIGN; ++i) if (((unsigned char *)aligned)[i] != PATTERN) return 2;
    int flags = CALL("get-flags", fcntl(fd, F_GETFL));
    if (flags < 0 || !(flags & O_DIRECT)) return 2;
    if (CALL("clear-direct", fcntl(fd, F_SETFL, flags & ~O_DIRECT)) != 0) return 2;
    if (CALL("ragged-pwrite-611-offset4096", pwrite(fd, aligned, RAGGED_BYTES, DIRECT_ALIGN)) != RAGGED_BYTES) return 2;
    if (CALL("restore-direct", fcntl(fd, F_SETFL, flags)) != 0) return 2;
    if (CALL("restored-flags", fcntl(fd, F_GETFL)) != flags) return 2;
    if (CALL("final-fsync", fsync(fd)) != 0) return 2;
    memset(aligned, 0, DIRECT_ALIGN);
    if (CALL("ragged-readback-4096-offset4096", pread(fd, aligned, DIRECT_ALIGN, DIRECT_ALIGN)) != DIRECT_ALIGN) return 2;
    for (size_t i = 0; i < DIRECT_ALIGN; ++i)
        if (((unsigned char *)aligned)[i] != (i < RAGGED_BYTES ? PATTERN : 0)) return 2;
    if (CALL("close-direct", close(fd)) != 0) return 2;
    free(aligned);
    close(parent);
    puts("DIRECT_CONTROL_COMPLETE: misalignment rejected with EINVAL and aligned I/O verified");
    return 0;
}
