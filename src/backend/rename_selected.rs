use super::*;

// The st_mode file-type values lstat reports on Linux; any other kind in a source is malformed.
const S_IFREG: u32 = 0o100000;
const S_IFDIR: u32 = 0o040000;
const S_IFLNK: u32 = 0o120000;
const S_IFIFO: u32 = 0o010000;
const S_IFSOCK: u32 = 0o140000;
const S_IFCHR: u32 = 0o020000;
const S_IFBLK: u32 = 0o060000;

pub(crate) fn request(
    out: &mut impl Write,
    ops: &mut Ops,
    path: &str,
    to: &str,
    menu_id: usize,
    line: &str,
) -> Vec<PathBuf> {
    if menu_id != 0 {
        return do_menu_rename(out, ops, path, to, menu_id);
    }
    // Sample input: {"c":"rename","path":"/home/u/a.txt","to":"b.txt","source":"2049:1234567:32768"}
    let Ok(doc) = crate::jsondoc::parse(line) else {
        return from_source(out, ops, path, to, "");
    };
    match doc.get("source") {
        Some(source) => from_source(out, ops, path, to, source.as_str().unwrap_or_default()),
        None => do_rename(out, ops, path, to),
    }
}

// The row's lstat identity is captured before the editor holds its listing back.
// Sample input: "2049:1234567:32768" (device:inode:file-type bits, decimal).
pub(crate) fn from_source(
    out: &mut impl Write,
    ops: &mut Ops,
    path: &str,
    to: &str,
    source: &str,
) -> Vec<PathBuf> {
    let parts: Vec<_> = source.split(':').collect();
    let identity = (|| {
        let [dev, ino, kind] = parts.as_slice() else {
            return None;
        };
        let (dev, ino, kind) = (
            dev.parse().ok()?,
            ino.parse().ok()?,
            kind.parse::<u32>().ok()?,
        );
        if !flea::bytename::view(path).is_absolute()
            || !matches!(
                kind,
                S_IFREG | S_IFDIR | S_IFLNK | S_IFIFO | S_IFSOCK | S_IFCHR | S_IFBLK
            )
        {
            return None;
        }
        Some(ItemIdentity::from_parts(
            dev,
            ino,
            kind,
            0,
            (0, 0),
            (0, 0),
            None,
        ))
    })();
    match identity {
        Some(identity) => selected(
            out,
            ops,
            path,
            to,
            super::super::menu_actions::Selected::retry_item(flea::bytename::exact(path), identity),
        ),
        None => {
            writeln!(
                out,
                "{}",
                error_line(&op_err("rename", path, SELECTION_CHANGED))
            )
            .ok();
            out.flush().ok();
            Vec::new()
        }
    }
}

// A menu rename captures the selected identity before its worker outlives the menu and listing.
pub(crate) fn do_menu_rename(
    out: &mut impl Write,
    ops: &mut Ops,
    path: &str,
    to: &str,
    id: usize,
) -> Vec<PathBuf> {
    let captured = ops
        .menuactions
        .as_ref()
        .ok_or_else(|| super::super::menu_actions::SELECTION_CHANGED.to_string())
        .and_then(|menu| menu.selected_path(id, &flea::bytename::view(path)));
    match captured {
        Ok(item) => selected(out, ops, path, to, item),
        Err(error) => {
            writeln!(out, "{}", error_line(&op_err("rename", path, &error))).ok();
            out.flush().ok();
            Vec::new()
        }
    }
}

fn selected(
    out: &mut impl Write,
    ops: &mut Ops,
    path: &str,
    to: &str,
    item: super::super::menu_actions::Selected,
) -> Vec<PathBuf> {
    if ops.live.running().is_some() || ops.jobs.unfinished() > 0 {
        busy(out, "rename");
        return Vec::new();
    }
    let from = path.to_string();
    let name = to.to_string();
    do_rename_with(out, ops, path, CALL_DEADLINE, move || {
        item.current()
            .map_err(|error| op_err("rename", &from, &error))?;
        ops::rename(&flea::bytename::view(&from), &name)
    })
}

// Even a local rename can wait on the filesystem; the dispatcher only accepts its immutable paths.
pub(crate) fn do_rename(out: &mut impl Write, ops: &mut Ops, path: &str, to: &str) -> Vec<PathBuf> {
    if ops.live.running().is_some() || ops.jobs.unfinished() > 0 {
        busy(out, "rename");
        return Vec::new();
    }
    let from = path.to_string();
    let name = to.to_string();
    do_rename_with(out, ops, path, CALL_DEADLINE, move || {
        ops::rename(&flea::bytename::view(&from), &name)
    })
}
