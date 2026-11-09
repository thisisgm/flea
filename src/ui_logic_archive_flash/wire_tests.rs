//! The wire lines the real backend parses: every builder round-trips through the reply parsers' fields.
use super::wire::*;
use crate::jsondoc::parse;

#[test]
fn compress_builders_carry_format_menu_and_secret() {
    let line = compress_request(&["/d/a", "/d/b"], "/d/a.7z", "7z", 4);
    assert_eq!(line, r#"{"c":"archive","op":"compress","paths":["/d/a","/d/b"],"dest":"/d/a.7z","format":"7z","menuId":4}"#);
    let secret = compress_password_request(&["/d/a"], "/d/a.7z", "7z", "s3cret", true, 0);
    assert!(secret.contains(r#""password":"s3cret""#) && secret.contains(r#""encryptNames":true"#));
    let plain = extract_request("/d/backup.7z", "/d/backup", 0);
    assert_eq!(plain, r#"{"c":"archive","op":"extract","path":"/d/backup.7z","dest":"/d/backup","menuId":0}"#);
    let retry = extract_password_request("/d/backup.7z", "/d/backup", "s3cret", 0);
    assert!(retry.contains(r#""password":"s3cret""#));
}

#[test]
fn flash_builders_carry_identity_fields() {
    assert_eq!(flash_image_request("/d/a.img"), r#"{"c":"flashimage","path":"/d/a.img"}"#);
    assert_eq!(flash_targets_request("/d/a.img"), r#"{"c":"flashtargets","image":"/d/a.img"}"#);
    assert_eq!(
        flash_request(31, "/d/a.img", "sda", 2, "9", 124623257600),
        r#"{"c":"flash","id":31,"image":"/d/a.img","dev":"sda","seq":2,"serial":"9","bytes":124623257600}"#
    );
    assert_eq!(flash_cancel_request(31), r#"{"c":"flashcancel","id":31}"#);
    assert_eq!(flash_eject_request("sda", 2), r#"{"c":"flasheject","dev":"sda","seq":2}"#);
}

#[test]
fn archivedone_arrives_with_path_and_dest() {
    let reply = parse(r#"{"t":"archivedone","id":7,"ok":false,"verified":true,"err":"encrypted","path":"/a/backup.7z","dest":"/a/backup"}"#).unwrap();
    assert_eq!(
        parse_archivedone(&reply),
        ArchiveDone { id: 7, ok: false, err: "encrypted".into(), path: "/a/backup.7z".into(), dest: "/a/backup".into() }
    );
}

#[test]
fn extract_job_lines_parse_into_samples() {
    let reply = parse(r#"{"t":"job","job":{"id":9,"kind":"extract","head":"Extracting 7 of 18","file":"DSC_4907.jpg","done":6,"total":18,"bytes":3700000,"totalBytes":9700000,"itemBytes":586600,"rate":2400000,"left":2,"state":"running","reason":"","output":[],"undo":false,"cancelled":false},"unfinished":3}"#).unwrap();
    assert_eq!(
        parse_extract_job(&reply),
        Some(ExtractSample {
            id: 9,
            done: 6,
            total: 18,
            file: "DSC_4907.jpg".into(),
            item_bytes: 586600,
            bytes: 3700000,
            total_bytes: 9700000,
            rate: 2400000,
            left: Some(2),
        }),
        "the producer's nested job line builds the sample"
    );
    let other = parse(r#"{"t":"job","job":{"id":10,"kind":"copy","done":1,"total":3},"unfinished":1}"#).unwrap();
    assert_eq!(parse_extract_job(&other), None, "another kind is not taken");
    let flat = parse(r#"{"t":"job","kind":"extract","done":6,"total":18}"#).unwrap();
    assert_eq!(parse_extract_job(&flat), None, "the invented flat line is not the producer's shape");
}

#[test]
fn flash_replies_parse_into_rows_and_runs() {
    let image = parse(r#"{"t":"flashimage","path":"/d/a.img.xz","kind":"xz","bytes":10,"unpacked":20,"known":true,"member":""}"#).unwrap();
    assert_eq!(
        parse_flashimage(&image, "/d/a.img.xz"),
        Some(FlashImage { kind: "xz".into(), bytes: 10, unpacked: 20, known: true, member: String::new() })
    );
    assert_eq!(parse_flashimage(&image, "/d/other.zip"), None);
    let targets = parse(r#"{"t":"flashtargets","drives":[{"dev":"sda","seq":2,"serial":"9","label":"128GB","model":"General USB Flash Disk","bytes":124623257600,"refused":""}]}"#).unwrap();
    let rows = parse_flashtargets(&targets);
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0].dev.as_str(), rows[0].bytes), ("sda", 124623257600));
    let started = parse(r#"{"t":"flashstarted","id":31,"total":124623257600,"known":true}"#).unwrap();
    assert_eq!(parse_flashstarted(&started), FlashStarted { id: 31, total: 124623257600, known: true });
    let progress = parse(r#"{"t":"flashprogress","id":31,"phase":"write","bytes":1048576,"total":124623257600,"permille":8}"#).unwrap();
    let sample = parse_flashprogress(&progress);
    assert_eq!((sample.id, sample.phase.as_str(), sample.bytes, sample.permille), (31, "write", 1048576, 8));
    let done = parse(r#"{"t":"flashdone","id":31,"ok":true,"phase":"verify","at":0,"err":"","blank":false}"#).unwrap();
    assert_eq!(
        parse_flashdone(&done),
        FlashDone { id: 31, ok: true, phase: "verify".into(), at: 0, err: String::new(), blank: false, tool: String::new() }
    );
    let eject = parse(r#"{"t":"flasheject","ok":true,"err":""}"#).unwrap();
    assert_eq!(parse_flasheject(&eject), FlashEject { ok: true, err: String::new() });
}

#[test]
fn flash_device_names_escape_into_request_lines() {
    let line = flash_request(31, "/d/a.img", "sda\" --x", 2, "9", 10);
    assert!(line.contains(r#""dev":"sda\" --x""#), "a hostile dev never breaks the line: {line}");
    let eject = flash_eject_request("sda\" --x", 2);
    assert!(eject.contains(r#""dev":"sda\" --x""#), "a hostile dev never breaks the line: {eject}");
}
