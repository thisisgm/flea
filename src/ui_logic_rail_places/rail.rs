//! One rail row's data, the shape every places section hands the rail host.
use crate::jsondoc::Json;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Group {
    #[default]
    Home,
    Recent,
    Trash,
    // A stored favourite record from ui.json.
    Favourite,
    // The older merged row, Home plus XDG dirs plus GTK bookmarks.
    Favorite,
    Tag,
    Network,
    Device,
    Plugin,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Kind {
    #[default]
    Home,
    Recent,
    Trash,
    Favourite,
    Favorite,
    Tag,
    Share,
    Dropbox,
    Cloud,
    Volume,
    Disk,
    Phone,
    PluginRow,
}

// Why a favourite cannot open: its record fails validation, or the backend's inspection named a diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FavouriteError {
    Record(RecordError),
    Status(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordError {
    InvalidRecord,
    LabelBlank,
    InvalidPath,
    NotAPlace,
}

impl RecordError {
    pub fn key(self) -> &'static str {
        match self {
            Self::InvalidRecord => "places.favorite.invalid_record",
            Self::LabelBlank => "places.favorite.label_blank",
            Self::InvalidPath => "places.favorite.invalid_path",
            Self::NotAPlace => "places.favorite.not_a_place",
        }
    }
}

// A plugin row's square: on, off or none, as the rail's existing mounted mark draws it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Square {
    #[default]
    None,
    On,
    Off,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RailEntry {
    pub group: Group,
    pub kind: Kind,
    // A label from data (a folder leaf, a record, a share name), or empty when label_key names it.
    pub label: String,
    // The catalog key a built-in row's label resolves through, so no English rides the data.
    pub label_key: &'static str,
    pub path: String,
    pub stored_path: String,
    pub uri: String,
    pub mount_uri: String,
    pub device: String,
    pub mounted: Option<bool>,
    pub error: Option<FavouriteError>,
    pub original: Option<Json>,
    pub favourite_index: Option<usize>,
    pub glyph: &'static str,
    pub tag: Option<&'static str>,
    pub square: Square,
    // The device unit's Eject.releasable verdict: the row's slot draws the eject mark instead of its square.
    pub releasable: bool,
    // A device's byte size from lsblk, which SidebarRow.qml prints when places.driveSize is on.
    pub size: Option<u64>,
    // The plugin section a row belongs to and the row's own id there, empty for built-in rows.
    pub section: String,
    pub id: String,
    // A plugin row's own mark.svg, drawn in place of its glyph; none for built-in rows.
    pub mark: Option<OwnedMark>,
}

// A plugin's mark.svg as read from its folder: the path keys the painter's cache, the bytes paint as an alpha mask.
#[derive(Clone, Debug, PartialEq)]
pub struct OwnedMark {
    pub path: String,
    pub svg: std::sync::Arc<[u8]>,
}

// Two rows name the same place when this matches, whatever their mount state or count.
#[derive(Clone, Debug, PartialEq)]
pub enum Identity {
    Favourite(Json),
    Place(Group, Kind, String),
}

impl RailEntry {
    // Record errors are permanent; an inspection status describes only the last availability check.
    pub fn invalid_record(&self) -> bool {
        matches!(self.error, Some(FavouriteError::Record(_)))
    }

    // The device node, uri or path that names a device, share or phone row, as Mounts.railKey reads it.
    pub fn rail_key(&self) -> &str {
        match (self.group, self.kind) {
            (Group::Device, Kind::Volume) => &self.device,
            (Group::Device, Kind::Phone) | (Group::Network, Kind::Share) => &self.uri,
            _ => "",
        }
    }

    pub fn identity(&self) -> Identity {
        if self.kind == Kind::Favourite {
            return Identity::Favourite(self.original.clone().unwrap_or(Json::Null));
        }
        if self.kind == Kind::PluginRow {
            return Identity::Place(
                self.group,
                self.kind,
                format!("{}\n{}", self.section, self.id),
            );
        }
        let key = [self.rail_key(), &self.device, &self.uri, &self.path]
            .into_iter()
            .find(|part| !part.is_empty())
            .unwrap_or("");
        Identity::Place(self.group, self.kind, key.to_owned())
    }
}
