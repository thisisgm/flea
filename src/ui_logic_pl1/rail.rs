//! Where a chosen rail menu row goes, resolved by the row's key rather than a stale position.
use super::menu::RailAction;
use super::mounts::row_by_key;
use super::RailEntry;

/// The service call a chosen row resolves to; positions index the list the row was found in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Route {
    FlashDrive(String),
    Eject(usize),
    Activate(usize),
    UnmountVolume(usize),
    OpenPhone(String),
    ReleasePhone(String),
    OpenShare(usize),
    UnmountShare(usize),
    EditPlace(usize),
    RenameShare(usize),
    ForgetShare(String),
    Nothing,
}

/// The rail menu's dispatch: a key that names no row any more does nothing.
pub fn route(action: RailAction, key: &str, devices: &[RailEntry], network: &[RailEntry]) -> Route {
    let device = row_by_key(devices, key);
    let share = row_by_key(network, key);
    match action {
        RailAction::FlashImage => device.map_or(Route::Nothing, |at| Route::FlashDrive(devices[at].device.clone())),
        RailAction::Eject => device.map_or(Route::Nothing, Route::Eject),
        RailAction::UnmountVolume => device.map_or(Route::Nothing, Route::UnmountVolume),
        RailAction::MountVolume | RailAction::UnlockVolume => device.map_or(Route::Nothing, Route::Activate),
        RailAction::Open => match (device, share) {
            (Some(at), _) => Route::Activate(at),
            (None, Some(at)) => Route::OpenShare(at),
            (None, None) => Route::OpenPhone(key.to_string()),
        },
        RailAction::UnmountPhone => Route::ReleasePhone(key.to_string()),
        RailAction::MountPhone => Route::OpenPhone(key.to_string()),
        RailAction::Unmount => share.map_or(Route::Nothing, Route::UnmountShare),
        RailAction::EditPlace => share.map_or(Route::Nothing, Route::EditPlace),
        RailAction::Rename => share.map_or(Route::Nothing, Route::RenameShare),
        RailAction::Remove => share.map_or(Route::Nothing, |at| Route::ForgetShare(network[at].uri.clone())),
        RailAction::RemoveFavourite => Route::Nothing,
    }
}

/// An action by its menu id, None for an id the rail does not know.
pub fn action_by_id(id: &str) -> Option<RailAction> {
    super::menu::INVENTORY.into_iter().find(|a| a.id() == id)
}
