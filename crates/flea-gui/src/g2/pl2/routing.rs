//! The path bar and rail use the same opener; URI detection happens before local path resolution.
use super::mount::Request;
use crate::g2::pl1::secret::Secret;
use flea::ui_logic_pl2::networkaddress;

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
    match networkaddress::parse(typed) {
        Ok(address) => AddressRoute::Network(Request {
            id: 0,
            uri: address.uri,
            label: address.host,
            origin,
            mounted: false,
            authenticated: false,
            password: Secret::new(),
        }),
        Err(_) => AddressRoute::Invalid("network.invalid_address"),
    }
}
