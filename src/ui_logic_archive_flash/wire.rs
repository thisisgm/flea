//! The archive and flash wire: the request lines the cards send and the reply lines they read.
use crate::json::escape;
use crate::jsondoc::Json;

// Sample input: two paths to 7z with the menu id riding along.
pub fn compress_request(paths: &[&str], dest: &str, format: &str, menu_id: u64) -> String {
    let paths = paths.iter().map(|path| format!("\"{}\"", escape(path))).collect::<Vec<_>>().join(",");
    format!(
        "{{\"c\":\"archive\",\"op\":\"compress\",\"paths\":[{paths}],\"dest\":\"{}\",\"format\":\"{format}\",\"menuId\":{menu_id}}}",
        escape(dest)
    )
}

// Sample input: the secret rides this wire and leaves it only on 7z's stdin pipe.
pub fn compress_password_request(
    paths: &[&str],
    dest: &str,
    format: &str,
    password: &str,
    encrypt_names: bool,
    menu_id: u64,
) -> String {
    let paths = paths.iter().map(|path| format!("\"{}\"", escape(path))).collect::<Vec<_>>().join(",");
    let encrypt = if encrypt_names { "true" } else { "false" };
    format!(
        "{{\"c\":\"archive\",\"op\":\"compress\",\"paths\":[{paths}],\"dest\":\"{}\",\"format\":\"{format}\",\"password\":\"{}\",\"encryptNames\":{encrypt},\"menuId\":{menu_id}}}",
        escape(dest),
        escape(password)
    )
}

// Sample input: an extract sends its path and destination with no password.
pub fn extract_request(path: &str, dest: &str, menu_id: u64) -> String {
    format!(
        "{{\"c\":\"archive\",\"op\":\"extract\",\"path\":\"{}\",\"dest\":\"{}\",\"menuId\":{menu_id}}}",
        escape(path),
        escape(dest)
    )
}

// Sample input: the retry carries the password over the same destination the first attempt named.
pub fn extract_password_request(path: &str, dest: &str, password: &str, menu_id: u64) -> String {
    format!(
        "{{\"c\":\"archive\",\"op\":\"extract\",\"path\":\"{}\",\"dest\":\"{}\",\"password\":\"{}\",\"menuId\":{menu_id}}}",
        escape(path),
        escape(dest),
        escape(password)
    )
}

// Sample input: the image path alone asks what the card drew.
pub fn flash_image_request(path: &str) -> String {
    format!("{{\"c\":\"flashimage\",\"path\":\"{}\"}}", escape(path))
}

// Sample input: the image rides along so the targets answer for it.
pub fn flash_targets_request(image: &str) -> String {
    format!("{{\"c\":\"flashtargets\",\"image\":\"{}\"}}", escape(image))
}

// Sample input: the identity fields ride the flash request so the backend re-checks the drive it showed.
pub fn flash_request(id: u64, image: &str, dev: &str, seq: u64, serial: &str, bytes: u64) -> String {
    format!(
        "{{\"c\":\"flash\",\"id\":{id},\"image\":\"{}\",\"dev\":\"{}\",\"seq\":{seq},\"serial\":\"{}\",\"bytes\":{bytes}}}",
        escape(image),
        escape(dev),
        escape(serial)
    )
}

// Sample input: cancelling names the running flash alone.
pub fn flash_cancel_request(id: u64) -> String {
    format!("{{\"c\":\"flashcancel\",\"id\":{id}}}")
}

// Sample input: ejecting names the drive and its sequence.
pub fn flash_eject_request(dev: &str, seq: u64) -> String {
    format!("{{\"c\":\"flasheject\",\"dev\":\"{}\",\"seq\":{seq}}}", escape(dev))
}

fn text(reply: &Json, key: &str) -> String {
    reply.get(key).and_then(Json::as_str).unwrap_or("").to_string()
}

fn number(reply: &Json, key: &str) -> u64 {
    reply.get(key).and_then(Json::as_f64).map(|n| n as u64).unwrap_or(0)
}

fn flag(reply: &Json, key: &str) -> bool {
    reply.get(key).and_then(Json::as_bool) == Some(true)
}

/// The terminal line an extract's routing reads: its error code with the path and destination it names.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ArchiveDone {
    pub id: u64,
    pub ok: bool,
    pub err: String,
    pub path: String,
    pub dest: String,
}

// Sample input: {"t":"archivedone","id":7,"ok":false,"verified":true,"err":"encrypted","path":"/a/backup.7z","dest":"/a/backup"}.
pub fn parse_archivedone(reply: &Json) -> ArchiveDone {
    ArchiveDone {
        id: number(reply, "id"),
        ok: flag(reply, "ok"),
        err: text(reply, "err"),
        path: text(reply, "path"),
        dest: text(reply, "dest"),
    }
}

/// The card's image from a flashimage reply; None answers another file's probe, such as the pane's zip check.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FlashImage {
    pub kind: String,
    pub bytes: u64,
    pub unpacked: u64,
    pub known: bool,
    pub member: String,
}

// Sample input: {"t":"flashimage","path":"/d/a.img.xz","kind":"xz","bytes":10,"unpacked":20,"known":true,"member":""}.
pub fn parse_flashimage(reply: &Json, path: &str) -> Option<FlashImage> {
    if text(reply, "path") != path {
        return None;
    }
    Some(FlashImage {
        kind: text(reply, "kind"),
        bytes: number(reply, "bytes"),
        unpacked: number(reply, "unpacked"),
        known: flag(reply, "known"),
        member: text(reply, "member"),
    })
}

/// One drive row from flashtargets.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TargetRow {
    pub dev: String,
    pub seq: u64,
    pub serial: String,
    pub label: String,
    pub model: String,
    pub bytes: u64,
    pub refused: String,
}

// Sample input: {"t":"flashtargets","drives":[{"dev":"sdb","seq":4,"serial":"S1","label":"STICK","model":"Ultra","bytes":32000000000,"refused":""}]}.
pub fn parse_flashtargets(reply: &Json) -> Vec<TargetRow> {
    reply
        .get("drives")
        .and_then(Json::as_array)
        .map(|rows| {
            rows.iter()
                .map(|row| TargetRow {
                    dev: text(row, "dev"),
                    seq: number(row, "seq"),
                    serial: text(row, "serial"),
                    label: text(row, "label"),
                    model: text(row, "model"),
                    bytes: number(row, "bytes"),
                    refused: text(row, "refused"),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The flash's own start: its id, the total it measures against and whether that total is known.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FlashStarted {
    pub id: u64,
    pub total: u64,
    pub known: bool,
}

// Sample input: {"t":"flashstarted","id":31,"total":124623257600,"known":true}.
pub fn parse_flashstarted(reply: &Json) -> FlashStarted {
    FlashStarted { id: number(reply, "id"), total: number(reply, "total"), known: flag(reply, "known") }
}

/// One progress sample: the phase, the bytes the drive took and the permille share.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FlashProgress {
    pub id: u64,
    pub phase: String,
    pub bytes: u64,
    pub total: u64,
    pub permille: u64,
}

// Sample input: {"t":"flashprogress","id":31,"phase":"write","bytes":1048576,"total":124623257600,"permille":8}.
pub fn parse_flashprogress(reply: &Json) -> FlashProgress {
    FlashProgress {
        id: number(reply, "id"),
        phase: text(reply, "phase"),
        bytes: number(reply, "bytes"),
        total: number(reply, "total"),
        permille: number(reply, "permille"),
    }
}

/// The terminal line: whether it landed, in which phase, where it stood and why not.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FlashDone {
    pub id: u64,
    pub ok: bool,
    pub phase: String,
    pub at: u64,
    pub err: String,
    pub blank: bool,
    pub tool: String,
}

// Sample input: {"t":"flashdone","id":31,"ok":true,"phase":"verify","at":0,"err":"","blank":false}.
pub fn parse_flashdone(reply: &Json) -> FlashDone {
    FlashDone {
        id: number(reply, "id"),
        ok: flag(reply, "ok"),
        phase: text(reply, "phase"),
        at: number(reply, "at"),
        err: text(reply, "err"),
        blank: flag(reply, "blank"),
        tool: text(reply, "tool"),
    }
}

/// One extracting sample from a `job` line of kind extract: the count from the archive's own index.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExtractSample {
    pub id: u64,
    pub done: usize,
    pub total: usize,
    pub file: String,
    pub item_bytes: u64,
    pub bytes: u64,
    pub total_bytes: u64,
    pub rate: u64,
    pub left: Option<u64>,
}

// Sample input: {"t":"job","job":{"kind":"extract","done":6,"total":18,"file":"DSC_4907.jpg"},"unfinished":3}; another kind is not taken.
pub fn parse_extract_job(reply: &Json) -> Option<ExtractSample> {
    if text(reply, "t") != "job" {
        return None;
    }
    let job = reply.get("job")?;
    if text(job, "kind") != "extract" {
        return None;
    }
    let n = |key: &str| number(job, key);
    Some(ExtractSample {
        id: n("id"),
        done: n("done") as usize,
        total: n("total") as usize,
        file: text(job, "file"),
        item_bytes: n("itemBytes"),
        bytes: n("bytes"),
        total_bytes: n("totalBytes"),
        rate: n("rate"),
        left: job.get("left").and_then(Json::as_f64).map(|n| n as u64),
    })
}

/// The eject answer: landed or still mounted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FlashEject {
    pub ok: bool,
    pub err: String,
}

// Sample input: {"t":"flasheject","ok":true,"err":""}.
pub fn parse_flasheject(reply: &Json) -> FlashEject {
    FlashEject { ok: flag(reply, "ok"), err: text(reply, "err") }
}
