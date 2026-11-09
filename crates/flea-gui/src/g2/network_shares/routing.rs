//! The path bar and rail use the same opener; URI detection happens before local path resolution.
use super::mount::Request;
use crate::g2::device_rail::secret::Secret;
use flea::ui_logic_network_shares::networkaddress;

pub enum AddressRoute {
    Local,
    Network(Request),
    Invalid(&'static str),
}
pub fn address(input: &str, origin: u64) -> AddressRoute {
    let typed = input.trim();
    let candidate = typed.contains("://")
        || (!typed.starts_with('/') && typed.contains('@') && typed.contains(':'));
    if !candidate {
        return AddressRoute::Local;
    }
    match networkaddress::parse(typed).or_else(|error| incomplete(typed).ok_or(error)) {
        // v0.3.12's onPathEntered opened a typed address with no label: its card suggests the share name and its bar keeps the mounted path.
        Ok(address) => AddressRoute::Network(Request {
            id: 0,
            uri: address.uri,
            label: String::new(),
            origin,
            mounted: false,
            authenticated: false,
            password: Secret::new(),
        }),
        Err(_) => AddressRoute::Invalid("network.invalid_address"),
    }
}

// A hostless typed location still attempts a connection; only the missing host relaxes saved-address validation.
pub(super) fn incomplete(input: &str) -> Option<networkaddress::Address> {
    let (scheme, path) = input
        .split_once("://")
        .or_else(|| input.strip_suffix(':').map(|scheme| (scheme, "")))?;
    if !path.is_empty() && !path.starts_with('/') {
        return None;
    }
    let mut address = flea::ui_logic_network_shares::form::Form::saved_address(&format!(
        "{scheme}://host.invalid{path}"
    ))
    .ok()?;
    address.host.clear();
    address.uri = format!("{}://{path}", address.scheme);
    Some(address)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smbpath_only_hostless_supported_addresses_relax_typed_validation() {
        for uri in [
            "smb://",
            "smb:///share",
            "sftp://",
            "ftp://",
            "davs://",
            "nfs://",
        ] {
            assert!(matches!(address(uri, 9), AddressRoute::Network(_)), "{uri}");
            assert!(
                networkaddress::parse(uri).is_err(),
                "saved addresses stay strict"
            );
            let parsed = incomplete(uri).unwrap();
            assert!(parsed.host.is_empty());
            let mut card = super::super::dialog::Dialog::default();
            card.open_location(uri, "", true);
            assert!(!card.opened, "an incomplete saved address cannot be edited");
            card.open_location(uri, "", false);
            assert!(card.opened && card.retry && !card.save_new);
            assert!(card.submit().is_empty(), "retry needs a host first");
            let mut request = match address(uri, 9) {
                AddressRoute::Network(request) => request,
                _ => unreachable!(),
            };
            request.id = 1;
            assert!(
                super::super::validate::refused(&request).is_some(),
                "saving still requires a host"
            );
        }
        for uri in [
            "https://",
            "smb://user:secret@host/share",
            "smb://host:0/",
            "smb://-option/",
            "smb:///%00",
            "smb:///%GG",
        ] {
            assert!(matches!(address(uri, 9), AddressRoute::Invalid(_)), "{uri}");
        }
        assert!(incomplete(&format!("smb:///{}", "a".repeat(8192))).is_none());
    }
}
