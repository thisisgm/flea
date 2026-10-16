// Fontconfig validates every container; the file extension is never proof that a font is usable.
#[path = "fontaxes.rs"]
pub mod axes;
use crate::json::escape;
use std::path::Path;
use std::process::{Command, Stdio};

pub struct Face { pub family: String, pub style: String, pub index: u32, pub variable: bool, pub monospace: bool, pub sample: String }
pub struct Font { pub faces: Vec<Face>, pub format: String, pub extension: &'static str, pub axes: String }

impl Font {
    pub fn faces_json(&self) -> String {
        format!("[{}]", self.faces.iter().map(|f| format!("{{\"family\":\"{}\",\"style\":\"{}\",\"index\":{},\"variable\":{},\"monospace\":{},\"sample\":\"{}\"}}",
            escape(&f.family), escape(&f.style), f.index, f.variable, f.monospace, escape(&f.sample))).collect::<Vec<_>>().join(","))
    }
}

pub fn inspect(path: &Path, bytes: &[u8]) -> Result<Font, String> {
    let output = Command::new("timeout").args(["10", "fc-scan", "--format",
        "%{family[0]}\x1f%{style[0]}\x1f%{index}\x1f%{fontformat}\x1f%{variable}\x1f%{charset}\x1f%{spacing}\x1e", "--"])
        .arg(path).stdin(Stdio::null()).stderr(Stdio::null()).output()
        .map_err(|_| "Font inspection needs fontconfig (fc-scan) and coreutils (timeout).")?;
    if !output.status.success() { return Err("Linux could not read this font. Metrics files (AFM/PFM) are not installable fonts on their own.".into()); }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut faces: Vec<Face> = Vec::new();
    let mut format = String::new();
    // Sample record: "Noto Sans\x1fRegular\x1f0\x1fTrueType\x1fTrue\x1f20-7e\x1e"; named instances use the upper index bits.
    for record in text.split('\x1e').filter(|s| !s.is_empty()) {
        let fields: Vec<_> = record.split('\x1f').collect();
        if fields.len() != 7 || fields[0].is_empty() { return Err("The font metadata is invalid.".into()); }
        let index: u32 = fields[2].parse().map_err(|_| "The font face index is invalid.")?;
        if index >> 16 != 0 { continue; }
        let variable = fields[4] == "True";
        if let Some(face) = faces.iter_mut().find(|f| f.index == index) { face.variable |= variable; continue; }
        if faces.len() == 256 { return Err("The collection exceeds 256 font faces.".into()); }
        faces.push(Face { family: fields[0].into(), style: fields[1].into(), index, variable, monospace: matches!(fields[6], "90" | "100" | "110"), sample: sample(fields[5]) });
        format = fields[3].into();
    }
    if faces.is_empty() { return Err("Linux found no usable font faces in this file.".into()); }
    let extension = extension(bytes, &format).ok_or("Linux read the font, but its container is not supported for installation.")?;
    let variables: Vec<_> = faces.iter().filter(|f| f.variable).map(|f| f.index.to_string()).collect();
    let axes = if variables.is_empty() { "{}".into() } else {
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        let result = Command::new("timeout").arg("10").arg(executable).args(["--font", "axes"])
            .arg(path).arg(variables.join(",")).stdin(Stdio::null()).stderr(Stdio::null()).output()
            .map_err(|_| "Variable font inspection needs FreeType and coreutils (timeout).")?;
        if !result.status.success() { return Err("The variable font axes could not be read.".into()); }
        String::from_utf8(result.stdout).map_err(|_| "The variable font axes are invalid.")?
    };
    Ok(Font { faces, format, extension, axes })
}

fn extension(bytes: &[u8], format: &str) -> Option<&'static str> {
    match bytes.get(..4) {
        Some(b"\0\x01\0\0") | Some(b"true") => return Some("ttf"),
        Some(b"OTTO") => return Some("otf"),
        Some(b"ttcf") => return Some("ttc"),
        Some(b"wOFF") => return Some("woff"),
        Some(b"wOF2") => return Some("woff2"),
        _ => {},
    }
    let zipped = bytes.starts_with(&[0x1f, 0x8b]);
    Some(match format {
        "Type 1" | "CID Type 1" => if bytes.starts_with(&[0x80]) { "pfb" } else { "pfa" },
        "Type 42" => "t42",
        "CFF" => "cff",
        "BDF" => if zipped { "bdf.gz" } else { "bdf" },
        "PCF" => if zipped { "pcf.gz" } else { "pcf" },
        "Windows FNT" => if bytes.starts_with(b"MZ") { "fon" } else { "fnt" },
        "PFR" => "pfr",
        // Resource-fork fonts validated by FreeType; never confuse them with an office document.
        "TrueType" => "dfont",
        _ => return None,
    })
}

pub fn discoverable(path: &Path) -> bool {
    let Ok(real) = path.canonicalize() else { return false; };
    let Ok(out) = Command::new("timeout").args(["10", "fc-list", "--format", "%{file}\n"])
        .stdin(Stdio::null()).stderr(Stdio::null()).output() else { return false; };
    out.status.success() && String::from_utf8_lossy(&out.stdout).lines().any(|p| Path::new(p) == real)
}

// Sample charset "20-7e f000-f005" yields only actual mapped, printable glyphs, bounded independently of range size.
fn sample(charset: &str) -> String {
    let mut out = String::new();
    for range in charset.split_whitespace() {
        let mut bounds = range.split('-');
        let Some(start) = bounds.next().and_then(|s| u32::from_str_radix(s, 16).ok()) else { continue };
        let end = bounds.next().and_then(|s| u32::from_str_radix(s, 16).ok()).unwrap_or(start);
        for code in start..=end.min(start.saturating_add(64)) {
            if let Some(ch) = char::from_u32(code).filter(|c| !c.is_control() && !c.is_whitespace()) {
                out.push(ch);
                if out.chars().count() == 48 { return out; }
            }
        }
    }
    out
}
