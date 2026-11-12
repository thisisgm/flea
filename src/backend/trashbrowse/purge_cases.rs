use super::run_case;

macro_rules! purge_case {
    ($name:ident, $case:literal) => {
        #[test]
        fn $name() {
            run_case($case, concat!(module_path!(), "::", stringify!($name)));
        }
    };
}
purge_case!(
    advloop_f5_refused_restore_keeps_its_owned_thumbnails,
    "blocked"
);
purge_case!(
    advloop_f5_restore_purges_only_its_owned_thumbnails,
    "restore"
);
purge_case!(
    advloop_f5_permanent_delete_purges_only_its_owned_thumbnails,
    "delete"
);
purge_case!(
    advloop_f5_empty_trash_purges_only_its_owned_thumbnails,
    "empty"
);
purge_case!(
    advloop_f7_multi_delete_reads_each_cache_directory_once,
    "batch-delete"
);
purge_case!(
    advloop_f7_failed_delete_keeps_its_thumbnails,
    "partial-delete"
);
purge_case!(
    advloop_f7_multi_restore_reads_each_cache_directory_once,
    "batch-restore"
);
purge_case!(
    advloop_f7_failed_restore_keeps_its_thumbnails,
    "partial-restore"
);
purge_case!(
    cancelled_empty_trash_purges_only_removed_items_once,
    "cancel-empty"
);
purge_case!(
    empty_trash_removes_what_the_listing_left_out_as_gio_does,
    "sweep"
);
