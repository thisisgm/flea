#define _GNU_SOURCE
#include <dlfcn.h>
#include <fcntl.h>
#include <stdlib.h>
#include <unistd.h>

int renameat2(int oldfd, const char *old, int newfd, const char *name, unsigned flags) {
    static int (*real_rename)(int, const char *, int, const char *, unsigned);
    if (!real_rename) real_rename = dlsym(RTLD_NEXT, "renameat2");
    const char *entered = getenv("FLEA_RENAME_ENTERED");
    const char *release = getenv("FLEA_RENAME_RELEASE");
    if (entered && release && access(release, F_OK) != 0) {
        int fd = open(entered, O_WRONLY | O_CREAT | O_EXCL, 0600);
        if (fd >= 0) close(fd);
        while (access(release, F_OK) != 0) usleep(1000);
    }
    return real_rename(oldfd, old, newfd, name, flags);
}
