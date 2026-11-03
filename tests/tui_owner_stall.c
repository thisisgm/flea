// Test-only statx barrier and mount table, with explicit entry, release and return receipts.
#define _GNU_SOURCE
#include <dlfcn.h>
#include <fcntl.h>
#include <stdarg.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

int open64(const char *path, int flags, ...) {
    int (*real_open)(const char *, int, ...) = dlsym(RTLD_NEXT, "open64");
    mode_t mode = 0;
    if (flags & O_CREAT) {
        va_list args;
        va_start(args, flags);
        mode = (mode_t)va_arg(args, int);
        va_end(args);
    }
    const char *mounts = getenv("FLEA_OWNER_TEST_MOUNTS");
    if (mounts && strcmp(path, "/proc/self/mountinfo") == 0) path = mounts;
    return real_open(path, flags, mode);
}

static void marker(const char *path) {
    int fd = open(path, O_WRONLY | O_CREAT, 0600);
    if (fd >= 0) close(fd);
}

int statx(int fd, const char *path, int flags, unsigned int mask, struct statx *out) {
    int (*real_statx)(int, const char *, int, unsigned int, struct statx *) = dlsym(RTLD_NEXT, "statx");
    static atomic_int held;
    const char *target = getenv("FLEA_OWNER_TEST_STAT");
    const char *arm = getenv("FLEA_OWNER_TEST_ARM");
    if (path && target && arm && strcmp(path, target) == 0 && access(arm, F_OK) == 0
        && atomic_exchange(&held, 1) == 0) {
        marker(getenv("FLEA_OWNER_TEST_ENTERED"));
        char release[4096];
        snprintf(release, sizeof(release), "%s.stat", getenv("FLEA_OWNER_TEST_RELEASE"));
        while (access(release, F_OK) != 0) usleep(1000);
        int result = real_statx(fd, path, flags, mask, out);
        snprintf(release, sizeof(release), "%s.returned", getenv("FLEA_OWNER_TEST_RELEASE"));
        marker(release);
        return result;
    }
    return real_statx(fd, path, flags, mask, out);
}
