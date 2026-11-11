// Descriptor-only statx accepts a null path with AT_EMPTY_PATH on current kernels.
#define _GNU_SOURCE
#include <fcntl.h>
#include <stddef.h>
#include <sys/stat.h>
#include <unistd.h>

int main(void) {
    struct statx info;
    statx(STDIN_FILENO, NULL, AT_EMPTY_PATH, STATX_TYPE, &info);
    return 0;
}
