use super::{xml::Node, Result};
use std::mem::size_of;

// The measured pre-XML mapping is 105 MiB excluding archive bytes; keep metadata and allocator headroom.
const PROCESS_STORAGE: usize = 128 * 1024 * 1024;
// A one-million-node Vec can transiently retain its old 48 MiB allocation while growing.
const XML_GROWTH_STORAGE: usize = 64 * 1024 * 1024;
// Compact extraction needs at most one document's 36 MiB of faces plus vertices; attributes are released as read.
const GEOMETRY_STORAGE: usize = 48 * 1024 * 1024;
// The binary encoder uses the default 8 KiB BufWriter after all XML documents have been dropped.
const WIRE_STORAGE: usize = 8 * 1024;
const SCRATCH_STORAGE: usize = if XML_GROWTH_STORAGE > GEOMETRY_STORAGE {
    XML_GROWTH_STORAGE
} else {
    GEOMETRY_STORAGE
};
const MAX_XML_STORAGE: usize = crate::backend::sandbox::ADDRESS_SPACE_BYTES as usize
    - PROCESS_STORAGE
    - SCRATCH_STORAGE
    - WIRE_STORAGE;
// Bounds a glibc heap chunk (one size word, 16-byte rounding, 32-byte minimum); mmap page rounding sits in PROCESS_STORAGE.
const ALLOCATION_ALIGNMENT: usize = 2 * size_of::<usize>();
const ALLOCATION_HEADER: usize = 2 * size_of::<usize>();
const MIN_ALLOCATION: usize = 4 * size_of::<usize>();
// Rust grows a String from 8 bytes and these vectors from 4 slots, doubling after that.
const MIN_STRING_CAPACITY: usize = 8;
const MIN_VECTOR_CAPACITY: usize = 4;
// Eleven pairs fit inside the previous reader's map leaf; keep the 9-11 range within that footprint.
const SMALL_ATTRIBUTES: usize = 11;
const ATTRIBUTE_CAPACITIES: [usize; 6] = [4, 8, SMALL_ATTRIBUTES, 16, 32, 64];

fn allocation(bytes: usize) -> usize {
    if bytes == 0 {
        return 0;
    }
    (bytes + ALLOCATION_HEADER)
        .next_multiple_of(ALLOCATION_ALIGNMENT)
        .max(MIN_ALLOCATION)
}

pub(super) struct Budget {
    remaining: usize,
    retained: usize,
    exhausted: bool,
    limited: bool,
}

impl Budget {
    pub fn new() -> Self {
        Self {
            remaining: MAX_XML_STORAGE,
            retained: 0,
            exhausted: false,
            limited: true,
        }
    }
    pub fn unlimited() -> Self {
        Self {
            remaining: usize::MAX,
            retained: 0,
            exhausted: false,
            limited: false,
        }
    }
    pub fn retained(&mut self, bytes: usize) {
        self.retained = bytes;
    }
    pub fn begin_document(&mut self) {
        self.remaining = if self.limited {
            MAX_XML_STORAGE.saturating_sub(self.retained)
        } else {
            usize::MAX
        };
    }
    pub fn exhausted(&self) -> bool {
        self.exhausted
    }

    pub fn take(&mut self, bytes: usize) -> Result<()> {
        let Some(remaining) = self.remaining.checked_sub(bytes) else {
            self.exhausted = true;
            return Err("XML storage exceeds its bound".into());
        };
        self.remaining = remaining;
        Ok(())
    }

    pub fn string(&mut self, source: &str) -> Result<String> {
        self.take(allocation(source.len()))?;
        Ok(source.into())
    }

    pub fn push(&mut self, target: &mut String, value: &str) -> Result<()> {
        let needed = target.len() + value.len();
        if needed > target.capacity() {
            let capacity = needed.max(target.capacity() * 2).max(MIN_STRING_CAPACITY);
            self.take(allocation(capacity) - allocation(target.capacity()))?;
        }
        target.push_str(value);
        Ok(())
    }

    pub fn release_string(&mut self, value: &String) {
        self.remaining = self.remaining.saturating_add(allocation(value.capacity()));
    }

    pub fn node(&mut self, nodes: &Vec<Node>) -> Result<()> {
        self.vector(nodes.len(), nodes.capacity(), size_of::<Node>())
    }

    pub fn child(&mut self, children: &Vec<usize>) -> Result<()> {
        self.vector(children.len(), children.capacity(), size_of::<usize>())
    }

    fn vector(&mut self, len: usize, capacity: usize, width: usize) -> Result<()> {
        if len == capacity {
            let next = (capacity * 2).max(MIN_VECTOR_CAPACITY);
            self.take(allocation(next * width) - allocation(capacity * width))?;
        }
        Ok(())
    }

    pub fn attribute(&mut self, attrs: &mut Vec<(String, String)>) -> Result<()> {
        let capacity = attrs.capacity();
        if attrs.len() == capacity {
            let next = ATTRIBUTE_CAPACITIES
                .into_iter()
                .find(|&step| step > capacity)
                .unwrap_or(capacity * 2);
            let width = size_of::<(String, String)>();
            self.take(allocation(next * width) - allocation(capacity * width))?;
            attrs.reserve_exact(next - attrs.len());
        }
        Ok(())
    }
}
