#define _GNU_SOURCE
#include <dlfcn.h>
#include <fcntl.h>
#include <pthread.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <unistd.h>

enum CounterSlot {
    MOUNTINFO_OPENS,
    MIME_OPENS,
    ICON_OPENS,
    ALIAS_OPENS,
    THUMBNAILER_OPENS,
    METADATA_CALLS,
    THREAD_STARTS,
    COUNTER_SLOT_COUNT
};

static uint64_t *counts;

__attribute__((constructor)) static void observe(void) {
    const char *path = getenv("FLEA_TEST_COUNTS");
    if (!path) return;
    int fd = syscall(SYS_openat, AT_FDCWD, path, O_RDWR, 0);
    if (fd < 0) _exit(86);
    const size_t bytes = COUNTER_SLOT_COUNT * sizeof(*counts);
    struct stat info;
    if (syscall(SYS_fstat, fd, &info) || info.st_size != (off_t)bytes) _exit(88);
    void *map = mmap(NULL, bytes, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    close(fd);
    if (map == MAP_FAILED) _exit(87);
    counts = map;
}

static void bump(unsigned i) {
    if (counts) __atomic_fetch_add(counts + i, 1, __ATOMIC_SEQ_CST);
}

static void opened(const char *path) {
    if (!path) return;
    if (!strcmp(path, "/proc/self/mountinfo")) bump(MOUNTINFO_OPENS);
    if (!strcmp(path, "/usr/share/mime/globs2")) bump(MIME_OPENS);
    if (!strcmp(path, "/usr/share/mime/generic-icons")) bump(ICON_OPENS);
    if (!strcmp(path, "/usr/share/mime/aliases")) bump(ALIAS_OPENS);
    if (strstr(path, ".thumbnailer")) bump(THUMBNAILER_OPENS);
}

#define OPEN(name) \
int name(const char *path, int flags, ...) { \
    mode_t mode = 0; \
    if ((flags & O_CREAT) || ((flags & O_TMPFILE) == O_TMPFILE)) { \
        va_list args; va_start(args, flags); mode = va_arg(args, int); va_end(args); \
    } \
    int (*next)(const char *, int, ...) = dlsym(RTLD_NEXT, #name); \
    opened(path); return next(path, flags, mode); \
}
OPEN(open)
OPEN(open64)

#define OPENAT(name) \
int name(int fd, const char *path, int flags, ...) { \
    mode_t mode = 0; \
    if ((flags & O_CREAT) || ((flags & O_TMPFILE) == O_TMPFILE)) { \
        va_list args; va_start(args, flags); mode = va_arg(args, int); va_end(args); \
    } \
    int (*next)(int, const char *, int, ...) = dlsym(RTLD_NEXT, #name); \
    opened(path); return next(fd, path, flags, mode); \
}
OPENAT(openat)
OPENAT(openat64)

#define STAT(name, type) \
int name(const char *path, type *out) { \
    int (*next)(const char *, type *) = dlsym(RTLD_NEXT, #name); \
    bump(METADATA_CALLS); return next(path, out); \
}
STAT(stat, struct stat)
STAT(lstat, struct stat)
STAT(stat64, struct stat64)
STAT(lstat64, struct stat64)

#define FSTAT(name, type) \
int name(int fd, type *out) { \
    int (*next)(int, type *) = dlsym(RTLD_NEXT, #name); \
    bump(METADATA_CALLS); return next(fd, out); \
}
FSTAT(fstat, struct stat)
FSTAT(fstat64, struct stat64)

#define STATAT(name, type) \
int name(int fd, const char *path, type *out, int flags) { \
    int (*next)(int, const char *, type *, int) = dlsym(RTLD_NEXT, #name); \
    bump(METADATA_CALLS); return next(fd, path, out, flags); \
}
STATAT(fstatat, struct stat)
STATAT(fstatat64, struct stat64)

int statx(int fd, const char *path, int flags, unsigned mask, struct statx *out) {
    int (*next)(int, const char *, int, unsigned, struct statx *) = dlsym(RTLD_NEXT, "statx");
    bump(METADATA_CALLS); return next(fd, path, flags, mask, out);
}

int pthread_create(pthread_t *thread, const pthread_attr_t *attr, void *(*entry)(void *), void *arg) {
    int (*next)(pthread_t *, const pthread_attr_t *, void *(*)(void *), void *) = dlsym(RTLD_NEXT, "pthread_create");
    bump(THREAD_STARTS); return next(thread, attr, entry, arg);
}
