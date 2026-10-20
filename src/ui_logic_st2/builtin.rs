//! The four features that ship as plugins inside Flea's package, in the order and states 0.3.12 ships them.
use super::manifest::{self, Manifest};

pub const SHELF: &str = "flea.shelf";

// The Shelf ships off as shelf.enabled does; the other three ship on and still show their rows only where their app is.
pub const BUILTINS: [(&str, bool); 4] = [
    (r#"{"schemaVersion":1,"id":"flea.shelf","name":"Shelf","version":"0.4.0","author":"Flea","license":"MIT",
"description":"Collect files from anywhere into one pile on the bar.","kinds":["shelf"],
"shelf":{"omarchyPlugin":"io.github.thisisgm.flea-shelf","glyph":"shelf"},
"access":["shelf:pile","shelf:bar","shelf:captures"]}"#, false),
    (r#"{"schemaVersion":1,"id":"flea.localsend","name":"LocalSend","version":"0.4.0","author":"Flea","license":"MIT",
"description":"Send files to nearby devices with LocalSend.","kinds":["send-target"],
"sendTargets":{"label":"LocalSend","glyph":"localsend"},"access":["selection","network","exec:localsend"]}"#, true),
    (r#"{"schemaVersion":1,"id":"flea.tailscale","name":"Tailscale","version":"0.4.0","author":"Flea","license":"MIT",
"description":"Send files to your devices with Taildrop.","kinds":["send-target"],
"sendTargets":{"label":"Taildrop","glyph":"tailscale"},"access":["selection","network","exec:tailscale"]}"#, true),
    (r#"{"schemaVersion":1,"id":"flea.dropbox","name":"Dropbox","version":"0.4.0","author":"Flea","license":"MIT",
"description":"Move files into your Dropbox folder.","kinds":["send-target","place"],
"sendTargets":{"label":"Dropbox","glyph":"dropbox"},"place":{"label":"Dropbox","glyph":"dropbox"},
"access":["write:selection","folder"]}"#, true),
];

pub fn manifests() -> Vec<(Manifest, bool)> {
    BUILTINS
        .iter()
        .map(|(text, on)| {
            let mut found = manifest::parse(text, true).expect("a built-in manifest is valid");
            // A built-in comes with Flea and updates with it, so it carries Flea's own version.
            found.version = env!("CARGO_PKG_VERSION").into();
            (found, *on)
        })
        .collect()
}

pub fn is_builtin(id: &str) -> bool {
    id.starts_with(manifest::RESERVED_PREFIX)
}
