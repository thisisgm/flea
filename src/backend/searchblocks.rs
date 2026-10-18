// Directory ancestry and contiguous runs of entries, independent of reconstructed paths.
use super::searchnames::{slot, table, Names, Refusal, EMPTY};
use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;

struct Directory {
    name: u32,
    parent: u32,
    text_len: u32,
}

struct Block {
    parent: u32,
    first: u32,
    count: u32,
}

pub(crate) struct Blocks {
    dirs: Vec<Directory>,
    lookup: Vec<u32>,
    runs: Vec<Block>,
    hash: RandomState,
}

impl Blocks {
    pub(crate) fn new() -> Self {
        Self { dirs: Vec::new(), lookup: Vec::new(), runs: Vec::new(), hash: RandomState::new() }
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
        self.dirs.push(Directory { name, parent, text_len });
        self.lookup[at] = id;
        Ok(id)
    }

    pub(crate) fn append(&mut self, parent: u32, row: u32) -> Result<(), Refusal> {
        if let Some(run) = self.runs.last_mut() {
            if run.parent == parent && run.first.checked_add(run.count) == Some(row) {
                run.count = run.count.checked_add(1).ok_or("capacity")?;
                return Ok(());
            }
        }
        self.runs.try_reserve(1).map_err(|_| "allocation")?;
        self.runs.push(Block { parent, first: row, count: 1 });
        Ok(())
    }

    pub(crate) fn text_len(&self, dir: u32) -> u32 {
        if dir == EMPTY { 0 } else { self.dirs[dir as usize].text_len }
    }

    pub(crate) fn ancestors(&self, mut dir: u32, into: &mut Vec<u32>) {
        into.clear();
        while dir != EMPTY {
            let record = &self.dirs[dir as usize];
            into.push(record.name);
            dir = record.parent;
        }
    }

    pub(crate) fn bytes(&self, allocated: bool) -> usize {
        let count = |len, capacity| if allocated { capacity } else { len };
        count(self.dirs.len(), self.dirs.capacity()) * size_of::<Directory>()
            + self.lookup.capacity() * size_of::<u32>()
            + count(self.runs.len(), self.runs.capacity()) * size_of::<Block>()
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
    use super::display_len;

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
