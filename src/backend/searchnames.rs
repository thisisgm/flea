// Interned raw basenames, with no retained full path or lossy identity key.
use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;

pub(crate) const SLAB: usize = 64 * 1024;
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
    hash: RandomState,
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
        Self { slabs: Vec::new(), refs: Vec::new(), lookup: Vec::new(), hash: RandomState::new(), groups: Vec::new() }
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
        if id % 64 == 0 { self.groups.try_reserve(1).map_err(|_| "allocation")?; }
        self.refs.try_reserve(1).map_err(|_| "allocation")?;
        if self.slabs.last().map_or(true, |slab| slab.len() + bytes.len() > SLAB) {
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
        if id % 64 == 0 { self.groups.push(0); }
        self.groups[id as usize / 64] |= bits;
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
            + count(self.slabs.len(), self.slabs.capacity()) * size_of::<Vec<u8>>()
            + count(self.refs.len(), self.refs.capacity()) * size_of::<Name>()
            + self.lookup.capacity() * size_of::<u32>()
            + count(self.groups.len(), self.groups.capacity()) * size_of::<u64>()
    }

    pub(crate) fn bits(&self, id: usize) -> u64 { self.refs[id].bits }

    pub(crate) fn group_bits(&self, group: usize) -> u64 { self.groups[group] }

    pub(crate) fn len(&self) -> usize {
        self.refs.len()
    }
}
