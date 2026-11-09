use super::*;

#[test]
fn a_raw_named_container_is_typed_by_its_extension() {
    use std::os::unix::ffi::OsStrExt;
    let exact = crate::bytename::path_text(std::path::Path::new(std::ffi::OsStr::from_bytes(
        b"/d/r\xff.odt",
    )))
    .into_owned();
    assert!(
        is_container_path(&exact),
        "the extension sits before the exact tail"
    );
}

// A zip of stored entries, each name and data, with its central directory and end record.
fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    zip_at(entries, 0)
}

// The same zip placed `base` bytes into a file, its offsets counted from the file's start.
fn zip_at(entries: &[(&str, &[u8])], base: u64) -> Vec<u8> {
    let mut out = Vec::new();
    let mut dir = Vec::new();
    for (name, data) in entries {
        let at = (base + out.len() as u64) as u32;
        out.extend_from_slice(&ZIP_MAGIC);
        out.extend_from_slice(&[20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        dir.extend_from_slice(&ENTRY_MAGIC);
        dir.extend_from_slice(&[20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        dir.extend_from_slice(&(data.len() as u32).to_le_bytes());
        dir.extend_from_slice(&(data.len() as u32).to_le_bytes());
        dir.extend_from_slice(&(name.len() as u16).to_le_bytes());
        dir.extend_from_slice(&[0; 12]);
        dir.extend_from_slice(&at.to_le_bytes());
        dir.extend_from_slice(name.as_bytes());
    }
    let dir_at = (base + out.len() as u64) as u32;
    out.extend_from_slice(&dir);
    out.extend_from_slice(&END_MAGIC);
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(dir.len() as u32).to_le_bytes());
    out.extend_from_slice(&dir_at.to_le_bytes());
    out.extend_from_slice(&[0, 0]);
    out
}

fn over(bytes: Vec<u8>) -> impl FnMut(u64, usize) -> Option<Vec<u8>> {
    move |at, len| {
        let at = at as usize;
        bytes
            .get(at..(at + len).min(bytes.len()))
            .map(<[u8]>::to_vec)
    }
}

fn verdict(path: &str, bytes: Vec<u8>) -> bool {
    let size = bytes.len() as u64;
    sniff(path, size, &mut over(bytes)).0
}

fn real(path: &str, bytes: Vec<u8>) -> bool {
    let size = bytes.len() as u64;
    is_real_container(path, size, &mut over(bytes))
}

#[test]
fn office_sniff_an_encrypted_ooxml_package_is_a_compound_file() {
    let mut cfb = CFB_MAGIC.to_vec();
    cfb.extend_from_slice(&[0; 504]);
    assert!(
        verdict("/d/lease.docx", cfb.clone()),
        "a .docx that is a compound file is locked"
    );
    assert!(
        verdict("/d/budget.XLSX", cfb.clone()),
        "the extension is read in any case"
    );
    assert!(
        !verdict("/d/old.doc", cfb),
        "a legacy .doc is a compound file by nature, never locked by that alone"
    );
    let plain = zip(&[("[Content_Types].xml", b"<Types/>")]);
    assert!(
        !verdict("/d/garden-report.docx", plain),
        "a zipped .docx is not locked"
    );
}

#[test]
fn office_sniff_an_encrypted_odf_stores_content_that_is_not_xml() {
    let locked = zip(&[
        ("mimetype", b"application/vnd.oasis.opendocument.text"),
        ("content.xml", &[0x9c, 0x13, 0x77, 0x01]),
    ]);
    assert!(
        verdict("/d/lease.odt", locked),
        "stored content.xml that is not XML is encrypted"
    );
    let open = zip(&[
        ("mimetype", b"application/vnd.oasis.opendocument.text"),
        ("content.xml", b"<?xml version=\"1.0\"?>"),
    ]);
    assert!(!verdict("/d/talk.odp", open), "stored XML is not encrypted");
    assert!(
        !verdict("/d/a.odt", b"not a zip at all".to_vec()),
        "a file that is not a zip is not locked"
    );
    let mut cut = zip(&[("content.xml", &[1, 2, 3])]);
    cut.truncate(cut.len() - 30);
    assert!(
        !verdict("/d/a.odt", cut),
        "a truncated directory is read as not locked"
    );
}

#[test]
fn office_container_a_fake_odt_is_not_a_real_container() {
    assert!(
        !real(
            "/d/letter.odt",
            b"PK not really an office document".to_vec()
        ),
        "a 33-byte fake .odt is not a real ODF container"
    );
    assert!(
        !real("/d/a.odt", b"not a zip at all".to_vec()),
        "an .odt that is not a zip is not a real container"
    );
    assert!(
        !real("/d/a.docx", zip(&[("word/document.xml", b"<w/>")])),
        "an OOXML zip with no [Content_Types].xml is not a real container"
    );
    assert!(
        !real(
            "/d/a.xlsx",
            zip(&[("mimetype", b"application/vnd.oasis.opendocument.text")])
        ),
        "an .xlsx holding only a mimetype is not a real OOXML container"
    );
    let odt = zip(&[
        ("mimetype", b"application/vnd.oasis.opendocument.text"),
        ("content.xml", b"<?xml version=\"1.0\"?>"),
    ]);
    assert!(real("/d/note.odt", odt), "a real .odt still previews");
    let docx = zip(&[
        ("[Content_Types].xml", b"<Types/>"),
        ("word/document.xml", b"<w/>"),
    ]);
    assert!(real("/d/report.docx", docx), "a real .docx still previews");
    let xlsx = zip(&[("[Content_Types].xml", b"<Types/>")]);
    assert!(real("/d/budget.xlsx", xlsx), "a real .xlsx still previews");
    let pptx = zip(&[("[Content_Types].xml", b"<Types/>")]);
    assert!(real("/d/talk.pptx", pptx), "a real .pptx still previews");
    assert!(
        real("/d/old.doc", b"not a zip at all".to_vec()),
        "a legacy .doc is not a container kind, so the conversion still decides"
    );
    let mut cfb = CFB_MAGIC.to_vec();
    cfb.resize(512, 0);
    assert!(
        real("/d/lease.docx", cfb),
        "a compound-file .docx is Office's encrypted package, not a fake"
    );
}

// The read is bounded: a 3 GB file (zip32 offsets) is sniffed from its head, its last 64 KiB and one local header, all three reached.
#[test]
fn office_sniff_huge_file_reaches_the_local_header() {
    let size: u64 = 3 << 30;
    let pad = size - zip(&[("content.xml", &[7; 32])]).len() as u64;
    let body = zip_at(&[("content.xml", &[7; 32])], pad);
    let mut reads = Vec::new();
    let mut read = |at: u64, len: usize| {
        reads.push((at, len));
        if at == 0 {
            return Some(ZIP_MAGIC.iter().copied().chain([0; 4]).collect());
        }
        let mut out = Vec::new();
        for i in 0..(len as u64).min(size - at) {
            let p = at + i;
            out.push(if p >= pad {
                body[(p - pad) as usize]
            } else {
                0
            });
        }
        Some(out)
    };
    let (locked, bytes) = sniff("/d/huge.odt", size, &mut read);
    assert_eq!(
        reads.len(),
        3,
        "the head, the tail and content.xml's local header: {reads:?}"
    );
    assert!(
        bytes <= HEAD + TAIL + LOCAL && locked,
        "read {bytes} bytes, locked {locked}"
    );
}

// Review r1: files LibreOffice 25.2.3 wrote with a password (ODF 1.4 whole-package and OOXML), and the same text open.
#[test]
fn office_sniff_real_libreoffice_files() {
    let odt = include_bytes!("../../tests/fixtures/office-locked/locked.odt").to_vec();
    let docx = include_bytes!("../../tests/fixtures/office-locked/locked.docx").to_vec();
    let open = include_bytes!("../../tests/fixtures/office-locked/note.odt").to_vec();
    assert!(
        !verdict("/d/note.odt", open),
        "the unlocked document is not locked"
    );
    assert!(
        verdict("/d/locked.docx", docx),
        "the encrypted OOXML package is locked"
    );
    assert!(
        verdict("/d/locked.odt", odt),
        "LibreOffice's own password-protected ODT is locked"
    );
    let whole = zip(&[
        (
            "mimetype",
            b"application/vnd.oasis.opendocument.spreadsheet",
        ),
        ("encrypted-package", &[3; 64]),
    ]);
    assert!(
        verdict("/d/a.ods", whole),
        "an encrypted-package entry is locked in any ODF kind"
    );
    let missing = zip(&[(
        "mimetype",
        b"application/vnd.oasis.opendocument.spreadsheet",
    )]);
    assert!(
        !verdict("/d/a.ods", missing),
        "an ODF with neither entry is a failure, not a password"
    );
    let tag = zip(&[("content.xml", b"<\x9c\x13\x77\x01\x02")]);
    assert!(
        verdict("/d/a.odt", tag),
        "encrypted data that happens to start with < is still not XML"
    );
}

#[test]
fn office_container_an_unreadable_directory_or_read_keeps_the_conversions_answer() {
    let mut big = ZIP_MAGIC.to_vec();
    big.resize(TAIL + 100, 0);
    let end = big.len() - 22;
    big[end..end + 4].copy_from_slice(&END_MAGIC);
    big[end + 10..end + 12].copy_from_slice(&1u16.to_le_bytes());
    big[end + 16..end + 20].copy_from_slice(&10u32.to_le_bytes());
    assert!(
        real("/d/deck.pptx", big),
        "a directory that starts before the tail cannot be read here, so the file stays real"
    );
    let mut zip64 = zip(&[("[Content_Types].xml", b"<Types/>")]);
    let end = zip64.len() - 22;
    zip64[end + 16..end + 20].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(
        real("/d/huge.xlsx", zip64),
        "a zip64 directory offset stays real"
    );
    let mut unreadable = |_: u64, _: usize| None;
    assert!(
        is_real_container("/d/report.docx", 4096, &mut unreadable),
        "a file the check cannot read keeps the conversion's answer"
    );
}
