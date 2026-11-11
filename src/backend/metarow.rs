#[derive(Clone)]
pub struct Meta {
    pub ino: Option<u64>,
    pub size: u64,
    pub mtime: i64,
    pub birth: Option<i64>,
    pub mode: u32,
    pub target_is_dir: bool,
    // Where a symlink points, verbatim and unresolved, so a relative target stays relative; empty otherwise, see docs/protocol.md "rows".
    pub target: String,
    // The filesystem this row lives on, so a drop tells a move within one volume from a copy across two; free from the stat.
    pub dev: u64,
}
