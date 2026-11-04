use super::{codecs, Model, Result, MAX_EXPANDED};
use super::zip::{u16_at,u32_at};

const MAX_BLOCKS: usize = 65_536;
const GCODE_BLOCK: u16 = 1;
const THUMBNAIL_BLOCK: u16 = 5;
const LAST_METADATA_BLOCK: u16 = 4;

// Sample input: FF FF FB 1D EB 01 0F 59 CC FF FF F9 (hex) expands to "G1 X10Y0\n".
fn unpack_meat(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let (mut packing,mut nospace) = (false,false);
    let (mut signal,mut command,mut full) = (false,false,0);
    let mut saved = None;
    let mut data = |byte: u8, packing: bool, nospace: bool, full: &mut usize, saved: &mut Option<u8>| -> Result<()> {
        let lookup = |n: u8| if n == 11 && nospace { b'E' } else { b"0123456789. \nGX"[n as usize] };
        if !packing { out.push(byte) }
        else if *full != 0 {
            out.push(byte);
            if let Some(c) = saved.take() { out.push(c) }
            *full -= 1;
        } else {
            let (lo,hi) = (byte & 15,byte >> 4);
            if lo == 15 {
                *full = if hi == 15 { 2 } else { *saved = Some(lookup(hi)); 1 };
            } else {
                let c = lookup(lo);
                out.push(c);
                if c != b'\n' { if hi == 15 { *full = 1 } else { out.push(lookup(hi)) } }
            }
        }
        if out.len() > MAX_EXPANDED { return Err("MeatPack output exceeds its bound".into()) }
        Ok(())
    };
    for &byte in bytes {
        if command {
            match byte {
                251 => packing = true, 250|249 => packing = false,
                247 => nospace = true, 246 => nospace = false, 248 => {},
                _ => return Err("unsupported MeatPack command".into()),
            }
            command = false;
        } else if byte == 255 {
            if signal { command = true; signal = false } else { signal = true }
        } else {
            if signal { data(255,packing,nospace,&mut full,&mut saved)?; signal = false }
            data(byte,packing,nospace,&mut full,&mut saved)?;
        }
    }
    if signal { data(255,packing,nospace,&mut full,&mut saved)? }
    if command || full != 0 || saved.is_some() { return Err("truncated MeatPack stream".into()) }
    Ok(out)
}

// Sample input (hex): 47 43 44 45 01 00 00 00 00 00 01 00 00 00 07 00 00 00 00 00 47 31 20 58 31 30 0A encodes G1 X10 followed by newline.
pub(super) fn parse(bytes: &[u8], model: &mut Model) -> Result<()> {
    if bytes.get(..4) != Some(b"GCDE") || u32_at(bytes,4)? != 1 { return Err("unsupported binary G-code header".into()) }
    let checksum = u16_at(bytes,8)?;
    if checksum > 1 { return Err("unsupported binary G-code checksum".into()) }
    let mut at = 10usize;
    let mut text = Vec::new();
    let mut blocks = 0;
    let mut expanded_total = 0usize;
    while at < bytes.len() {
        blocks += 1;
        if blocks > MAX_BLOCKS { return Err("too many binary G-code blocks".into()) }
        let begin = at;
        let kind = u16_at(bytes,at)?;
        let compression = u16_at(bytes,at+2)?;
        let expanded = u32_at(bytes,at+4)? as usize;
        let packed = if compression == 0 { expanded } else { u32_at(bytes,at+8)? as usize };
        at += if compression == 0 { 8 } else { 12 };
        let params = if kind == THUMBNAIL_BLOCK { 6 } else if kind <= LAST_METADATA_BLOCK { 2 } else { return Err("unsupported binary G-code block".into()) };
        let encoding = u16_at(bytes,at)?;
        let payload = at.checked_add(params).and_then(|n| n.checked_add(packed)).filter(|&n| n <= bytes.len()).ok_or("truncated binary G-code block")?;
        expanded_total = expanded_total.checked_add(expanded).filter(|&n| n <= MAX_EXPANDED).ok_or("binary G-code expansion exceeds its bound")?;
        let source = &bytes[at+params..payload];
        if checksum == 1 && u32_at(bytes,payload)? != codecs::crc32(&bytes[begin..payload]) {
            return Err("binary G-code checksum mismatch".into());
        }
        let data = match compression {
            0 => source.to_vec(),
            1 => codecs::inflate(source,expanded,true)?,
            2|3 => codecs::heatshrink(source,expanded,if compression == 2 { 11 } else { 12 })?,
            _ => return Err("unsupported binary G-code compression".into()),
        };
        match kind {
            GCODE_BLOCK => {
                let data = match encoding { 0 => data, 1|2 => unpack_meat(&data)?, _ => return Err("unsupported binary G-code encoding".into()) };
                if data.len() > MAX_EXPANDED-text.len() { return Err("binary toolpath exceeds its bound".into()) }
                text.extend_from_slice(&data);
            }
            THUMBNAIL_BLOCK => {
                if u16_at(bytes,at+2)? == 0 || u16_at(bytes,at+4)? == 0 { return Err("invalid binary thumbnail dimensions".into()) }
                let format = match encoding { 0 => "png", 1 => "jpg", 2 => "qoi", _ => return Err("unsupported binary thumbnail".into()) };
                model.thumbnail(None,format,data)?;
            }
            _ => {
                if encoding != 0 { return Err("unsupported binary metadata encoding".into()) }
                super::projectmeta::pairs(model,super::text(&data)?);
            }
        }
        at = payload+if checksum == 1 { 4 } else { 0 };
    }
    super::gcode::parse(&text,model)
}

#[cfg(test)]
pub(super) fn meat_for_test(bytes: &[u8]) -> Result<Vec<u8>> { unpack_meat(bytes) }
