use std::process::Command;

// The hand-off ui/TabBar.qml gives the one window a tear-off starts; nothing else Flea starts may inherit it.
pub const ENV: [&str; 3] = ["FLEA_TAB_SOURCE_PID", "FLEA_TAB_CURSOR", "FLEA_TAB_TOKEN"];

// A terminal or an opened program that kept these would replay a finished lift into its own Flea.
pub fn drop_env(command: &mut Command) {
    command.env_remove("FLEA_TAB_SORT");
    for name in ENV {
        command.env_remove(name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Sample input: `"FLEA_TAB_TOKEN=" + root.outToken, ...` gives FLEA_TAB_TOKEN; only lines holding `on` are read.
    fn hand_off_names<'a>(source: &'a str, on: &str) -> Vec<&'a str> {
        let quoted = source
            .lines()
            .filter(|line| line.contains(on))
            .flat_map(|line| line.split('"'));
        quoted
            .filter(|word| word.starts_with("FLEA_TAB_"))
            .map(|word| word.trim_end_matches('='))
            .collect()
    }

    #[test]
    fn the_hand_off_names_exactly_what_the_tab_bar_sets() {
        assert_eq!(
            hand_off_names(include_str!("../ui/TabBar.qml"), "\"FLEA_TAB_"),
            ENV
        );
    }

    #[test]
    fn a_fresh_qml_launch_drops_the_same_names() {
        assert_eq!(
            hand_off_names(include_str!("../ui/js/Tabs.js"), "var TEAR_OFF_ENV"),
            ENV
        );
    }

    #[test]
    fn a_name_in_one_list_only_is_told_apart() {
        let grown =
            "execDetached([\"env\", \"FLEA_TAB_SOURCE_PID=\" + a, \"FLEA_TAB_EXTRA=\" + b])";
        assert_eq!(
            hand_off_names(grown, "\"FLEA_TAB_"),
            ["FLEA_TAB_SOURCE_PID", "FLEA_TAB_EXTRA"]
        );
        assert_ne!(hand_off_names(grown, "\"FLEA_TAB_"), ENV);
    }

    #[test]
    fn a_child_never_sees_a_variable_the_caller_set_for_it() {
        let mut child = Command::new("sh");
        child.args(["-c", "printf '%s' \"${FLEA_TAB_SOURCE_PID-}${FLEA_TAB_CURSOR-}${FLEA_TAB_TOKEN-}${FLEA_TAB_SORT-}\""]);
        for name in ENV {
            child.env(name, "stale");
        }
        child.env("FLEA_TAB_SORT", "stale");
        drop_env(&mut child);
        let out = child.output().unwrap();
        assert!(out.status.success());
        assert_eq!(String::from_utf8_lossy(&out.stdout), "");
    }

    #[test]
    fn an_unrelated_variable_survives() {
        let mut child = Command::new("sh");
        child
            .args(["-c", "printf '%s' \"$FLEA_TAB_UNRELATED\""])
            .env("FLEA_TAB_UNRELATED", "kept");
        child.env("FLEA_TAB_SORT", "stale");
        drop_env(&mut child);
        assert_eq!(
            String::from_utf8_lossy(&child.output().unwrap().stdout),
            "kept"
        );
    }
}
