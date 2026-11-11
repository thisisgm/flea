//! The rail's local source paths, read together on its existing mount poll.
use std::io::Read as _;
use std::path::PathBuf;

#[derive(Clone)]
pub(crate) struct Sources {
    pub home: PathBuf,
    pub mountinfo: PathBuf,
    pub dropbox: PathBuf,
}

impl Default for Sources {
    fn default() -> Self {
        let home = if cfg!(test) {
            PathBuf::new()
        } else {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
        };
        Self {
            mountinfo: if cfg!(test) {
                PathBuf::new()
            } else {
                "/proc/self/mountinfo".into()
            },
            dropbox: home.join(".dropbox/info.json"),
            home,
        }
    }
}

impl Sources {
    pub fn read_mountinfo(&self) -> String {
        #[cfg(test)]
        if self.mountinfo.as_os_str().is_empty() {
            // Keep readiness real while unit fixtures supply their own network mounts.
            return "1 0 8:1 / / rw - ext4 /dev/root rw\n".into();
        }
        std::fs::read_to_string(&self.mountinfo).unwrap_or_default()
    }

    pub fn load_rows(
        &self,
        gio: &str,
        marks: &str,
        info: &str,
    ) -> Vec<flea::ui_logic_device_rail::RailEntry> {
        let mut rows = flea::ui_logic_network_shares::rail::rebuild(
            gio,
            &flea::ui_logic_device_rail::mounts::non_file_bookmarks(marks),
            info,
        );
        let dropbox = if self.home.as_os_str().is_empty() {
            String::new()
        } else {
            flea::backend::regfile::open_if_regular(&self.dropbox, 0)
                .ok()
                .and_then(|file| {
                    let mut text = String::new();
                    file.take(65_537).read_to_string(&mut text).ok()?;
                    (text.len() <= 65_536).then_some(text)
                })
                .map(|text| flea::ui_logic_send_share::dropbox::account(&text, "").path)
                .unwrap_or_default()
        };
        let local = flea::ui_logic_network_shares::cloud::rows(
            info,
            &self.home.to_string_lossy(),
            &dropbox,
        );
        let at = rows
            .iter()
            .position(|row| !row.path.is_empty())
            .unwrap_or(rows.len());
        rows.splice(at..at, local);
        rows
    }
}
