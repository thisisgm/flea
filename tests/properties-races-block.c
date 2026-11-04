// Test-only opendir barrier: prove navigation works while a size worker is inside a syscall.
#define _GNU_SOURCE
#include <dirent.h>
#include <dlfcn.h>
#include <fcntl.h>
#include <stdlib.h>
#include <stdatomic.h>
#include <string.h>
#include <unistd.h>
#include <limits.h>

static void hold_directory(const char *path) {
    static atomic_int blocked;
    const char *target = getenv("FLEA_TEST_SIZE_PATH");
    char resolved[PATH_MAX];
    if (target && (strcmp(path, target) == 0 || (realpath(path, resolved) && strcmp(resolved, target) == 0)) && atomic_exchange(&blocked, 1) == 0) {
        int fd = open(getenv("FLEA_TEST_SIZE_ENTERED"), O_WRONLY | O_CREAT, 0600);
        if (fd >= 0) close(fd);
        while (access(getenv("FLEA_TEST_SIZE_RELEASE"), F_OK) != 0) usleep(1000);
    }
}

DIR *opendir(const char *path) {
    DIR *(*real_opendir)(const char *) = dlsym(RTLD_NEXT, "opendir");
    hold_directory(path);
    return real_opendir(path);
}

#include <sys/stat.h>
#include <stdio.h>
#include <stdarg.h>

int open64(const char *path, int flags, ...) {
    int (*real_open)(const char *, int, ...) = dlsym(RTLD_NEXT, "open64");
    mode_t mode = 0;
    if (flags & O_CREAT) {
        va_list args;
        va_start(args, flags);
        mode = (mode_t)va_arg(args, int);
        va_end(args);
    }
    const char *mounts = getenv("FLEA_REVIEW_MOUNTS");
    if (mounts && strcmp(path, "/proc/self/mountinfo") == 0) path = mounts;
    if (flags & O_DIRECTORY) hold_directory(path);
    return real_open(path, flags, mode);
}

int statx(int fd, const char *path, int flags, unsigned int mask, struct statx *out) {
    int (*real_statx)(int, const char *, int, unsigned int, struct statx *) = dlsym(RTLD_NEXT, "statx");
    static atomic_int held;
    const char *target = getenv("FLEA_REVIEW_STAT");
    const char *arm = getenv("FLEA_REVIEW_ARM");
    int armed = target && arm && strcmp(path, target) == 0 && access(arm, F_OK) == 0;
    const char *stats = getenv("FLEA_REVIEW_STATS");
    if (armed && stats) {
        int trace = open(stats, O_WRONLY | O_CREAT | O_APPEND, 0600);
        if (trace >= 0) { write(trace, "s", 1); close(trace); }
    }
    if (armed && atomic_exchange(&held, 1) == 0) {
        int marker = open(getenv("FLEA_REVIEW_ENTERED"), O_WRONLY | O_CREAT, 0600);
        if (marker >= 0) close(marker);
        char release[4096];
        snprintf(release, sizeof(release), "%s.stat", getenv("FLEA_REVIEW_RELEASE"));
        while (access(release, F_OK) != 0) usleep(1000);
    }
    return real_statx(fd, path, flags, mask, out);
}
