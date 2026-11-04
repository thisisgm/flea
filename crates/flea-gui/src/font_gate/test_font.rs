//! Give the bundled font a test family without depending on a host's font installation.

// OpenType offsets: font header numTables 4; table record tag 0, checkSum 4, offset 8, length 12.
const TABLE_DIRECTORY: usize = 12;
const TABLE_RECORD: usize = 16;
// name table: format 0, count 2, stringOffset 4; name record platformID 0, length 8, offset 10.
const NAME_HEADER: usize = 6;
const NAME_RECORD: usize = 12;
// head.checkSumAdjustment sits at 8 and makes the whole font sum to this OpenType constant.
const HEAD_ADJUSTMENT: usize = 8;
const CHECKSUM_MAGIC: u32 = 0xb1b0_afba;

pub(super) fn named(source: &[u8], family: &str) -> Vec<u8> {
    fn word(bytes: &[u8], at: usize) -> usize {
        u16::from_be_bytes(bytes[at..at + 2].try_into().unwrap()) as usize
    }
    fn long(bytes: &[u8], at: usize) -> usize {
        u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize
    }
    fn checksum(bytes: &[u8]) -> u32 {
        bytes.chunks(4).fold(0u32, |sum, chunk| {
            let mut word = [0; 4];
            word[..chunk.len()].copy_from_slice(chunk);
            sum.wrapping_add(u32::from_be_bytes(word))
        })
    }
    let record = |tag: &[u8]| {
        (0..word(source, 4))
            .map(|index| TABLE_DIRECTORY + index * TABLE_RECORD)
            .find(|&at| &source[at..at + 4] == tag)
            .unwrap()
    };
    let table = record(b"name");
    let at = long(source, table + 8);
    assert_eq!(
        word(source, at),
        0,
        "bundled font name table format changed"
    );
    let count = word(source, at + 2);
    let strings = at + word(source, at + 4);
    let mut name = source[at..at + NAME_HEADER + count * NAME_RECORD].to_vec();
    let start = name.len();
    name[4..6].copy_from_slice(&(start as u16).to_be_bytes());
    for index in 0..count {
        let record = NAME_HEADER + index * NAME_RECORD;
        let length = word(source, at + record + 8);
        let offset = word(source, at + record + 10);
        let bytes = &source[strings + offset..strings + offset + length];
        let unicode = matches!(word(source, at + record), 0 | 3);
        let text = if unicode {
            let words: Vec<_> = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|word| u16::from_be_bytes(*word))
                .collect();
            String::from_utf16(&words).unwrap()
        } else {
            String::from_utf8(bytes.to_vec()).unwrap()
        };
        let text = text
            .replace("JetBrainsMono", &family.replace(' ', ""))
            .replace("JetBrains Mono", family);
        let bytes: Vec<_> = if unicode {
            text.encode_utf16().flat_map(u16::to_be_bytes).collect()
        } else {
            text.into_bytes()
        };
        let offset = u16::try_from(name.len() - start).unwrap();
        let length = u16::try_from(bytes.len()).unwrap();
        name[record + 8..record + 10].copy_from_slice(&length.to_be_bytes());
        name[record + 10..record + 12].copy_from_slice(&offset.to_be_bytes());
        name.extend(bytes);
    }
    let mut font = source.to_vec();
    font.resize(font.len().next_multiple_of(4), 0);
    let offset = font.len() as u32;
    font[table + 4..table + 8].copy_from_slice(&checksum(&name).to_be_bytes());
    font[table + 8..table + 12].copy_from_slice(&offset.to_be_bytes());
    font[table + 12..table + 16].copy_from_slice(&(name.len() as u32).to_be_bytes());
    font.extend(name);
    let adjustment = long(source, record(b"head") + 8) + HEAD_ADJUSTMENT;
    font[adjustment..adjustment + 4].fill(0);
    let sum = CHECKSUM_MAGIC.wrapping_sub(checksum(&font));
    font[adjustment..adjustment + 4].copy_from_slice(&sum.to_be_bytes());
    font
}
