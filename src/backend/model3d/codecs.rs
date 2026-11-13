use super::{Result, MAX_EXPANDED};

pub(crate) fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 { crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1)) }
    }
    !crc
}

struct Bits<'a> { bytes: &'a [u8], bit: usize }

impl Bits<'_> {
    fn get(&mut self, count: usize) -> Result<usize> {
        if self.bit.checked_add(count).is_none_or(|end| end > self.bytes.len() * 8) { return Err("truncated compressed stream".into()) }
        let mut value = 0;
        for shift in 0..count {
            value |= usize::from((self.bytes[self.bit / 8] >> (self.bit % 8)) & 1) << shift;
            self.bit += 1;
        }
        Ok(value)
    }
}

struct Huffman { count: [usize; 16], symbols: Vec<usize> }

impl Huffman {
    fn new(lengths: &[usize]) -> Result<Self> {
        let mut count = [0; 16];
        for &len in lengths {
            if len > 15 { return Err("invalid Huffman length".into()) }
            count[len] += 1;
        }
        count[0] = 0;
        let mut left = 1isize;
        for &n in &count[1..] {
            left = (left << 1) - n as isize;
            if left < 0 { return Err("oversubscribed Huffman tree".into()) }
        }
        let mut symbols = Vec::new();
        for len in 1..=15 {
            for (symbol, &n) in lengths.iter().enumerate() { if n == len { symbols.push(symbol) } }
        }
        Ok(Self { count, symbols })
    }

    fn decode(&self, bits: &mut Bits) -> Result<usize> {
        let (mut code, mut first, mut index) = (0, 0, 0);
        for len in 1..=15 {
            code = (code << 1) | bits.get(1)?;
            if code >= first && code - first < self.count[len] { return Ok(self.symbols[index + code - first]) }
            index += self.count[len];
            first = (first + self.count[len]) << 1;
        }
        Err("invalid Huffman code".into())
    }
}

fn trees(bits: &mut Bits, fixed: bool) -> Result<(Huffman, Huffman)> {
    if fixed {
        let lengths: Vec<usize> = (0..288).map(|i| match i { 0..=143 => 8, 144..=255 => 9, 256..=279 => 7, _ => 8 }).collect();
        return Ok((Huffman::new(&lengths)?, Huffman::new(&[5; 32])?));
    }
    let nl = bits.get(5)? + 257;
    let nd = bits.get(5)? + 1;
    let nc = bits.get(4)? + 4;
    if nl > 286 { return Err("invalid deflate tree size".into()) }
    const ORDER: [usize; 19] = [16,17,18,0,8,7,9,6,10,5,11,4,12,3,13,2,14,1,15];
    let mut code = [0; 19];
    for &i in &ORDER[..nc] { code[i] = bits.get(3)? }
    let code = Huffman::new(&code)?;
    let mut lengths = Vec::new();
    while lengths.len() < nl + nd {
        let symbol = code.decode(bits)?;
        let (n, value) = match symbol {
            0..=15 => (1, symbol),
            16 => (bits.get(2)? + 3, *lengths.last().ok_or("Huffman repeat has no predecessor")?),
            17 => (bits.get(3)? + 3, 0),
            18 => (bits.get(7)? + 11, 0),
            _ => return Err("invalid Huffman repeat".into()),
        };
        if lengths.len() + n > nl + nd { return Err("Huffman repeat exceeds tree".into()) }
        lengths.resize(lengths.len() + n, value);
    }
    if lengths[256] == 0 { return Err("deflate tree has no end marker".into()) }
    Ok((Huffman::new(&lengths[..nl])?, Huffman::new(&lengths[nl..])?))
}

pub(crate) fn inflate(bytes: &[u8], expected: usize, zlib: bool) -> Result<Vec<u8>> {
    if expected > MAX_EXPANDED { return Err("expanded stream exceeds its bound".into()) }
    let source = if zlib {
        if bytes.len() < 6 || bytes[0] & 15 != 8 || bytes[0] >> 4 > 7 || bytes[1] & 32 != 0
            || !u16::from_be_bytes([bytes[0],bytes[1]]).is_multiple_of(31) { return Err("invalid zlib stream".into()) }
        &bytes[2..bytes.len()-4]
    } else { bytes };
    let mut bits = Bits { bytes: source, bit: 0 };
    let mut out = Vec::new();
    loop {
        let last = bits.get(1)? != 0;
        match bits.get(2)? {
            0 => {
                bits.bit = bits.bit.div_ceil(8) * 8;
                let len = bits.get(16)?;
                if bits.get(16)? != (len ^ 65535) || len > expected - out.len() { return Err("invalid stored deflate block".into()) }
                for _ in 0..len { out.push(bits.get(8)? as u8) }
            }
            kind @ (1 | 2) => {
                let (literal, distance) = trees(&mut bits, kind == 1)?;
                loop {
                    let symbol = literal.decode(&mut bits)?;
                    match symbol {
                        0..=255 => {
                            if out.len() >= expected { return Err("deflate output exceeds declared size".into()) }
                            out.push(symbol as u8);
                        }
                        256 => break,
                        257..=285 => {
                            const LENGTHS: [usize; 29] = [3,4,5,6,7,8,9,10,11,13,15,17,19,23,27,31,35,43,51,59,67,83,99,115,131,163,195,227,258];
                            const EXTRA: [usize; 29] = [0,0,0,0,0,0,0,0,1,1,1,1,2,2,2,2,3,3,3,3,4,4,4,4,5,5,5,5,0];
                            const DISTANCE: [usize; 30] = [1,2,3,4,5,7,9,13,17,25,33,49,65,97,129,193,257,385,513,769,1025,1537,2049,3073,4097,6145,8193,12289,16385,24577];
                            let n = LENGTHS[symbol-257] + bits.get(EXTRA[symbol-257])?;
                            let d = distance.decode(&mut bits)?;
                            if d >= 30 { return Err("invalid deflate distance".into()) }
                            let back = DISTANCE[d] + bits.get(if d < 4 { 0 } else { d/2 - 1 })?;
                            if back > out.len() || n > expected - out.len() { return Err("deflate back-reference exceeds its bound".into()) }
                            for _ in 0..n { out.push(out[out.len()-back]) }
                        }
                        _ => return Err("invalid deflate symbol".into()),
                    }
                }
            }
            _ => return Err("reserved deflate block".into()),
        }
        if last { break }
    }
    if out.len() != expected || bits.bit.div_ceil(8) != source.len() { return Err("deflate size mismatch".into()) }
    if zlib {
        let (mut a,mut b) = (1u32,0u32);
        for &v in &out { a = (a + u32::from(v)) % 65521; b = (b + a) % 65521 }
        if bytes[bytes.len()-4..] != ((b << 16) | a).to_be_bytes() { return Err("zlib checksum mismatch".into()) }
    }
    Ok(out)
}

pub(super) fn heatshrink(bytes: &[u8], expected: usize, window: usize) -> Result<Vec<u8>> {
    if expected > MAX_EXPANDED { return Err("heatshrink output exceeds its bound".into()) }
    let mut at = 0;
    let mut read = |count: usize| -> Result<usize> {
        if at + count > bytes.len()*8 { return Err("truncated heatshrink stream".into()) }
        let mut value = 0;
        for _ in 0..count { value = value*2 + usize::from((bytes[at/8] >> (7-at%8)) & 1); at += 1 }
        Ok(value)
    };
    let mut out = Vec::new();
    while out.len() < expected {
        if read(1)? == 1 { out.push(read(8)? as u8) }
        else {
            let distance = read(window)? + 1;
            let length = read(4)? + 1;
            if length > expected-out.len() { return Err("heatshrink output exceeds declared size".into()) }
            for _ in 0..length { out.push(if distance > out.len() { 0 } else { out[out.len()-distance] }) }
        }
    }
    if bytes.len()*8 - at >= 8 { return Err("trailing heatshrink bytes".into()) }
    Ok(out)
}

// Sample input: SGVsbG8= decodes to the five bytes Hello.
pub(super) fn base64(source: &str) -> Result<Vec<u8>> {
    if source.len() > super::MAX_THUMBNAIL*2 { return Err("thumbnail exceeds its bound".into()) }
    let mut out = Vec::new();
    let (mut acc,mut bits,mut padding) = (0u32,0,0);
    for b in source.bytes().filter(|b| !b.is_ascii_whitespace()) {
        if b == b'=' { padding += 1; if padding > 2 { return Err("invalid base64 padding".into()) } continue }
        if padding != 0 { return Err("base64 data follows padding".into()) }
        let n = match b { b'A'..=b'Z' => b-b'A', b'a'..=b'z' => b-b'a'+26, b'0'..=b'9' => b-b'0'+52, b'+' => 62, b'/' => 63, _ => return Err("invalid base64".into()) };
        acc = (acc << 6) | u32::from(n);
        bits += 6;
        if bits >= 8 { bits -= 8; out.push((acc >> bits) as u8) }
    }
    if !matches!((bits,padding), (0,0) | (2,1) | (4,2)) || acc & ((1 << bits)-1) != 0 { return Err("truncated base64".into()) }
    Ok(out)
}
