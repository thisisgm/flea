// Directory ancestry and membership heads for initially contiguous, locally mutable blocks.
use super::searchnames::{slot, table, Names, Refusal, Seeded, EMPTY};
use std::hash::BuildHasher;

struct Directory {
    name: u32,
    parent: u32,
    text_len: u32,
    bits: u64,
    head: u32,
    child: u32,
    sibling: u32,
}

pub(crate) struct Blocks {
    dirs: Vec<Directory>,
    lookup: Vec<u32>,
    hash: Seeded,
    root: u32,
    child: u32,
}

impl Blocks {
    pub(crate) fn new() -> Self {
        Self { dirs: Vec::new(), lookup: Vec::new(), hash: Seeded::default(), root: EMPTY, child: EMPTY }
    }

    pub(crate) fn directory(&mut self, names: &Names, parent: u32, name: u32) -> Result<u32, Refusal> {
        let hash = self.hash.hash_one((parent, name));
        if !self.lookup.is_empty() {
            let at = slot(&self.lookup, hash, |id| {
                let dir = &self.dirs[id as usize];
                (dir.parent, dir.name) == (parent, name)
            });
            if self.lookup[at] != EMPTY {
                return Ok(self.lookup[at]);
            }
        }
        let id = u32::try_from(self.dirs.len()).ok().filter(|&id| id != EMPTY).ok_or("capacity")?;
        let text_len = self.text_len(parent).checked_add(u32::from(parent != EMPTY))
            .and_then(|len| len.checked_add(display_len(names.get(name)) as u32)).ok_or("capacity")?;
        if (self.dirs.len() + 1) * 4 > self.lookup.len() * 3 {
            let mut lookup = table(self.lookup.len().checked_mul(2).ok_or("capacity")?.max(16))?;
            for (id, dir) in self.dirs.iter().enumerate() {
                let at = slot(&lookup, self.hash.hash_one((dir.parent, dir.name)), |_| false);
                lookup[at] = id as u32;
            }
            self.lookup = lookup;
        }
        self.dirs.try_reserve(1).map_err(|_| "allocation")?;
        let at = slot(&self.lookup, hash, |_| false);
        let sibling = if parent == EMPTY { self.child } else { self.dirs[parent as usize].child };
        let bits = names.bits(name as usize) | super::searchbits::mask(b"/") | self.bits(parent);
        self.dirs.push(Directory { name, parent, text_len, bits, head: EMPTY, child: EMPTY, sibling });
        if parent == EMPTY { self.child = id; } else { self.dirs[parent as usize].child = id; }
        self.lookup[at] = id;
        Ok(id)
    }

    pub(crate) fn text_len(&self, dir: u32) -> u32 {
        if dir == EMPTY { 0 } else { self.dirs[dir as usize].text_len }
    }

    // Names and ancestry are immutable until reclamation builds a new directory arena.
    pub(crate) fn bits(&self, dir: u32) -> u64 {
        if dir == EMPTY { 0 } else { self.dirs[dir as usize].bits }
    }

    pub(crate) fn head(&self, dir: u32) -> u32 {
        if dir == EMPTY { self.root } else { self.dirs[dir as usize].head }
    }

    // The scope has no row; other directory identities live in their containing block.
    pub(crate) fn identity(&self, dir: u32) -> Option<(u32, u32)> {
        if dir == EMPTY { return None; }
        let dir = &self.dirs[dir as usize];
        Some((dir.parent, dir.name))
    }

    pub(crate) fn set_head(&mut self, dir: u32, row: u32) {
        if dir == EMPTY { self.root = row; } else { self.dirs[dir as usize].head = row; }
    }

    pub(crate) fn children(&self, dir: u32) -> Vec<u32> {
        let mut child = if dir == EMPTY { self.child } else { self.dirs[dir as usize].child };
        let mut children = Vec::new();
        while child != EMPTY {
            children.push(child);
            child = self.dirs[child as usize].sibling;
        }
        children
    }

    pub(crate) fn ancestors(&self, mut dir: u32, into: &mut Vec<u32>) {
        into.clear();
        while dir != EMPTY {
            let record = &self.dirs[dir as usize];
            into.push(record.name);
            dir = record.parent;
        }
    }

    pub(crate) fn len(&self) -> usize { self.dirs.len() }

    pub(crate) fn record(&self, dir: u32) -> (u32, u32) {
        let dir = &self.dirs[dir as usize];
        (dir.name, dir.parent)
    }

    pub(crate) fn bytes(&self, allocated: bool) -> usize {
        let count = |len, capacity| if allocated { capacity } else { len };
        count(self.dirs.len(), self.dirs.capacity()) * std::mem::size_of::<Directory>()
            + self.lookup.capacity() * std::mem::size_of::<u32>()
    }
}

// Counts the bytes from_utf8_lossy would produce without allocating its replacement string.
pub(crate) fn display_len(mut bytes: &[u8]) -> usize {
    let mut len = 0;
    loop {
        match std::str::from_utf8(bytes) {
            Ok(valid) => return len + valid.len(),
            Err(error) => {
                len += error.valid_up_to() + char::REPLACEMENT_CHARACTER.len_utf8();
                let Some(invalid) = error.error_len() else { return len };
                bytes = &bytes[error.valid_up_to() + invalid..];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{display_len, Blocks, Names, EMPTY};
    use std::hash::BuildHasher;

    #[test]
    fn search_blocks_probe_distance_stays_short_on_sequential_ids() {
        let mut names = Names::new();
        let mut blocks = Blocks::new();
        let mut parents = vec![EMPTY];
        for i in 0..30_000u32 {
            let name = names.intern(format!("d{}", i % 97).as_bytes()).unwrap();
            let parent = parents[i as usize / 8];
            parents.push(blocks.directory(&names, parent, name).unwrap());
        }
        let mask = blocks.lookup.len() - 1;
        let mut total = 0;
        for (at, &id) in blocks.lookup.iter().enumerate() {
            if id == EMPTY { continue; }
            let dir = &blocks.dirs[id as usize];
            let home = blocks.hash.hash_one((dir.parent, dir.name)) as usize & mask;
            total += (at + blocks.lookup.len() - home) & mask;
        }
        let mean = total as f64 / blocks.len() as f64;
        assert!(mean < 2.0, "mean probe distance {mean} on sequential directory ids: the low hash bits are weak");
    }

    #[test]
    fn search_display_length_matches_lossy_utf8_for_every_two_byte_input() {
        for first in 0..=u8::MAX {
            for second in 0..=u8::MAX {
                let bytes = [first, second];
                assert_eq!(display_len(&bytes), String::from_utf8_lossy(&bytes).len());
            }
        }
    }

    #[test]
    fn search_display_length_handles_valid_runs_and_truncated_invalid_sequences() {
        let cases: &[&[u8]] = &[
            b"", b"plain", "caf\u{e9}/\u{20000}".as_bytes(), b"a\xffb\xfec",
            b"\xe2\x82", b"a\xf0\x9f\x92", b"\xed\xa0\x80", b"\xf4\x90\x80\x80",
            b"\xe2\x82\xac\xe2\x82\xff\xf0\xa0\x80\x80\xf0\x9f",
        ];
        for bytes in cases {
            for end in 0..=bytes.len() {
                assert_eq!(display_len(&bytes[..end]), String::from_utf8_lossy(&bytes[..end]).len());
            }
        }
    }
}
