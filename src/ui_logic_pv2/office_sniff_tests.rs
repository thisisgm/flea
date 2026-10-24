use super::*;

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
