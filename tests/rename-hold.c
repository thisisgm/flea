#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/file.h>
#include <unistd.h>

static void mark_entered(const char *path) {
    int fd = open(path, O_WRONLY | O_CREAT | O_EXCL, 0600);
    if (fd >= 0) close(fd);
}

int renameat2(int oldfd, const char *old, int newfd, const char *name, unsigned flags) {
    static int (*real_rename)(int, const char *, int, const char *, unsigned);
    if (!real_rename) real_rename = dlsym(RTLD_NEXT, "renameat2");
    const char *entered = getenv("FLEA_RENAME_ENTERED");
    const char *release = getenv("FLEA_RENAME_RELEASE");
    if (entered && release && access(release, F_OK) != 0) {
        mark_entered(entered);
        while (access(release, F_OK) != 0) usleep(1000);
    }
    return real_rename(oldfd, old, newfd, name, flags);
}

int flock(int fd, int operation) {
    int (*real_flock)(int, int) = dlsym(RTLD_NEXT, "flock");
    int result = real_flock(fd, operation);
    int saved_errno = errno;
    const char *expected = getenv("FLEA_JOURNAL_LOCK");
    const char *entered = getenv("FLEA_JOURNAL_ENTERED");
    if (result < 0 && saved_errno == EWOULDBLOCK && operation == (LOCK_EX | LOCK_NB) && expected && entered) {
        char link[PATH_MAX], path[PATH_MAX + 1];
        snprintf(link, sizeof(link), "/proc/self/fd/%d", fd);
        ssize_t length = readlink(link, path, PATH_MAX);
        if (length >= 0) {
            path[length] = '\0';
            if (strcmp(path, expected) == 0) mark_entered(entered);
        }
    }
    errno = saved_errno;
    return result;
}
