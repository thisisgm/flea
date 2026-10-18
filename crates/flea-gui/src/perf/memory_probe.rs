//! Offline memory accounting for the native probe; never called from a render path.
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Memory {
    pub rss_kib: u64,
    pub pss_kib: u64,
    pub private_clean_kib: u64,
    pub private_dirty_kib: u64,
    pub private_hugetlb_kib: u64,
}

impl Memory {
    // Hugetlb is separate because Linux excludes it from RSS and PSS.
    pub fn uss_kib(self) -> u64 {
        self.private_clean_kib + self.private_dirty_kib
    }

    pub fn checked_add(self, other: Self) -> Result<Self, String> {
        let add = |a: u64, b| {
            a.checked_add(b)
                .ok_or_else(|| "memory sum overflow".to_string())
        };
        let sum = Self {
            rss_kib: add(self.rss_kib, other.rss_kib)?,
            pss_kib: add(self.pss_kib, other.pss_kib)?,
            private_clean_kib: add(self.private_clean_kib, other.private_clean_kib)?,
            private_dirty_kib: add(self.private_dirty_kib, other.private_dirty_kib)?,
            private_hugetlb_kib: add(self.private_hugetlb_kib, other.private_hugetlb_kib)?,
        };
        sum.private_clean_kib
            .checked_add(sum.private_dirty_kib)
            .ok_or("USS overflow")?;
        Ok(sum)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Mapping {
    pub permissions: String,
    pub path: String,
    pub memory: Memory,
}

impl Mapping {
    // These are mapping classes, not claims about which allocator owns a page.
    pub fn class(&self, executable: &str) -> String {
        let path = self.path.strip_suffix(" (deleted)").unwrap_or(&self.path);
        let executable = executable.strip_suffix(" (deleted)").unwrap_or(executable);
        if !executable.is_empty() && path == executable {
            return format!("executable:{}", self.permissions);
        }
        match path {
            "[heap]" => "heap".into(),
            "[stack]" => "stack".into(),
            "" => "anonymous".into(),
            _ if path.starts_with("[anon:") => format!("anonymous:{path}"),
            _ if path.starts_with("/dev/dri/") => format!("drm-mapping:{path}"),
            _ if path.starts_with("/memfd:") || path.starts_with("memfd:") => {
                format!("memfd:{path}")
            }
            _ if [".ttf", ".otf", ".ttc"]
                .iter()
                .any(|suffix| path.ends_with(suffix)) =>
            {
                format!("font:{path}")
            }
            _ => format!("object:{path}"),
        }
    }
}

#[derive(Default)]
struct Fields {
    memory: Memory,
    seen: u8,
}

impl Fields {
    fn line(&mut self, line: &str) -> Result<(), String> {
        let Some((key, rest)) = line.split_once(':') else {
            return Err(format!("invalid smaps field: {line}"));
        };
        let (bit, value) = match key {
            "Rss" => (1, &mut self.memory.rss_kib),
            "Pss" => (2, &mut self.memory.pss_kib),
            "Private_Clean" => (4, &mut self.memory.private_clean_kib),
            "Private_Dirty" => (8, &mut self.memory.private_dirty_kib),
            "Private_Hugetlb" => (16, &mut self.memory.private_hugetlb_kib),
            _ => return Ok(()),
        };
        if self.seen & bit != 0 {
            return Err(format!("duplicate smaps field: {key}"));
        }
        let mut words = rest.split_whitespace();
        *value = words
            .next()
            .ok_or("missing memory value")?
            .parse::<u64>()
            .map_err(|_| format!("invalid smaps value: {key}"))?;
        if words.next() != Some("kB") || words.next().is_some() {
            return Err(format!("invalid smaps unit: {key}"));
        }
        self.seen |= bit;
        Ok(())
    }

    fn finish(self) -> Result<Memory, String> {
        if self.seen & 15 != 15 {
            return Err("incomplete smaps record".into());
        }
        self.memory
            .private_clean_kib
            .checked_add(self.memory.private_dirty_kib)
            .ok_or("USS overflow")?;
        Ok(self.memory)
    }
}

fn header(line: &str) -> Result<(&str, &str), String> {
    let mut rest = line;
    let mut fields = Vec::with_capacity(5);
    for _ in 0..5 {
        rest = rest.trim_start();
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        fields.push(&rest[..end]);
        rest = &rest[end..];
    }
    let (start, end) = fields[0].split_once('-').ok_or("invalid mapping range")?;
    let start = u64::from_str_radix(start, 16).map_err(|_| "invalid mapping start")?;
    let end = u64::from_str_radix(end, 16).map_err(|_| "invalid mapping end")?;
    if start >= end || fields[1].len() != 4 || fields[4].parse::<u64>().is_err() {
        return Err("invalid mapping header".into());
    }
    Ok((fields[1], rest.trim_start()))
}

pub fn parse_smaps(text: &str) -> Result<Vec<Mapping>, String> {
    let mut mappings = Vec::new();
    let mut current: Option<(String, String, Fields)> = None;
    for line in text.lines().filter(|line| !line.is_empty()) {
        if line.as_bytes()[0].is_ascii_hexdigit()
            && line
                .split_whitespace()
                .next()
                .is_some_and(|word| word.contains('-'))
        {
            let (permissions, path) = header(line)?;
            if let Some((permissions, path, fields)) = current.take() {
                mappings.push(Mapping {
                    permissions,
                    path,
                    memory: fields.finish()?,
                });
            }
            current = Some((permissions.into(), path.into(), Fields::default()));
        } else {
            current
                .as_mut()
                .ok_or("smaps field without a mapping")?
                .2
                .line(line)?;
        }
    }
    let (permissions, path, fields) = current.ok_or("empty smaps")?;
    mappings.push(Mapping {
        permissions,
        path,
        memory: fields.finish()?,
    });
    Ok(mappings)
}

pub fn parse_rollup(text: &str) -> Result<Memory, String> {
    let mut lines = text.lines();
    let (_, path) = header(lines.next().ok_or("empty rollup")?)?;
    if path != "[rollup]" {
        return Err("missing rollup header".into());
    }
    let mut fields = Fields::default();
    for line in lines {
        fields.line(line)?;
    }
    fields.finish()
}

pub fn account(mappings: &[Mapping], executable: &str) -> Result<BTreeMap<String, Memory>, String> {
    let mut classes = BTreeMap::<String, Memory>::new();
    for mapping in mappings {
        let memory = classes.entry(mapping.class(executable)).or_default();
        *memory = memory.checked_add(mapping.memory)?;
    }
    Ok(classes)
}

// Every mapped page belongs to one class; rollup sampling drift stays visible to the caller.
pub fn total(classes: &BTreeMap<String, Memory>) -> Result<Memory, String> {
    classes
        .values()
        .try_fold(Memory::default(), |sum, memory| sum.checked_add(*memory))
}
