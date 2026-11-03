/* Regular-file statx capability and aligned direct-I/O control. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>

enum { DIRECT_ALIGN = 4096, DRIVE_BYTES = 16000000, RAGGED_BYTES = 611, PATTERN = 0x5a };
_Static_assert(sizeof(struct statx) == 256, "Linux statx size");
_Static_assert(offsetof(struct statx, stx_mask) == 0, "Linux statx mask offset");
_Static_assert(offsetof(struct statx, stx_dio_mem_align) == 152, "Linux statx memory alignment offset");
_Static_assert(offsetof(struct statx, stx_dio_offset_align) == 156, "Linux statx offset alignment offset");
_Static_assert(STATX_DIOALIGN == 0x2000, "Linux statx DIOALIGN request bit");

static int supports_fixed_geometry(uint32_t memory, uint32_t offset) {
    return memory > 1 && offset > 0 && DIRECT_ALIGN % memory == 0 && DIRECT_ALIGN % offset == 0;
}

static void phase(const char *name, const char *edge, long result, int error) {
    struct timespec now;
    clock_gettime(CLOCK_REALTIME, &now);
    printf("PHASE %ld.%09ld %s %s result=%ld errno=%d (%s)\n",
           now.tv_sec, now.tv_nsec, name, edge, result, error, strerror(error));
    fflush(stdout);
}

#define CALL(name, expression) \
    (phase(name, "BEGIN", 0, 0), errno = 0, result = (expression), error = errno, \
     phase(name, "END", result, error), result)

int main(int argc, char **argv) {
    setvbuf(stdout, NULL, _IONBF, 0);
    if (argc != 2) return 2;
    char *root = realpath(argv[1], NULL);
    if (!root) return 2;
    printf("FACT page_size=%ld direct_align=%d O_DIRECT=%#o root=%s\n",
           sysconf(_SC_PAGESIZE), DIRECT_ALIGN, O_DIRECT, root);
    int parent = open(root, O_RDONLY | O_DIRECTORY | O_NOFOLLOW);
    free(root);
    if (parent < 0) return 2;
    long result;
    int error;
    int probe = CALL("probe-open-direct", openat(parent, "probe", O_CREAT | O_EXCL | O_RDWR | O_DIRECT | O_NOFOLLOW, 0600));
    if (probe < 0) return (error == EINVAL || error == EOPNOTSUPP) ? 77 : 2;
    struct statx sx = {0};
    (void)CALL("statx-capability", statx(probe, "", AT_EMPTY_PATH, STATX_DIOALIGN, &sx));
    printf("FACT statx_mask=%#x dio_mem_align=%u dio_offset_align=%u\n",
           sx.stx_mask, sx.stx_dio_mem_align, sx.stx_dio_offset_align);
    if (result != 0 || !(sx.stx_mask & STATX_DIOALIGN) || sx.stx_dio_mem_align <= 1) return 77;
    if (!supports_fixed_geometry(sx.stx_dio_mem_align, sx.stx_dio_offset_align)) {
        puts("UNSUPPORTED: kernel alignment is incompatible with the fixed 4096-byte control");
        return 77;
    }
    if (CALL("probe-close", close(probe)) != 0) return 2;
    int fd = CALL("target-open-direct", openat(parent, "drive.img", O_CREAT | O_EXCL | O_RDWR | O_DIRECT | O_NOFOLLOW, 0600));
    if (fd < 0) return 2;
    struct stat st;
    if (fstat(fd, &st) || !S_ISREG(st.st_mode)) return 2;
    void *aligned = NULL;
    if (posix_memalign(&aligned, DIRECT_ALIGN, DIRECT_ALIGN)) return 2;
    printf("FACT aligned=%p modulo4096=%zu modulo_kernel_mem=%zu request_bytes=%d offset=0\n",
           aligned, (uintptr_t)aligned % DIRECT_ALIGN, (uintptr_t)aligned % sx.stx_dio_mem_align, DIRECT_ALIGN);
    if ((uintptr_t)aligned % sx.stx_dio_mem_align != 0) return 77;
    memset(aligned, PATTERN, DIRECT_ALIGN);
    if (CALL("truncate-target", ftruncate(fd, DRIVE_BYTES)) != 0) return 2;
    if (CALL("aligned-pwrite", pwrite(fd, aligned, DIRECT_ALIGN, 0)) != DIRECT_ALIGN) return 2;
    if (CALL("aligned-fsync", fsync(fd)) != 0) return 2;
    memset(aligned, 0, DIRECT_ALIGN);
    if (CALL("aligned-pread", pread(fd, aligned, DIRECT_ALIGN, 0)) != DIRECT_ALIGN) return 2;
    for (size_t i = 0; i < DIRECT_ALIGN; ++i) if (((unsigned char *)aligned)[i] != PATTERN) return 2;
    int flags = CALL("get-flags", fcntl(fd, F_GETFL));
    if (flags < 0 || !(flags & O_DIRECT)) return 2;
    if (CALL("clear-direct", fcntl(fd, F_SETFL, flags & ~O_DIRECT)) != 0) return 2;
    int buffered_flags = CALL("buffered-flags", fcntl(fd, F_GETFL));
    if (buffered_flags < 0 || (buffered_flags & O_DIRECT)) return 2;
    if (CALL("buffered-ragged-pwrite", pwrite(fd, aligned, RAGGED_BYTES, DIRECT_ALIGN)) != RAGGED_BYTES) return 2;
    if (CALL("restore-direct", fcntl(fd, F_SETFL, flags)) != 0) return 2;
    if (CALL("restored-flags", fcntl(fd, F_GETFL)) != flags) return 2;
    if (CALL("final-fsync", fsync(fd)) != 0) return 2;
    memset(aligned, 0, DIRECT_ALIGN);
    if (CALL("aligned-ragged-readback", pread(fd, aligned, DIRECT_ALIGN, DIRECT_ALIGN)) != DIRECT_ALIGN) return 2;
    for (size_t i = 0; i < DIRECT_ALIGN; ++i)
        if (((unsigned char *)aligned)[i] != (i < RAGGED_BYTES ? PATTERN : 0)) return 2;
    if (CALL("target-close", close(fd)) != 0) return 2;
    free(aligned);
    close(parent);
    puts("DIRECT_CONTROL_COMPLETE: statx capability and aligned direct I/O verified");
    return 0;
}
