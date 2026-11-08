//! Provider enable-state gating, one menu row per provider; the ids are pinned to st2 and mn1 by tests.
use crate::jsondoc::{self, Json};

// The st2 built-in ids carrying send targets, as st2 builtin.rs writes them.
pub const PLUGIN_TAILSCALE: &str = "flea.tailscale";
pub const PLUGIN_LOCALSEND: &str = "flea.localsend";
pub const PLUGIN_DROPBOX: &str = "flea.dropbox";
pub const PLUGIN_AIRDROP: &str = "flea.airdrop";

// The menu rows this unit gates, as mn1 inventory.rs spells their stored ids.
pub const MENU_TAILDROP: &str = "taildrop";
pub const MENU_LOCALSEND: &str = "localsend";
pub const MENU_DROPBOX: &str = "dropbox";
pub const MENU_SHARELINK: &str = "sharelink";
pub const MENU_AIRDROP: &str = "airdrop";

// Which provider a menu row or peer list asks about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Tailscale,
    LocalSend,
    Dropbox,
    AirDrop,
}

impl Role {
    pub fn plugin_id(self) -> &'static str {
        match self {
            Self::Tailscale => PLUGIN_TAILSCALE,
            Self::LocalSend => PLUGIN_LOCALSEND,
            Self::Dropbox => PLUGIN_DROPBOX,
            Self::AirDrop => PLUGIN_AIRDROP,
        }
    }

    pub fn menu_id(self) -> &'static str {
        match self {
            Self::Tailscale => MENU_TAILDROP,
            Self::LocalSend => MENU_LOCALSEND,
            Self::Dropbox => MENU_DROPBOX,
            Self::AirDrop => MENU_AIRDROP,
        }
    }
}

// One provider: installed from the facts, enabled and failed from st2; ShareLink rides the Dropbox one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Provider {
    pub plugin: String,
    pub installed: bool,
    pub enabled: bool,
    pub failed: bool,
}

impl Provider {
    // The st2 contributing rule: enabled and started; any other draws nothing.
    pub fn contributing(&self) -> bool {
        self.enabled && !self.failed
    }

    // The row shows only while the binary is there, its plugin is on and Settings leaves it on.
    pub fn row_on(&self, hidden: &[String], menu_id: &str) -> bool {
        self.installed && self.contributing() && !hidden.iter().any(|id| id == menu_id)
    }
}

pub fn provider(providers: &[Provider], role: Role) -> Option<&Provider> {
    providers.iter().find(|p| p.plugin == role.plugin_id())
}

// The backend's providers facts as one GPUI snapshot; malformed input never panics, it errs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Facts {
    pub tailscale_installed: bool,
    pub localsend_installed: bool,
    pub dropbox_installed: bool,
    pub tailscale_command: String,
    // Tailscale is on PATH but cannot run (not executable, a dangling link); v0.3.12 keeps its row, unavailable.
    pub tailscale_unusable: bool,
    pub send_command: String,
    pub localsend_command: String,
    // The provider's own reason when the CLI is missing; the caller words the sentence when it is empty.
    pub localsend_reason: String,
    pub dropbox_command: String,
    pub dropbox_ready: bool,
    // owly, the AirDrop stack the AirDrop plugin runs; empty while it is not on PATH.
    pub owly_command: String,
}

// Sample input: {"taildrop":{"installed":true,"command":"/usr/bin/tailscale","reason":""},"taildropSend":{...},"localsend":{"installed":false,"command":"","reason":"localsend-cli is not installed."},"rclone":{...},"dropbox":{...},"owly":{...},"dropboxInfo":"{...}","dropboxError":""}
pub fn parse_facts(raw: &str) -> Result<Facts, String> {
    let data = jsondoc::parse(raw).map_err(|problem| format!("providers facts do not parse: {problem}"))?;
    let command = |key: &str| {
        data.get(key)
            .and_then(|facts| facts.get("command"))
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string()
    };
    Ok(Facts {
        tailscale_installed: data.get("taildrop").and_then(|facts| facts.get("installed")).and_then(Json::as_bool) == Some(true),
        localsend_installed: data.get("localsend").and_then(|facts| facts.get("installed")).and_then(Json::as_bool) == Some(true),
        dropbox_installed: data.get("dropbox").and_then(|facts| facts.get("installed")).and_then(Json::as_bool) == Some(true),
        tailscale_command: command("taildrop"),
        tailscale_unusable: data
            .get("taildrop")
            .and_then(|facts| facts.get("installed"))
            .and_then(Json::as_bool)
            == Some(true)
            && command("taildrop").is_empty(),
        send_command: command("taildropSend"),
        localsend_command: command("localsend"),
        localsend_reason: data
            .get("localsend")
            .and_then(|facts| facts.get("reason"))
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string(),
        dropbox_command: command("dropbox"),
        dropbox_ready: !data
            .get("dropboxInfo")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .is_empty(),
        owly_command: command("owly"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on(plugin: &str) -> Provider {
        Provider { plugin: plugin.into(), installed: true, enabled: true, failed: false }
    }

    #[test]
    fn only_an_installed_enabled_healthy_provider_keeps_its_row() {
        let providers = vec![on(PLUGIN_TAILSCALE)];
        let host = provider(&providers, Role::Tailscale).expect("tailscale");
        assert!(host.row_on(&[], Role::Tailscale.menu_id()));
        assert!(!host.row_on(&[MENU_TAILDROP.to_string()], Role::Tailscale.menu_id()));
        let off = Provider { enabled: false, ..on(PLUGIN_TAILSCALE) };
        assert!(!off.row_on(&[], Role::Tailscale.menu_id()));
        let failed = Provider { failed: true, ..on(PLUGIN_TAILSCALE) };
        assert!(!failed.row_on(&[], Role::Tailscale.menu_id()));
        let missing = Provider { installed: false, ..on(PLUGIN_TAILSCALE) };
        assert!(!missing.row_on(&[], Role::Tailscale.menu_id()));
        assert!(provider(&providers, Role::Dropbox).is_none());
    }

    #[test]
    fn roles_name_their_plugin_and_menu_ids() {
        assert_eq!(Role::Tailscale.plugin_id(), "flea.tailscale");
        assert_eq!(Role::LocalSend.plugin_id(), "flea.localsend");
        assert_eq!(Role::Dropbox.plugin_id(), "flea.dropbox");
        assert_eq!(Role::Tailscale.menu_id(), "taildrop");
        assert_eq!(Role::LocalSend.menu_id(), "localsend");
        assert_eq!(Role::Dropbox.menu_id(), "dropbox");
        assert_eq!(Role::AirDrop.plugin_id(), "flea.airdrop");
        assert_eq!(Role::AirDrop.menu_id(), "airdrop");
    }

    #[test]
    fn menu_ids_match_mn1_inventory_and_plugin_ids_match_st2_builtins() {
        for (menu, plugin) in [(MENU_TAILDROP, PLUGIN_TAILSCALE), (MENU_LOCALSEND, PLUGIN_LOCALSEND), (MENU_DROPBOX, PLUGIN_DROPBOX), (MENU_AIRDROP, PLUGIN_AIRDROP)] {
            assert!(
                crate::ui_logic_mn1::inventory::INVENTORY.iter().any(|spec| spec.id == menu),
                "mn1 lost the {menu} row this unit gates"
            );
            assert!(
                crate::ui_logic_st2::builtin::manifests().iter().any(|(manifest, _)| manifest.id == plugin),
                "st2 has no builtin manifest {plugin}"
            );
        }
        assert!(
            crate::ui_logic_mn1::inventory::INVENTORY.iter().any(|spec| spec.id == MENU_SHARELINK),
            "mn1 lost the sharelink row"
        );
    }

    #[test]
    fn facts_name_the_three_commands_and_malformed_input_errs() {
        let facts = parse_facts(
            r#"{"taildrop":{"installed":true,"command":"/usr/bin/tailscale","reason":""},
"taildropSend":{"installed":true,"command":"/usr/bin/omarchy-tailscale-send","reason":""},
"localsend":{"installed":true,"command":"/usr/bin/localsend-cli","reason":""},
"dropbox":{"installed":true,"command":"/usr/bin/dropbox-cli","reason":""},
"owly":{"installed":true,"command":"/usr/bin/owly","reason":""},
"dropboxInfo":"{\"personal\":{\"path\":\"/d\"}}","dropboxError":""}"#,
        )
        .expect("facts parse");
        assert_eq!(facts.tailscale_command, "/usr/bin/tailscale");
        assert_eq!(facts.send_command, "/usr/bin/omarchy-tailscale-send");
        assert_eq!(facts.localsend_command, "/usr/bin/localsend-cli");
        assert_eq!(facts.localsend_reason, "");
        assert_eq!(facts.dropbox_command, "/usr/bin/dropbox-cli");
        assert!(facts.dropbox_ready);
        assert_eq!(facts.owly_command, "/usr/bin/owly");
        assert!(parse_facts("not json").is_err());
        assert!(parse_facts("{}").expect("empty facts").tailscale_command.is_empty());
    }
}
