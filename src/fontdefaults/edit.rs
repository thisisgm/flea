use super::{alacritty, files, settings};
use std::path::{Path, PathBuf};

pub fn terminal(config: &Path, target: &str) -> PathBuf {
    let name = match target { "alacritty" => "alacritty.toml", "kitty" => "kitty.conf", "foot" => "foot.ini",
        "ghostty" if config.join("ghostty/config.ghostty").exists() => "config.ghostty", _ => "config" };
    config.join(target).join(name)
}
pub fn available(config: &Path, target: &str) -> bool {
    ["desktop", "serif", "monospace"].contains(&target) || terminal(config, target).exists() || files::executable(target)
}
fn assignment<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let (key, value) = line.trim().split_once('=')?;
    (key.trim() == name).then(|| value.trim())
}
fn section(line: &str) -> Option<&str> {
    line.trim().strip_prefix('[')?.split_once(']').map(|(name, _)| name)
}
fn current<'a>(text: &'a str, wanted: &str, name: &str) -> Option<&'a str> {
    let mut inside = "";
    for line in text.lines() {
        if let Some(header) = section(line) { inside = header; }
        if inside == wanted { if let Some(value) = assignment(line, name) { return Some(value); } }
    }
    None
}
fn key(text: &str, wanted: &str, name: &str, value: &str) -> String {
    let mut lines: Vec<String> = text.split_inclusive('\n').map(str::to_string).collect();
    let mut inside = "".to_string();
    let mut end = lines.len();
    let mut seen = wanted.is_empty();
    let mut found = false;
    for (i, line) in lines.iter_mut().enumerate() {
        if let Some(header) = section(line) {
            if inside == wanted { end = i; }
            inside = header.into();
            seen |= inside == wanted;
        }
        if inside == wanted && assignment(line, name).is_some() { *line = format!("{name}={value}\n"); found = true; }
    }
    if !found {
        if !seen { return format!("{}\n[{wanted}]\n{name}={value}\n", text.trim_end()); }
        // A last line without a newline must not swallow the new assignment.
        if end > 0 && !lines[end - 1].ends_with('\n') { lines[end - 1].push('\n'); }
        lines.insert(end, format!("{name}={value}\n"));
    }
    lines.concat()
}
fn quoted(family: &str) -> String { format!("\"{}\"", crate::json::escape(family)) }
fn xml(family: &str) -> String { family.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;") }
fn foot_suffix(value: &str) -> Option<&str> {
    let mut escaped = false;
    for (i, ch) in value.char_indices() {
        if !escaped && (ch == ':' || ch == ',') { return Some(&value[i..]); }
        escaped = !escaped && ch == '\\';
    }
    None
}
fn foot(text: &str, family: &str) -> String {
    let wanted = if text.lines().any(|l| section(l) == Some("main")) { "main" } else { "" };
    let escaped: String = family.chars().flat_map(|c| if "\\,:=-".contains(c) { vec!['\\', c] } else { vec![c] }).collect();
    let mut out = text.to_string();
    for name in ["font", "font-bold", "font-italic", "font-bold-italic"] {
        if let Some(old) = current(&out, wanted, name) {
            let next = format!("{}{}", escaped, foot_suffix(old).unwrap_or(":size=9"));
            out = key(&out, wanted, name, &next);
        } else if name == "font" { out = key(&out, wanted, name, &format!("{escaped}:size=9")); }
    }
    out
}
fn ghostty(text: &str, family: &str) -> String {
    let mut out = text.to_string();
    for name in ["font-family", "font-family-bold", "font-family-italic", "font-family-bold-italic"] {
        let mut found = false;
        out = out.split_inclusive('\n').map(|line| {
            if !found && assignment(line, name).is_some() { found = true; format!("{name} = {}\n", quoted(family)) }
            else { line.to_string() }
        }).collect();
        if !found && name == "font-family" { out = format!("{}\n{name} = {}\n", out.trim_end(), quoted(family)); }
    }
    out
}
fn kitty(text: &str, family: &str) -> String {
    let mut out: String = text.split_inclusive('\n').filter(|line| {
        !line.split_whitespace().next().map(|s| ["font_family", "bold_font", "italic_font", "bold_italic_font"].contains(&s)).unwrap_or(false)
    }).collect();
    out = format!("{}\nfont_family family={}\nbold_font auto\nitalic_font auto\nbold_italic_font auto\n", out.trim_end(), quoted(family));
    out
}
pub fn changes(config: &Path, target: &str, family: &str) -> Result<Vec<(PathBuf, String)>, String> {
    if ["desktop", "serif", "monospace"].contains(&target) {
        let alias = match target { "desktop" => "sans-serif", "serif" => "serif", _ => "monospace" };
        let content = format!("<?xml version=\"1.0\"?>\n<!DOCTYPE fontconfig SYSTEM \"urn:fontconfig:fonts.dtd\">\n<fontconfig><match target=\"pattern\"><test name=\"family\" qual=\"any\"><string>{alias}</string></test><edit name=\"family\" mode=\"assign_replace\" binding=\"strong\"><string>{}</string></edit></match></fontconfig>\n", xml(family));
        let mut out = vec![(config.join(format!("fontconfig/conf.d/99-flea-{alias}.conf")), content)];
        if target == "desktop" {
            for version in ["3.0", "4.0"] {
                let path = config.join(format!("gtk-{version}/settings.ini"));
                let text = files::read(&path)?.unwrap_or_default();
                let size = current(&text, "Settings", "gtk-font-name").map(settings::size).unwrap_or("11");
                let next = key(&text, "Settings", "gtk-font-name", &format!("{family} {size}"));
                out.push((path, next));
            }
        }
        return Ok(out);
    }
    let path = terminal(config, target);
    let text = files::read(&path)?.unwrap_or_default();
    let next = match target { "alacritty" => alacritty::edit(&text, family)?, "kitty" => kitty(&text, family),
        "ghostty" => ghostty(&text, family), "foot" => foot(&text, family), _ => return Err("Choose a supported destination.".into()) };
    Ok(vec![(path, next)])
}
