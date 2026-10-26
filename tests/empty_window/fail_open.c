#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <stdlib.h>
#include <string.h>

int eventfd(unsigned int value, int flags) {
    const char *fail = getenv("FLEA_TEST_FAIL_OPEN");
    if (fail && !strcmp(fail, "eventfd")) { errno = EMFILE; return -1; }
    int (*next)(unsigned int, int) = dlsym(RTLD_NEXT, "eventfd");
    return next(value, flags);
}

int timerfd_create(int clock, int flags) {
    const char *fail = getenv("FLEA_TEST_FAIL_OPEN");
    if (fail && !strcmp(fail, "timerfd")) { errno = EMFILE; return -1; }
    int (*next)(int, int) = dlsym(RTLD_NEXT, "timerfd_create");
    return next(clock, flags);
}
