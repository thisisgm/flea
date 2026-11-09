//! The features that ship as plugins inside Flea's package: 0.3.12's four in its order and states, then AirDrop.
use super::manifest::{self, Manifest};

pub const SHELF: &str = "flea.shelf";
pub const AIRDROP: &str = "flea.airdrop";

// The Shelf and AirDrop ship off; the other three ship on and still show their rows only where their app is.
pub const BUILTINS: [(&str, bool); 5] = [
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
    // AirDrop runs the owly the user installed (https://github.com/t4t5/owly, MIT, by Tristan, @t4t5).
    (r#"{"schemaVersion":1,"id":"flea.airdrop","name":"AirDrop","version":"0.4.0","author":"Flea","license":"MIT",
"description":"Share files with iPhones, iPads and Macs nearby.","kinds":["send-target"],
"sendTargets":{"label":"AirDrop","glyph":"airdrop"},"access":["airdrop:send","airdrop:wifi","airdrop:receive"],
"schema":[{"key":"folder","type":"string","label":"Save received files to","defaultValue":""}]}"#, false),
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
