use super::job::Job;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq)]
pub enum Protocol {
    None,
    Sixel,
    Kitty,
}
pub struct Graphics {
    pub protocol: Protocol,
    job: Option<Job>,
    identity: PathBuf,
    pub bytes: Vec<u8>,
    pub error: String,
    pub columns: usize,
    pub rows: usize,
    pixels: (usize, usize),
}
impl Graphics {
    pub fn new() -> Self {
        Self {
            protocol: Protocol::None,
            job: None,
            identity: PathBuf::new(),
            bytes: Vec::new(),
            error: String::new(),
            columns: 0,
            rows: 0,
            pixels: (0, 0),
        }
    }
    pub fn request(&mut self, path: PathBuf, columns: usize, rows: usize, pixels: (usize, usize)) {
        if self.protocol == Protocol::None {
            return;
        }
        if self.identity == path
            && self.columns == columns
            && self.rows == rows
            && self.pixels == pixels
        {
            return;
        }
        self.clear();
        self.identity = path.clone();
        self.columns = columns;
        self.rows = rows;
        self.pixels = pixels;
        let format = if self.protocol == Protocol::Kitty {
            "png:-"
        } else {
            "sixel:-"
        };
        self.job = Some(Job::start(
            path,
            vec![
                "/usr/bin/magick".into(),
                "{input}".into(),
                "-resize".into(),
                format!("{}x{}", pixels.0, pixels.1),
                format.into(),
            ],
        ));
    }
    pub fn accept(&mut self) -> bool {
        let Some(job) = &self.job else {
            return false;
        };
        let Ok(result) = job.result.try_recv() else {
            return false;
        };
        self.job = None;
        match result {
            Ok(bytes) => {
                self.bytes = if self.protocol == Protocol::Kitty {
                    kitty(&bytes, self.columns, self.rows)
                } else {
                    bytes
                }
            }
            Err(error) => self.error = error,
        }
        true
    }
    pub fn clear(&mut self) {
        if self.protocol == Protocol::Kitty && (!self.bytes.is_empty() || !self.identity.as_os_str().is_empty()) {
            print!("\x1b_Ga=d,d=I,i=42,q=2\x1b\\");
        }
        self.job = None;
        self.bytes.clear();
        self.error.clear();
        self.identity = PathBuf::new();
    }
}
impl Drop for Graphics {
    fn drop(&mut self) {
        self.clear();
    }
}
const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk[0] as usize;
        let b = chunk.get(1).copied().unwrap_or(0) as usize;
        let c = chunk.get(2).copied().unwrap_or(0) as usize;
        out.push(BASE64[a >> 2] as char);
        out.push(BASE64[((a & 3) << 4) | (b >> 4)] as char);
        out.push(if chunk.len() > 1 {
            BASE64[((b & 15) << 2) | (c >> 6)] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            BASE64[c & 63] as char
        } else {
            '='
        });
    }
    out
}
pub fn kitty(bytes: &[u8], columns: usize, rows: usize) -> Vec<u8> {
    const CHUNK: usize = 4096;
    let encoded = base64(bytes);
    let mut out = String::new();
    let chunks = encoded.as_bytes().chunks(CHUNK);
    let count = chunks.len();
    for (i, chunk) in chunks.enumerate() {
        let more = usize::from(i + 1 < count);
        let header = if i == 0 {
            format!("a=T,f=100,i=42,c={},r={},q=2,m={}", columns, rows, more)
        } else {
            format!("m={}", more)
        };
        out.push_str(&format!(
            "\x1b_G{};{}\x1b\\",
            header,
            std::str::from_utf8(chunk).unwrap_or("")
        ));
    }
    out.into_bytes()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn kitty_encoding_is_bounded_and_exact() {
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert!(String::from_utf8(kitty(b"png", 10, 4))
            .unwrap()
            .contains("c=10,r=4,q=2,m=0"));
    }
}
