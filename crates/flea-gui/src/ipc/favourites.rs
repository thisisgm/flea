//! Read-only Favorites save state from the settings host.
use super::Adapter;
use crate::window::settings::Host;
use flea::ipc::wire::Reply;

pub(crate) struct FavouritesAdapter<'a> {
    pub host: &'a Host,
}

impl Adapter for FavouritesAdapter<'_> {
    fn call(&self, function: &str, _arguments: &[String]) -> Option<Reply> {
        match function {
            "favouritesSaving" => Some(Reply::value(
                (!self.host.favourite_writes.is_empty()).to_string(),
            )),
            _ => None,
        }
    }
}
