#define _GNU_SOURCE
#include <fcntl.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <time.h>
#include <unistd.h>

static const int wait_seconds = 30;
static const long poll_nanoseconds = 10000000;
static _Thread_local int held;

static void hold_copy(int fd) {
    const char *root = getenv("FLEA_JOBS_BARRIER");
    char path[PATH_MAX], source[PATH_MAX], marker[PATH_MAX];
    struct stat metadata;
    if (held || !root || root[0] != '/' || fstat(fd, &metadata) || !S_ISREG(metadata.st_mode)) return;
    snprintf(path, sizeof path, "/proc/self/fd/%d", fd);
    ssize_t size = readlink(path, source, sizeof source - 1);
    if (size < 0) return;
    source[size] = '\0';
    size_t root_size = strlen(root);
    const char *name = strrchr(source, '/');
    if (strncmp(source, root, root_size) || source[root_size] != '/' || !name || strcmp(name, "/hold-copy.txt")) return;
    snprintf(marker, sizeof marker, "%s/.flea-test-sandbox", root);
    if (access(marker, F_OK)) return;
    held = 1;
    long thread = syscall(SYS_gettid);
    snprintf(path, sizeof path, "%s/barriers/%ld.ready", root, thread);
    int ready = open(path, O_WRONLY | O_CREAT | O_EXCL, 0600);
    if (ready < 0) return;
    close(ready);
    char release[PATH_MAX], all[PATH_MAX];
    snprintf(release, sizeof release, "%s/barriers/%ld.release", root, thread);
    snprintf(all, sizeof all, "%s/barriers/release-all", root);
    struct timespec start, now, poll = { .tv_sec = 0, .tv_nsec = poll_nanoseconds };
    clock_gettime(CLOCK_MONOTONIC, &start);
    while (access(release, F_OK) && access(all, F_OK)) {
        clock_gettime(CLOCK_MONOTONIC, &now);
        if (now.tv_sec - start.tv_sec >= wait_seconds) break;
        nanosleep(&poll, NULL);
    }
}

ssize_t read(int fd, void *buffer, size_t count) {
    hold_copy(fd);
    return syscall(SYS_read, fd, buffer, count);
}
