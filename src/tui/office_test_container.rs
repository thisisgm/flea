//! Valid ZIP container envelopes keep the existing mocked-conversion tests independent of container rejection.
pub(crate) fn container(body: &[u8]) -> Vec<u8> {
    let xml = [
        b"<?xml version=\"1.0\"?><test>".as_slice(),
        body,
        b"</test>",
    ]
    .concat();
    let entries: [(&str, &[u8]); 3] = [
        ("mimetype", b"application/vnd.oasis.opendocument.text"),
        ("[Content_Types].xml", b"<Types/>"),
        ("content.xml", &xml),
    ];
    let mut output = Vec::new();
    let mut directory = Vec::new();
    for (name, body) in entries {
        let offset = output.len() as u32;
        output.extend_from_slice(b"PK\x03\x04");
        output.extend_from_slice(&[20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        output.extend_from_slice(&(body.len() as u32).to_le_bytes());
        output.extend_from_slice(&(body.len() as u32).to_le_bytes());
        output.extend_from_slice(&(name.len() as u16).to_le_bytes());
        output.extend_from_slice(&0u16.to_le_bytes());
        output.extend_from_slice(name.as_bytes());
        output.extend_from_slice(body);
        directory.extend_from_slice(b"PK\x01\x02");
        directory.extend_from_slice(&[20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        directory.extend_from_slice(&(body.len() as u32).to_le_bytes());
        directory.extend_from_slice(&(body.len() as u32).to_le_bytes());
        directory.extend_from_slice(&(name.len() as u16).to_le_bytes());
        directory.extend_from_slice(&[0; 12]);
        directory.extend_from_slice(&offset.to_le_bytes());
        directory.extend_from_slice(name.as_bytes());
    }
    let offset = output.len() as u32;
    output.extend_from_slice(&directory);
    output.extend_from_slice(b"PK\x05\x06");
    output.extend_from_slice(&[0; 4]);
    output.extend_from_slice(&3u16.to_le_bytes());
    output.extend_from_slice(&3u16.to_le_bytes());
    output.extend_from_slice(&(directory.len() as u32).to_le_bytes());
    output.extend_from_slice(&offset.to_le_bytes());
    output.extend_from_slice(&[0; 2]);
    output
}
