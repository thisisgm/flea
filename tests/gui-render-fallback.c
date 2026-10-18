// Forward a real Mesa ICD except for an injected vkCreateDevice failure.
#include <dlfcn.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

typedef void (*Proc)(void);
typedef Proc (*GetProc)(void *, const char *);
static void *driver;

static void load_driver(void) {
    if (!driver) {
        driver = dlopen(REAL_ICD_PATH, RTLD_NOW | RTLD_LOCAL);
        if (!driver) {
            fputs("render-test: real Mesa ICD could not load\n", stderr);
            abort();
        }
    }
}

static int fail_device(void *physical, const void *info, const void *allocator, void **device) {
    (void)physical;
    (void)info;
    (void)allocator;
    (void)device;
    static const char message[] = "render-test: vkCreateDevice refused\n";
    (void)write(STDERR_FILENO, message, sizeof(message) - 1);
    return -3;
}

Proc vk_icdGetInstanceProcAddr(void *instance, const char *name) {
    load_driver();
    if (strcmp(name, "vkCreateDevice") == 0) {
        return (Proc)fail_device;
    }
    GetProc get = (GetProc)dlsym(driver, "vk_icdGetInstanceProcAddr");
    return get ? get(instance, name) : NULL;
}

Proc vk_icdGetPhysicalDeviceProcAddr(void *instance, const char *name) {
    load_driver();
    GetProc get = (GetProc)dlsym(driver, "vk_icdGetPhysicalDeviceProcAddr");
    return get ? get(instance, name) : vk_icdGetInstanceProcAddr(instance, name);
}

int vk_icdNegotiateLoaderICDInterfaceVersion(uint32_t *version) {
    load_driver();
    int (*negotiate)(uint32_t *) = (int (*)(uint32_t *))dlsym(driver, "vk_icdNegotiateLoaderICDInterfaceVersion");
    if (negotiate) {
        return negotiate(version);
    }
    if (*version > 4) {
        *version = 4;
    }
    return 0;
}
