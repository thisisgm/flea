use super::{codecs, Result, MAX_EXPANDED};
use std::collections::BTreeMap;

const EOCD_BYTES: usize = 22;
const MAX_COMMENT_BYTES: usize = 65_535;
const CENTRAL_HEADER_BYTES: usize = 46;
const LOCAL_HEADER_BYTES: usize = 30;
const MAX_MEMBERS: usize = 4096;
const MAX_MEMBER_NAME_BYTES: usize = 1024;

pub(super) fn u16_at(bytes: &[u8], at: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(bytes.get(at..at+2).ok_or("truncated 3D container")?.try_into().unwrap()))
}

pub(super) fn u32_at(bytes: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(bytes.get(at..at+4).ok_or("truncated 3D container")?.try_into().unwrap()))
}

// Sample input (hex): 50 4B 05 06 followed by eighteen 00 bytes is an empty ZIP; local and central signatures are 50 4B 03 04 and 50 4B 01 02.
pub(super) fn unpack(bytes: &[u8]) -> Result<BTreeMap<String,Vec<u8>>> {
    let end = (bytes.len().saturating_sub(EOCD_BYTES+MAX_COMMENT_BYTES)..bytes.len().saturating_sub(EOCD_BYTES-1)).rev()
        .find(|&at| bytes.get(at..at+4) == Some(b"PK\x05\x06") && u16_at(bytes,at+20).is_ok_and(|n| at+EOCD_BYTES+usize::from(n) == bytes.len()))
        .ok_or("ZIP directory is missing")?;
    if u16_at(bytes,end+4)? != 0 || u16_at(bytes,end+6)? != 0 || u16_at(bytes,end+8)? != u16_at(bytes,end+10)? {
        return Err("split ZIP is unsupported".into());
    }
    let count = usize::from(u16_at(bytes,end+10)?);
    if count > MAX_MEMBERS { return Err("too many 3D archive members".into()) }
    let mut at = u32_at(bytes,end+16)? as usize;
    let size = u32_at(bytes,end+12)? as usize;
    if at.checked_add(size) != Some(end) { return Err("invalid ZIP directory bound".into()) }
    let mut total = 0usize;
    let mut files = BTreeMap::new();
    for _ in 0..count {
        if bytes.get(at..at+4) != Some(b"PK\x01\x02") { return Err("invalid ZIP member".into()) }
        let flags = u16_at(bytes,at+8)?;
        let method = u16_at(bytes,at+10)?;
        let crc = u32_at(bytes,at+16)?;
        let packed = u32_at(bytes,at+20)? as usize;
        let expanded = u32_at(bytes,at+24)? as usize;
        let name_len = usize::from(u16_at(bytes,at+28)?);
        let extra_len = usize::from(u16_at(bytes,at+30)?);
        let comment_len = usize::from(u16_at(bytes,at+32)?);
        let local = u32_at(bytes,at+42)? as usize;
        let name = std::str::from_utf8(bytes.get(at+CENTRAL_HEADER_BYTES..at+CENTRAL_HEADER_BYTES+name_len).ok_or("truncated ZIP name")?).map_err(|_| "invalid ZIP member name")?;
        if name.len() > MAX_MEMBER_NAME_BYTES || name.starts_with('/') || name.contains(['\0','\\']) || name.split('/').any(|p| p == "..") {
            return Err("unsafe ZIP member name".into());
        }
        at = at.checked_add(CENTRAL_HEADER_BYTES+name_len+extra_len+comment_len).filter(|&v| v <= end).ok_or("truncated ZIP directory")?;
        if flags & 1 != 0 || !matches!(method,0|8) { return Err("unsupported ZIP encoding".into()) }
        total = total.checked_add(expanded).filter(|&n| n <= MAX_EXPANDED).ok_or("3D archive exceeds expanded bound")?;
        if bytes.get(local..local+4) != Some(b"PK\x03\x04") || u16_at(bytes,local+8)? != method { return Err("invalid local ZIP header".into()) }
        let local_name = usize::from(u16_at(bytes,local+26)?);
        let start = local.checked_add(LOCAL_HEADER_BYTES+local_name+usize::from(u16_at(bytes,local+28)?)).ok_or("invalid ZIP offset")?;
        if bytes.get(local+LOCAL_HEADER_BYTES..local+LOCAL_HEADER_BYTES+local_name) != Some(name.as_bytes()) || start.checked_add(packed).is_none_or(|n| n > end-size) {
            return Err("ZIP payload exceeds its bound".into());
        }
        let source = bytes.get(start..start+packed).ok_or("truncated ZIP payload")?;
        let payload = if method == 8 { codecs::inflate(source,expanded,false)? } else {
            if packed != expanded { return Err("ZIP stored size mismatch".into()) }
            source.to_vec()
        };
        if codecs::crc32(&payload) != crc { return Err("ZIP checksum mismatch".into()) }
        if files.insert(name.into(),payload).is_some() { return Err("duplicate ZIP member".into()) }
    }
    if at != end { return Err("ZIP directory size mismatch".into()) }
    Ok(files)
}
