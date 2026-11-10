//! Network mount aliases follow persisted bookmark and favourite labels, never live provider labels.
use super::crumbs::{relabel_aliases, remember_alias};
use crate::state::Shell;
use flea::{jsondoc::Json, ui_logic_device_rail::mounts, ui_logic_network_shares::smb};
use std::collections::HashMap;

pub(crate) fn saved_labels(bookmarks: &str) -> HashMap<String, String> {
    mounts::non_file_bookmarks(bookmarks)
        .into_iter()
        .map(|place| (mounts::normalize(&place.uri), place.label))
        .collect()
}

impl Shell {
    fn alias_labels(&self, state: &Json) -> HashMap<String, String> {
        let mut labels = self.chrome.saved_labels.clone();
        if let Some(favourites) = state
            .get("places")
            .and_then(|places| places.get("favourites"))
            .and_then(Json::as_array)
        {
            for favourite in favourites {
                if let (Some(path), Some(label)) = (
                    favourite.get("path").and_then(Json::as_str),
                    favourite.get("label").and_then(Json::as_str),
                ) {
                    if !path.is_empty() && !label.is_empty() {
                        labels.insert(mounts::normalize(path), label.into());
                    }
                }
            }
        }
        labels
    }

    pub(crate) fn relabel_path_aliases(&mut self, state: &Json) {
        self.chrome.aliases = relabel_aliases(&self.chrome.aliases, &self.alias_labels(state));
    }

    pub(crate) fn path_saved_labels(&mut self, labels: HashMap<String, String>) -> bool {
        self.chrome.saved_labels = labels;
        let next = relabel_aliases(
            &self.chrome.aliases,
            &self.alias_labels(&self.launch.settings.data),
        );
        if next == self.chrome.aliases {
            return false;
        }
        self.chrome.aliases = next;
        true
    }

    /// A launch path inside a mounted share names the share's root as a rail click does; a typed share stays bare.
    pub(crate) fn alias_launched_shares(
        &mut self,
        rows: &[flea::ui_logic_device_rail::RailEntry],
    ) -> bool {
        let before = self.chrome.aliases.clone();
        for path in self.launch.paths.clone() {
            // A gvfs share row carries only its uri; its root is the launch path's share folder.
            let gvfs = gvfs_share_root(&path);
            for row in rows.iter().filter(|row| row.mounted) {
                let root = match gvfs {
                    _ if !row.path.is_empty() => super::crumbs::clean_root(&row.path),
                    Some(root) if gvfs_folder_is(root, &row.uri) => root,
                    _ => continue,
                };
                let named = super::crumbs::alias_at(root, &self.chrome.aliases)
                    .is_some_and(|alias| alias.path == root);
                if mounts::inside(&path, root) && !named {
                    self.remember_path_alias(root, &row.label, &row.uri);
                }
            }
        }
        before != self.chrome.aliases
    }

    pub(crate) fn remember_path_alias(&mut self, path: &str, label: &str, uri: &str) {
        let key = mounts::normalize(uri);
        let labels = self.alias_labels(&self.launch.settings.data);
        let label = labels.get(&key).map_or(label, String::as_str);
        self.chrome.aliases = remember_alias(&self.chrome.aliases, path, label, Some(&key));
    }
}

// Sample input: "/run/user/1000/gvfs/dav:host=h,port=8080,ssl=false/photos" answers the path up to the share folder.
fn gvfs_share_root(path: &str) -> Option<&str> {
    if !mounts::under_gvfs(path) {
        return None;
    }
    let start = path.find("/gvfs/")? + "/gvfs/".len();
    Some(
        path[start..]
            .find('/')
            .map_or(path, |slash| &path[..start + slash]),
    )
}

// Sample input: ".../gvfs/dav:host=h,port=8080,ssl=false,prefix=%2Fdav" names the share "dav://h:8080/dav"; ssl=true is davs.
fn gvfs_folder_is(root: &str, uri: &str) -> bool {
    let folder = root.rsplit('/').next().unwrap_or_default();
    let Some((kind, keys)) = folder.split_once(':') else {
        return false;
    };
    // Sample input: ".../gvfs/smb-share:server=nas,share=media,user=dan" names "smb://dan@nas/media/" and "smb://nas/media/", never "smb://nas/media/photos/".
    if kind == "smb-share" {
        let below_share = uri
            .split_once("://")
            .and_then(|(_, rest)| rest.split_once('/'))
            .is_some_and(|(_, folders)| folders.trim_matches('/').contains('/'));
        return !below_share && !smb::resolve(uri, &[root.to_owned()], "").path.is_empty();
    }
    let value = |key: &str| {
        keys.split(',')
            .find_map(|pair| pair.strip_prefix(key)?.strip_prefix('='))
    };
    let Some(host) = value("host") else {
        return false;
    };
    let scheme = match (kind, value("ssl")) {
        ("dav", Some("true")) => "davs",
        (kind, _) => kind,
    };
    let user = value("user").map_or(String::new(), |user| format!("{user}@"));
    let port = value("port").map_or(String::new(), |port| format!(":{port}"));
    let named = format!(
        "{scheme}://{user}{host}{port}{}",
        value("prefix").unwrap_or("/")
    );
    let spelled = |uri: &str| mounts::normalize(&mounts::decode_path(uri));
    spelled(&named) == spelled(uri)
}

#[cfg(test)]
mod tests {
    use super::{gvfs_folder_is, gvfs_share_root};

    #[test]
    fn s8_14_a_gvfs_share_folder_matches_only_its_own_uri() {
        let folder = "dav:host=127.0.0.1,port=18767,ssl=false";
        assert!(gvfs_folder_is(folder, "dav://127.0.0.1:18767/"));
        assert!(!gvfs_folder_is(folder, "dav://127.0.0.1:9/"));
        assert!(!gvfs_folder_is(folder, "davs://127.0.0.1:18767/"));
        assert!(gvfs_folder_is("dav:host=h,ssl=true", "davs://h/"));
        assert!(gvfs_folder_is("sftp:host=h,user=me", "sftp://me@h:22/"));
        assert!(!gvfs_folder_is("sftp:host=h,user=me", "sftp://h/"));
        let b = "dav:host=h,ssl=false,prefix=%2Fremote.php%2Fdav%2Ffiles%2Fb";
        assert!(gvfs_folder_is(b, "dav://h/remote.php/dav/files/b/"));
        assert!(!gvfs_folder_is(b, "dav://h/remote.php/dav/files/a/"));
        assert!(!gvfs_folder_is(b, "dav://h/"));
        let share = "smb-share:server=nas,share=media";
        assert!(gvfs_folder_is(share, "smb://nas/media/"));
        assert!(!gvfs_folder_is(share, "smb://nas/other/"));
        assert!(!gvfs_folder_is(share, "smb://dan@nas/media/"));
        assert!(!gvfs_folder_is(share, "sftp://nas/media/"));
        assert!(gvfs_folder_is(share, "smb://nas/media"));
        assert!(!gvfs_folder_is(share, "smb://nas/media/photos/"));
        assert!(!gvfs_folder_is(share, "smb://nas/media/media"));
        let dan = "smb-share:server=nas,share=media,user=dan";
        assert!(gvfs_folder_is(dan, "smb://dan@nas/media/"));
        assert!(!gvfs_folder_is(dan, "smb://dan@nas/media/photos/"));
        assert!(gvfs_folder_is(dan, "smb://nas/media/"));
        assert!(!gvfs_folder_is(dan, "smb://eve@nas/media/"));
        assert_eq!(
            gvfs_share_root(&format!("/run/user/1000/gvfs/{folder}/photos")),
            Some(format!("/run/user/1000/gvfs/{folder}").as_str())
        );
        assert_eq!(gvfs_share_root("/home/me/gvfs/dav:host=h/a"), None);
    }
}
