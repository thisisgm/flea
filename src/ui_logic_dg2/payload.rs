//! The drag marker and the uri-list and plain-text codecs a drop reads (Drag.js markerPayload, DragOut.js filePath and sources).

/// What a lift bakes into the drag: sender, rows, lift keys, source folder and its device, deletability and listing numbering.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Marker {
    pub instance: String,
    pub rows: Vec<usize>,
    pub copy: bool,
    pub source: String,
    pub dev: u64,
    pub deletable: bool,
    pub shift: bool,
    pub listing: u64,
}

impl Marker {
    // Sample output: "me\n1,3\nmove\n%2Fhome%2Fu\n42\n1\n0\n7", the eight-field form Drag.markerPayload writes.
    pub fn encode(&self) -> String {
        let rows = self
            .rows
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            self.instance,
            rows,
            if self.copy { "copy" } else { "move" },
            encode_component(&self.source),
            self.dev,
            if self.deletable { "1" } else { "0" },
            if self.shift { "1" } else { "0" },
            self.listing
        )
    }

    // Five- and six-field legacy markers carry a raw source; seven and eight percent-encode it; anything else is None.
    pub fn parse(payload: &str) -> Option<Self> {
        let fields: Vec<&str> = payload.split('\n').collect();
        if !matches!(fields.len(), 5..=8) {
            return None;
        }
        let dev = fields[4];
        let signed = dev.strip_prefix('-').unwrap_or(dev);
        if signed.is_empty() || !signed.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let copy = match fields[2] {
            "copy" => true,
            "move" => false,
            _ => return None,
        };
        let flag = |at: usize| match fields.get(at) {
            None => Some(None),
            Some(&"0") => Some(Some(false)),
            Some(&"1") => Some(Some(true)),
            Some(_) => None,
        };
        let deletable = flag(5)?.unwrap_or(true);
        let shift = flag(6)?.unwrap_or(false);
        let source = if fields.len() >= 7 {
            decode_component(fields[3])?
        } else {
            fields[3].to_owned()
        };
        let listing = match fields.get(7) {
            None => 0,
            Some(text) if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) => {
                text.parse().ok()?
            }
            Some(_) => return None,
        };
        // A negative or unreadable device is unknown, as Number(...) || 0 reads it.
        let dev = if dev.starts_with('-') {
            0
        } else {
            dev.parse().unwrap_or(0)
        };
        Some(Self {
            instance: fields[0].to_owned(),
            rows: fields[1]
                .split(',')
                .filter_map(|row| row.parse().ok())
                .collect(),
            copy,
            source,
            dev,
            deletable,
            shift,
            listing,
        })
    }
}

/// Whether a marker text is absent or well formed; a malformed one refuses every drop (Drag.canDropInto).
pub fn marker_ok(marker: Option<&str>) -> bool {
    // map_or because is_none_or needs Rust 1.82 over the 1.77 floor.
    marker.map_or(true, |text| Marker::parse(text).is_some())
}

// encodeURIComponent: unreserved ASCII and !'()*~ stay, every other byte is %XX.
fn encode_component(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

// decodeURIComponent: a bad escape or invalid UTF-8 is None.
fn decode_component(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let hex = std::str::from_utf8(bytes.get(at + 1..at + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// A path as a file:// URI, each component encoded on its own (Drag.uriFor).
pub fn uri_for(path: &str) -> String {
    let parts: Vec<String> = path.split('/').map(encode_component).collect();
    format!("file://{}", parts.join("/"))
}

/// Sample input "file:///d/a%20b.txt" answers "/d/a b.txt"; another host or scheme answers None (DragOut.filePath).
pub fn file_path(url: &str) -> Option<String> {
    let rest = url.strip_prefix("file://")?;
    let path = if rest.starts_with('/') {
        rest
    } else {
        let slash = rest.find('/')?;
        if &rest[..slash] != "localhost" {
            return None;
        }
        &rest[slash..]
    };
    decode_component(path)
}

pub fn file_paths(urls: &[String]) -> Vec<String> {
    urls.iter().filter_map(|url| file_path(url)).collect()
}

/// The uri-list lines of a drop, comments and blanks left out; the paths come from file_paths.
pub fn uri_lines(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes)
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

/// Sample input "/a/x.txt\nhttp://e/y\n" answers ["/a/x.txt"] (DragOut.plainPaths).
pub fn plain_paths(plain: &str) -> Vec<String> {
    plain
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .filter(|line| line.starts_with('/'))
        .map(str::to_owned)
        .collect()
}

/// The local paths a drop carries; plain text adds lines only to a drag without Flea's marker (DragOut.sources).
pub fn sources(urls: &[String], plain: Option<&str>, has_marker: bool) -> Vec<String> {
    let mut paths = file_paths(urls);
    if !has_marker {
        for extra in plain.map(plain_paths).unwrap_or_default() {
            if !paths.contains(&extra) {
                paths.push(extra);
            }
        }
    }
    paths
}
