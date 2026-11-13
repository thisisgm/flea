// Interned raw basenames, with no retained full path or lossy identity key.
use std::hash::BuildHasher;

// Keyed per process and per table, so a directory of crafted names cannot force colliding probes.
pub(crate) type Seeded = foldhash::quality::RandomState;

pub(crate) const SLAB: usize = 64 * 1024;
// One u64 of letter bits covers this many consecutive name ids, so a query can skip a whole group.
const GROUP_NAMES: u32 = 64;
pub(crate) const EMPTY: u32 = u32::MAX;
pub(crate) type Refusal = &'static str;

#[derive(Clone, Copy)]
struct Name {
    off: u32,
    len: u32,
    bits: u64,
}

pub(crate) struct Names {
    slabs: Vec<Vec<u8>>,
    refs: Vec<Name>,
    lookup: Vec<u32>,
    hash: Seeded,
    groups: Vec<u64>,
}

// Fallible table allocation keeps allocation refusal distinguishable from a complete walk.
pub(crate) fn table(len: usize) -> Result<Vec<u32>, Refusal> {
    let mut slots = Vec::new();
    slots.try_reserve_exact(len).map_err(|_| "allocation")?;
    slots.resize(len, EMPTY);
    Ok(slots)
}

pub(crate) fn slot(slots: &[u32], hash: u64, same: impl Fn(u32) -> bool) -> usize {
    let mut at = hash as usize & (slots.len() - 1);
    while slots[at] != EMPTY && !same(slots[at]) {
        at = (at + 1) & (slots.len() - 1);
    }
    at
}

impl Names {
    pub(crate) fn new() -> Self {
        Self { slabs: Vec::new(), refs: Vec::new(), lookup: Vec::new(), hash: Seeded::default(), groups: Vec::new() }
    }

    pub(crate) fn get(&self, id: u32) -> &[u8] {
        let name = self.refs[id as usize];
        let off = name.off as usize;
        &self.slabs[off / SLAB][off % SLAB..off % SLAB + name.len as usize]
    }

    pub(crate) fn intern(&mut self, bytes: &[u8]) -> Result<u32, Refusal> {
        let hash = self.hash.hash_one(bytes);
        if !self.lookup.is_empty() {
            let at = slot(&self.lookup, hash, |id| self.get(id) == bytes);
            if self.lookup[at] != EMPTY {
                return Ok(self.lookup[at]);
            }
        }
        let id = u32::try_from(self.refs.len()).ok().filter(|&id| id != EMPTY).ok_or("capacity")?;
        if bytes.len() > SLAB {
            return Err("capacity");
        }
        if (self.refs.len() + 1) * 4 > self.lookup.len() * 3 {
            let len = self.lookup.len().checked_mul(2).ok_or("capacity")?.max(16);
            let mut lookup = table(len)?;
            for name in 0..self.refs.len() as u32 {
                let at = slot(&lookup, self.hash.hash_one(self.get(name)), |_| false);
                lookup[at] = name;
            }
            self.lookup = lookup;
        }
        if id % GROUP_NAMES == 0 { self.groups.try_reserve(1).map_err(|_| "allocation")?; }
        self.refs.try_reserve(1).map_err(|_| "allocation")?;
        if self.slabs.last().is_none_or(|slab| slab.len() + bytes.len() > SLAB) {
            self.slabs.try_reserve(1).map_err(|_| "allocation")?;
            let mut slab = Vec::new();
            slab.try_reserve_exact(SLAB).map_err(|_| "allocation")?;
            self.slabs.push(slab);
        }
        let slab = self.slabs.last().unwrap();
        let off = (self.slabs.len() - 1).checked_mul(SLAB).and_then(|off| off.checked_add(slab.len()))
            .and_then(|off| u32::try_from(off).ok()).ok_or("capacity")?;
        let at = slot(&self.lookup, hash, |_| false);
        let slab = self.slabs.last_mut().unwrap();
        slab.try_reserve_exact(SLAB - slab.len()).map_err(|_| "allocation")?;
        slab.extend_from_slice(bytes);
        let bits = super::searchbits::mask(bytes);
        self.refs.push(Name { off, len: bytes.len() as u32, bits });
        if id % GROUP_NAMES == 0 { self.groups.push(0); }
        self.groups[(id / GROUP_NAMES) as usize] |= bits;
        self.lookup[at] = id;
        Ok(id)
    }

    pub(crate) fn finish(&mut self) {
        if let Some(slab) = self.slabs.last_mut() {
            slab.shrink_to_fit();
        }
    }

    // Logical bytes include interning slots, slab descriptors and all slab slack.
    pub(crate) fn bytes(&self, allocated: bool) -> usize {
        let count = |len, capacity| if allocated { capacity } else { len };
        self.slabs.iter().map(|slab| slab.capacity()).sum::<usize>()
            + count(self.slabs.len(), self.slabs.capacity()) * std::mem::size_of::<Vec<u8>>()
            + count(self.refs.len(), self.refs.capacity()) * std::mem::size_of::<Name>()
            + self.lookup.capacity() * std::mem::size_of::<u32>()
            + count(self.groups.len(), self.groups.capacity()) * std::mem::size_of::<u64>()
    }

    pub(crate) fn bits(&self, id: usize) -> u64 { self.refs[id].bits }

    pub(crate) fn group_bits(&self, id: u32) -> u64 { self.groups[(id / GROUP_NAMES) as usize] }

    pub(crate) fn len(&self) -> usize {
        self.refs.len()
    }
}

#[cfg(test)]
mod tests {
    use super::{Names, Seeded, EMPTY};
    use std::hash::BuildHasher;

    // Mean linear-probe distance of every interned name from its home slot.
    fn mean_probe(names: &Names) -> f64 {
        let mask = names.lookup.len() - 1;
        let mut total = 0;
        for (at, &id) in names.lookup.iter().enumerate() {
            if id == EMPTY { continue; }
            let home = names.hash.hash_one(names.get(id)) as usize & mask;
            total += (at + names.lookup.len() - home) & mask;
        }
        total as f64 / names.len() as f64
    }

    #[test]
    fn search_names_hash_is_seeded_foldhash_per_table() {
        let (a, b): (Seeded, Seeded) = (Names::new().hash, Names::new().hash);
        let differs = (0..64u32).any(|i| a.hash_one(i.to_le_bytes()) != b.hash_one(i.to_le_bytes()));
        assert!(differs, "two tables share one key, so one crafted directory collides in every index");
    }

    #[test]
    fn search_names_probe_distance_stays_short_on_fixture_shaped_names() {
        let mut names = Names::new();
        for i in 0..100_000u32 {
            names.intern(format!("file-{i:06}.txt").as_bytes()).unwrap();
        }
        for i in 0..3_000u32 {
            names.intern(format!("d{i}").as_bytes()).unwrap();
        }
        let mean = mean_probe(&names);
        assert!(mean < 2.0, "mean probe distance {mean} on sequential names: the low hash bits are weak");
    }
}
