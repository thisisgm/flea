//! Copy As text: every variant covers the whole selection, one path per line.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Path,
    Relative,
    Name,
    Stem,
    Dirpath,
    Uri,
    Quoted,
}

impl Format {
    // Sample input: "copyStem" is Stem; the flyout leaf ids and the backend's format words both answer.
    pub fn parse(word: &str) -> Option<Self> {
        Some(match word {
            "copyPath" | "path" => Self::Path,
            "copyRelative" | "relative" => Self::Relative,
            "copyName" | "name" => Self::Name,
            "copyStem" | "stem" => Self::Stem,
            "copydirpath" | "dirpath" => Self::Dirpath,
            "copyUri" | "uri" => Self::Uri,
            "copyQuoted" | "quoted" => Self::Quoted,
            _ => return None,
        })
    }
}

// Sample input: "/home/gm/archive.tar.gz" is "archive.tar.gz".
pub fn leaf(path: &str) -> &str {
    path.rfind('/').map_or(path, |cut| &path[cut + 1..])
}

// Sample input: "archive.tar.gz" is "archive.tar"; a dotfile keeps its whole name.
pub fn stem(path: &str) -> &str {
    let name = leaf(path);
    let start = usize::from(name.starts_with('.'));
    match name.rfind('.') {
        Some(dot) if dot > start => &name[..dot],
        _ => name,
    }
}

// Sample input: "/home/gm/a.txt" is "/home/gm"; "/" stays "/".
pub fn dirpath(path: &str) -> &str {
    match path.rfind('/') {
        None => path,
        Some(0) => "/",
        Some(cut) => &path[..cut],
    }
}

// encodeURI's reserved and unreserved sets, then # and ? escaped so a name never reads as a fragment or query.
pub fn file_uri(path: &str) -> String {
    let mut out = String::from("file://");
    for &byte in
        std::os::unix::ffi::OsStrExt::as_bytes(crate::bytename::text_path(path).as_os_str())
    {
        let keep = byte.is_ascii_alphanumeric() || b";,/:@&=+$-_.!~*'()".contains(&byte);
        if keep {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

// Sample input: "/home/gm/proj/src/a.rs" from "/home/gm/proj" is "src/a.rs", "/home/gm/b.txt" from it is "../b.txt"; an empty base keeps the path.
pub fn relative(path: &str, base: &str) -> String {
    if base.is_empty() {
        return path.to_owned();
    }
    let to: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    let from: Vec<&str> = base.split('/').filter(|part| !part.is_empty()).collect();
    let shared = to.iter().zip(&from).take_while(|(a, b)| a == b).count();
    let mut walk = vec![".."; from.len() - shared];
    walk.extend_from_slice(&to[shared..]);
    if walk.is_empty() {
        ".".into()
    } else {
        walk.join("/")
    }
}

// Only Relative reads base; text forms read a name that is not UTF-8 by its lossy spelling, Uri and Quoted by its bytes.
pub fn one(path: &str, format: Format, base: &str) -> String {
    let (shown, base) = (
        crate::bytename::display(path),
        crate::bytename::display(base),
    );
    match format {
        Format::Path => shown.into_owned(),
        Format::Relative => relative(&shown, &base),
        Format::Name => leaf(&shown).to_owned(),
        Format::Stem => stem(&shown).to_owned(),
        Format::Dirpath => dirpath(&shown).to_owned(),
        Format::Uri => file_uri(path),
        Format::Quoted if crate::bytename::is_exact(path) => quoted_bytes(path),
        Format::Quoted => format!("'{}'", path.replace('\'', "'\\''")),
    }
}

// Sample input: the text of /d/col<0xff> answers $'/d/col\xff'; every byte outside printable ASCII is escaped.
fn quoted_bytes(path: &str) -> String {
    let mut out = String::from("$'");
    for &byte in
        std::os::unix::ffi::OsStrExt::as_bytes(crate::bytename::text_path(path).as_os_str())
    {
        match byte {
            b'\'' | b'\\' => {
                out.push('\\');
                out.push(byte as char);
            }
            0x20..=0x7e => out.push(byte as char),
            _ => out.push_str(&format!("\\x{byte:02x}")),
        }
    }
    out.push('\'');
    out
}

pub fn lines(paths: &[&str], format: Format, base: &str) -> String {
    paths
        .iter()
        .map(|path| one(path, format, base))
        .collect::<Vec<_>>()
        .join("\n")
}
