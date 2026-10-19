//! Copy As text: every variant covers the whole selection, one path per line.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Path,
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
    for byte in path.bytes() {
        let keep = byte.is_ascii_alphanumeric() || b";,/:@&=+$-_.!~*'()".contains(&byte);
        if keep {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

pub fn one(path: &str, format: Format) -> String {
    match format {
        Format::Path => path.to_owned(),
        Format::Name => leaf(path).to_owned(),
        Format::Stem => stem(path).to_owned(),
        Format::Dirpath => dirpath(path).to_owned(),
        Format::Uri => file_uri(path),
        Format::Quoted => format!("'{}'", path.replace('\'', "'\\''")),
    }
}

pub fn lines(paths: &[&str], format: Format) -> String {
    paths
        .iter()
        .map(|path| one(path, format))
        .collect::<Vec<_>>()
        .join("\n")
}
