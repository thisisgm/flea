// Test-only opendir barrier: prove navigation works while a size worker is inside a syscall.
#define _GNU_SOURCE
#include <dirent.h>
#include <dlfcn.h>
#include <fcntl.h>
#include <stdlib.h>
#include <stdatomic.h>
#include <string.h>
#include <unistd.h>

DIR *opendir(const char *path) {
    DIR *(*real_opendir)(const char *) = dlsym(RTLD_NEXT, "opendir");
    static atomic_int blocked;
    const char *target = getenv("FLEA_TEST_SIZE_PATH");
    if (target && strcmp(path, target) == 0 && atomic_exchange(&blocked, 1) == 0) {
        int fd = open(getenv("FLEA_TEST_SIZE_ENTERED"), O_WRONLY | O_CREAT, 0600);
        if (fd >= 0) close(fd);
        while (access(getenv("FLEA_TEST_SIZE_RELEASE"), F_OK) != 0) usleep(1000);
    }
    return real_opendir(path);
}
