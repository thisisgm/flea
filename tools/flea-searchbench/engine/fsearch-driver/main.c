/* SPDX-License-Identifier: GPL-2.0-only */
/* Bench-only driver, kept outside Flea's tracked source. */
#include "fsearch_database_include.h"
#include "fsearch_database_include_manager.h"
#include "fsearch_database_exclude_manager.h"
#include "fsearch_database_index_store.h"
#include "fsearch_database_search_view.h"
#include "fsearch_query.h"
#include <gio/gio.h>
#include <locale.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void json_string(const char *s) {
    putchar('"');
    for (const unsigned char *p = (const unsigned char *)s; *p; p++) {
        if (*p == '"' || *p == '\\') {
            putchar('\\');
            putchar(*p);
        } else if (*p < 0x20) {
            printf("\\u%04x", *p);
        } else {
            putchar(*p);
        }
    }
    putchar('"');
}

static bool is_root(FsearchDatabaseEntry *entry, const char *scope) {
    if (!entry) return false;
    GString *path = db_entry_get_path_full(entry);
    bool result = strcmp(path->str, scope) == 0;
    g_string_free(path, TRUE);
    return result;
}

static bool query(FsearchDatabaseIndexStore *store, const char *scope, char **fields) {
    uint64_t gen = g_ascii_strtoull(fields[1], NULL, 10);
    uint32_t limit = (uint32_t)g_ascii_strtoull(fields[2], NULL, 10);
    uint32_t offset = (uint32_t)g_ascii_strtoull(fields[3], NULL, 10);
    FsearchQueryFlags flags = QUERY_FLAG_AUTO_SEARCH_IN_PATH;
    if (strcmp(fields[4], "regex") == 0) flags |= QUERY_FLAG_REGEX;
    int64_t at = g_get_monotonic_time();
    g_autoptr(FsearchQuery) q = fsearch_query_new(fields[5], NULL, NULL, flags, "findbench1");
    g_autoptr(GCancellable) cancel = g_cancellable_new();
    fsearch_database_index_store_lock(store);
    bool complete = fsearch_database_index_store_search(store, 1, q, DATABASE_INDEX_PROPERTY_NAME,
                                                       FSEARCH_SORT_DIRECTION_ASCENDING, cancel);
    if (!complete) {
        fsearch_database_index_store_unlock(store);
        return false;
    }
    double core_ms = (g_get_monotonic_time() - at) / 1000.0;
    FsearchDatabaseSearchView *view = fsearch_database_index_store_get_search_view(store, 1);
    g_autoptr(FsearchDatabaseSearchInfo) info = fsearch_database_search_view_get_info(view);
    uint32_t total = fsearch_database_search_info_get_num_entries(info);
    bool root_first = total && is_root(fsearch_database_search_view_get_entry_for_idx(view, 0), scope);
    uint32_t scoped_total = total - (root_first ? 1 : 0);
    GPtrArray *paths = g_ptr_array_new_with_free_func(g_free);
    uint32_t begin = offset + (root_first ? 1 : 0);
    for (uint32_t i = begin; i < total && paths->len < limit; i++) {
        FsearchDatabaseEntry *entry = fsearch_database_search_view_get_entry_for_idx(view, i);
        GString *path = db_entry_get_path_full(entry);
        size_t len = strlen(scope);
        if (strncmp(path->str, scope, len) != 0 || path->str[len] != '/') {
            g_string_free(path, TRUE);
            continue;
        }
        g_ptr_array_add(paths, g_strdup(path->str + len + 1));
        g_string_free(path, TRUE);
    }
    if (g_str_has_prefix(fields[4], "audit:")) {
        guint rank = 0;
        for (guint i = 0; i < paths->len; i++) {
            if (strcmp(g_ptr_array_index(paths, i), fields[4] + 6) == 0) { rank = i + 1; break; }
        }
        printf("{\"t\":\"rank\",\"gen\":%llu,\"rank\":%u,\"n\":%u,\"complete\":%s}\n",
               (unsigned long long)gen, rank, scoped_total, paths->len == scoped_total ? "true" : "false");
        g_ptr_array_unref(paths);
        fflush(stdout);
        fsearch_database_index_store_unlock(store);
        return true;
    }
    double comparable_core_ms = (g_get_monotonic_time() - at) / 1000.0;
    printf("{\"t\":\"result\",\"gen\":%llu,\"n\":%u,\"core_ms\":%.6f,\"comparable_core_ms\":%.6f,\"paths\":[",
           (unsigned long long)gen, scoped_total, core_ms, comparable_core_ms);
    for (guint i = 0; i < paths->len; i++) {
        if (i) putchar(',');
        json_string(g_ptr_array_index(paths, i));
    }
    puts("]}");
    g_ptr_array_unref(paths);
    fflush(stdout);
    fsearch_database_index_store_unlock(store);
    return true;
}

int main(int argc, char **argv) {
    if (argc != 3 || !g_path_is_absolute(argv[1])) {
        fputs("usage: fsearch-driver ABSOLUTE_SCOPE THREADS\n", stderr);
        return 2;
    }
    setlocale(LC_ALL, "C.UTF-8");
    uint32_t threads = (uint32_t)g_ascii_strtoull(argv[2], NULL, 10);
    if (threads != g_get_num_processors()) {
        fputs("GLib CPU count does not match affinity budget; reject unfair run\n", stderr);
        return 2;
    }
    printf("{\"t\":\"boot\",\"cpus\":%u,\"scan_threads\":1}\n", threads);
    fflush(stdout);
    g_autoptr(FsearchDatabaseIncludeManager) includes = fsearch_database_include_manager_new();
    g_autoptr(FsearchDatabaseExcludeManager) excludes = fsearch_database_exclude_manager_new();
    fsearch_database_exclude_manager_set_exclude_hidden(excludes, TRUE);
    g_autoptr(FsearchDatabaseInclude) include = fsearch_database_include_new(argv[1], TRUE, FALSE, TRUE, TRUE, 0);
    fsearch_database_include_manager_add(includes, include);
    g_autoptr(FsearchDatabaseIndexStore) store = NULL;
    char *line = NULL;
    size_t capacity = 0;
    int result = 0;
    while (getline(&line, &capacity, stdin) >= 0) {
        line[strcspn(line, "\r\n")] = '\0';
        char **fields = g_strsplit(line, "\t", 6);
        uint32_t count = g_strv_length(fields);
        if (strcmp(fields[0], "O") == 0 && !store) {
            int64_t at = g_get_monotonic_time();
            store = fsearch_database_index_store_new(includes, excludes, DATABASE_INDEX_PROPERTY_FLAG_DEFAULT, NULL, NULL);
            g_autoptr(GCancellable) cancel = g_cancellable_new();
            fsearch_database_index_store_start(store, cancel);
            if (!fsearch_database_index_store_is_running(store)) {
                g_strfreev(fields);
                result = 2;
                break;
            }
            fsearch_database_index_store_start_monitoring(store);
            printf("{\"t\":\"ready\",\"build_ms\":%.6f,\"n\":%u,\"files\":%u,\"watcher\":\"inotify\"}\n",
                   (g_get_monotonic_time() - at) / 1000.0,
                   fsearch_database_index_store_get_num_files(store) + fsearch_database_index_store_get_num_folders(store) - 1,
                   fsearch_database_index_store_get_num_files(store));
            fflush(stdout);
        } else if (strcmp(fields[0], "Q") == 0 && store && count == 6) {
            if (!query(store, argv[1], fields)) result = 2;
        } else if (strcmp(fields[0], "P") == 0) {
            puts("{\"t\":\"pong\"}");
            fflush(stdout);
        } else if (strcmp(fields[0], "X") == 0) {
            g_strfreev(fields);
            break;
        } else {
            fputs("invalid driver command\n", stderr);
            result = 2;
        }
        g_strfreev(fields);
        if (result) break;
    }
    free(line);
    return result;
}
