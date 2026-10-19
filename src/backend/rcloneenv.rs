// Explicit sends own operation and filter options; configuration, credentials and backend options stay inherited.
use std::ffi::OsStr;
use std::process::Command;

// Rclone 1.75's global copy, sync, filter and reporting options are not remote settings.
const OPERATIONS: &str = "MODIFY_WINDOW CHECKSUM SIZE_ONLY IGNORE_TIMES IGNORE_EXISTING IGNORE_ERRORS
    DRY_RUN INTERACTIVE LINKS COPY_LINKS SKIP_LINKS SKIP_SPECIALS ONE_FILE_SYSTEM
    LOCAL_COPY_LINKS LOCAL_LINKS LOCAL_SKIP_LINKS LOCAL_SKIP_SPECIALS LOCAL_ONE_FILE_SYSTEM
    QUIET VERBOSE MAX_DELETE MAX_DELETE_SIZE TRACK_RENAMES
    TRACK_RENAMES_STRATEGY UPDATE MAX_DEPTH IGNORE_SIZE IGNORE_CHECKSUM IGNORE_CASE_SYNC FIX_CASE
    NO_TRAVERSE CHECK_FIRST NO_CHECK_DEST NO_UPDATE_MODTIME NO_UPDATE_DIR_MODTIME COMPARE_DEST
    COPY_DEST BACKUP_DIR SUFFIX SUFFIX_KEEP_EXTENSION FAST_LIST LIST_CUTOFF IMMUTABLE STATS_UNIT
    STATS_FILE_NAME_LENGTH STATS_FILE_NAME_LEN LOG_LEVEL STATS_LOG_LEVEL LOG_FILE LOG_FORMAT SYSLOG
    USE_SYSLOG SYSLOG_FACILITY DUMP STREAMING_UPLOAD_CUTOFF MAX_TRANSFER MAX_DURATION CUTOFF_MODE
    MAX_BACKLOG MAX_STATS_GROUPS STATS STATS_ONE_LINE STATS_ONE_LINE_DATE STATS_ONE_LINE_DATE_FORMAT
    PROGRESS PROGRESS_TERMINAL_TITLE MULTI_THREAD_CUTOFF MULTI_THREAD_STREAMS MULTI_THREAD_WRITE_BUFFER_SIZE
    MULTI_THREAD_CHUNK_SIZE USE_JSON_LOG ORDER_BY REFRESH_TIMES METADATA METADATA_MAPPER SERVER_SIDE_ACROSS_CONFIGS
    DEFAULT_TIME INPLACE PARTIAL_SUFFIX NAME_TRANSFORM DELETE_EXCLUDED EXCLUDE_IF_PRESENT FILES_FROM
    FILES_FROM_RAW FILES_FROM0 MIN_AGE MAX_AGE MIN_SIZE MAX_SIZE IGNORE_CASE HASH_FILTER FILTER FILTER_FROM
    EXCLUDE EXCLUDE_FROM INCLUDE INCLUDE_FROM METADATA_FILTER METADATA_FILTER_FROM METADATA_EXCLUDE
    METADATA_EXCLUDE_FROM METADATA_INCLUDE METADATA_INCLUDE_FROM CREATE_EMPTY_SRC_DIRS ONE_WAY DOWNLOAD
    SUM COMBINED ERROR MATCH DIFFER MISSING_ON_DST MISSING_ON_SRC";

fn operation(key: &OsStr) -> bool {
    key.to_str()
        .and_then(|key| key.strip_prefix("RCLONE_"))
        .is_some_and(|name| OPERATIONS.split_whitespace().any(|option| option == name))
}

pub(super) fn sanitize(command: &mut Command) {
    remove(command, std::env::vars_os().map(|(key, _)| key));
}

fn remove(command: &mut Command, keys: impl IntoIterator<Item = std::ffi::OsString>) {
    for key in keys {
        if operation(&key) {
            command.env_remove(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rclone_operation_environment_is_removed_without_touching_remote_configuration() {
        let mut command = Command::new("rclone");
        let operation_keys = [
            "RCLONE_EXCLUDE",
            "RCLONE_FILTER_FROM",
            "RCLONE_FILES_FROM_RAW",
            "RCLONE_MAX_DEPTH",
            "RCLONE_MIN_SIZE",
            "RCLONE_HASH_FILTER",
            "RCLONE_IGNORE_EXISTING",
            "RCLONE_NAME_TRANSFORM",
            "RCLONE_LOG_FILE",
            "RCLONE_DRY_RUN",
            "RCLONE_LOCAL_COPY_LINKS",
            "RCLONE_LOCAL_SKIP_LINKS",
            "RCLONE_SKIP_SPECIALS",
            "RCLONE_LOCAL_ONE_FILE_SYSTEM",
            "RCLONE_QUIET",
            "RCLONE_VERBOSE",
        ];
        let remote_keys = [
            "RCLONE_CONFIG",
            "RCLONE_CONFIG_PASS",
            "RCLONE_PASSWORD_COMMAND",
            "RCLONE_CONFIG_GDRIVE_TYPE",
            "RCLONE_CONFIG_GDRIVE_TOKEN",
            "RCLONE_S3_ACCESS_KEY_ID",
            "RCLONE_DRIVE_TOKEN",
            "RCLONE_LOCAL_ENCODING",
            "HTTPS_PROXY",
        ];
        remove(
            &mut command,
            operation_keys
                .iter()
                .chain(&remote_keys)
                .map(|key| std::ffi::OsString::from(*key)),
        );
        let removed: Vec<_> = command
            .get_envs()
            .map(|(key, value)| {
                assert!(value.is_none());
                key.to_str().unwrap()
            })
            .collect();
        for key in operation_keys {
            assert!(removed.contains(&key), "{key}");
        }
        for key in remote_keys {
            assert!(!removed.contains(&key), "{key}");
        }
    }
}
