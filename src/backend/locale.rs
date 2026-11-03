use std::ffi::OsStr;

// Parsed commands and all jailed children need untranslated UTF-8 text and dot decimals; Flea translates none of their text.
pub(crate) fn command(program: impl AsRef<OsStr>) -> std::process::Command {
    let mut child = std::process::Command::new(program);
    child.env("LC_ALL", "C.UTF-8").env_remove("LANGUAGE");
    child
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    #[test]
    fn parsed_children_override_the_inherited_locale() {
        for locale in ["de_DE.UTF-8", "ar_AE.UTF-8"] {
            let output = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "backend::locale::tests::locale_child", "--nocapture"])
                .env("FLEA_LOCALE_TEST_CHILD", "1")
                .env("LC_ALL", locale).env("LANG", locale).env("LANGUAGE", "de:ar")
                .output().expect("the isolated locale test starts");
            let stdout = String::from_utf8(output.stdout).unwrap();
            assert!(output.status.success(), "{stdout}");
            assert!(stdout.lines().any(|line| line.starts_with("test result: ok. 1 passed; 0 failed;")), "{stdout}");
            assert!(stdout.contains("locale child body completed"), "{stdout}");
        }
    }

    #[test]
    fn locale_child() {
        if std::env::var_os("FLEA_LOCALE_TEST_CHILD").is_none() { return; }
        let probe = "printf '%s|%s|%s' \"$LC_ALL\" \"${LANGUAGE-unset}\" \"$LANG\"";
        let locale = std::env::var("LC_ALL").unwrap();
        let output = command("/bin/sh").args(["-c", probe]).output().unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8(output.stdout).unwrap(), format!("C.UTF-8|unset|{locale}"));
        let mut opened = Command::new("/bin/sh");
        opened.args(["-c", probe]);
        crate::terminal::detach(&mut opened);
        let output = opened.stdout(Stdio::piped()).output().unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8(output.stdout).unwrap(), format!("{locale}|de:ar|{locale}"));
        assert_eq!(std::env::var("LC_ALL").unwrap(), locale);
        assert_eq!(std::env::var("LANGUAGE").unwrap(), "de:ar");
        println!("locale child body completed");
    }
}
