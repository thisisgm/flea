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
#include <stdarg.h>

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

int open64(const char *path, int flags, ...) {
    int (*real_open)(const char *, int, ...) = dlsym(RTLD_NEXT, "open64");
    mode_t mode = 0;
    if (flags & O_CREAT) {
        va_list args;
        va_start(args, flags);
        mode = (mode_t)va_arg(args, int);
        va_end(args);
    }
    if (flags & O_DIRECTORY) hold_directory(path);
    return real_open(path, flags, mode);
}
