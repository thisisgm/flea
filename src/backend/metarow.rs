#[derive(Clone)]
pub struct Meta {
    pub size: u64,
    pub mtime: i64,
    pub mode: u32,
    pub target_is_dir: bool,
    // Where a symlink points, verbatim and unresolved, empty for every other kind of row; see
    // docs/protocol.md "rows". The link's own bytes, so a relative target stays relative.
    pub target: String,
    // The filesystem this row lives on, so a drop can tell a move within one volume from a copy
    // across two the way Finder does. Free here: the stat that fills the fields above already read it.
    pub dev: u64,
}
