// One operation budget covers capture buffers, snapshot allocations, parsing and sorting.
use std::cell::Cell;
use std::cmp::Ordering;
use std::mem::size_of;
use std::time::Instant;

pub const ALLOCATION_LIMIT: usize = 64 * 1024 * 1024;
const ALLOCATION_OVERHEAD: usize = 64;
const CHECK_CHUNK_BYTES: usize = 4096;

pub struct Budget<'a> {
    used: Cell<usize>,
    limit: usize,
    deadline: Instant,
    cancelled: &'a dyn Fn() -> bool,
}

impl<'a> Budget<'a> {
    pub fn new(deadline: Instant, cancelled: &'a dyn Fn() -> bool) -> Self {
        Self {
            used: Cell::new(0),
            limit: ALLOCATION_LIMIT,
            deadline,
            cancelled,
        }
    }

    pub fn check(&self) -> Result<(), String> {
        if (self.cancelled)() {
            Err("Git status was cancelled.".into())
        } else if Instant::now() >= self.deadline {
            Err("Git status timed out.".into())
        } else {
            Ok(())
        }
    }

    pub fn claim(&self, bytes: usize) -> Result<(), String> {
        self.check()?;
        let used = self
            .used
            .get()
            .checked_add(bytes)
            .filter(|used| *used <= self.limit);
        match used {
            Some(used) => {
                self.used.set(used);
                Ok(())
            }
            None => Err("Git status snapshot is too large.".into()),
        }
    }

    // Cumulative charges retain old storage, bounding even a reallocation peak conservatively.
    pub fn reserve<T>(&self, values: &mut Vec<T>, extra: usize) -> Result<(), String> {
        self.check()?;
        let needed = values
            .len()
            .checked_add(extra)
            .ok_or("Git status snapshot is too large.")?;
        if needed <= values.capacity() {
            return Ok(());
        }
        let next = needed.max(values.capacity().saturating_mul(2)).max(4);
        let charge = next
            .checked_mul(size_of::<T>())
            .and_then(|n| n.checked_add(ALLOCATION_OVERHEAD))
            .ok_or("Git status snapshot is too large.")?;
        self.claim(charge)?;
        values
            .try_reserve_exact(next - values.len())
            .map_err(|_| "Git status could not allocate its snapshot.")?;
        if values.capacity() > next {
            return Err("Git status could not allocate its snapshot.".into());
        }
        Ok(())
    }

    pub fn copy(&self, bytes: &[u8]) -> Result<Vec<u8>, String> {
        let mut result = Vec::new();
        self.reserve(&mut result, bytes.len())?;
        for chunk in bytes.chunks(CHECK_CHUNK_BYTES) {
            self.check()?;
            result.extend_from_slice(chunk);
        }
        Ok(result)
    }

    pub fn branch(&self, bytes: &[u8]) -> Result<String, String> {
        let capacity = bytes
            .len()
            .checked_mul(3)
            .ok_or("Git status snapshot is too large.")?;
        let mut text = Vec::new();
        self.reserve(&mut text, capacity)?;
        let mut remaining = bytes;
        while !remaining.is_empty() {
            self.check()?;
            match std::str::from_utf8(remaining) {
                Ok(valid) => {
                    text.extend_from_slice(valid.as_bytes());
                    break;
                }
                Err(error) => {
                    let end = error.valid_up_to();
                    text.extend_from_slice(&remaining[..end]);
                    text.extend_from_slice("\u{fffd}".as_bytes());
                    remaining =
                        &remaining[end + error.error_len().unwrap_or(remaining.len() - end)..];
                }
            }
        }
        self.check()?;
        Ok(String::from_utf8(text).expect("only validated UTF-8 and replacement characters"))
    }
}

// Heapsort uses no scratch allocation and can stop between comparisons and swaps.
pub fn sort<T>(
    values: &mut [T],
    budget: &Budget<'_>,
    compare: impl Fn(&T, &T) -> Ordering,
) -> Result<(), String> {
    fn sift<T>(
        values: &mut [T],
        mut root: usize,
        end: usize,
        budget: &Budget<'_>,
        compare: &impl Fn(&T, &T) -> Ordering,
    ) -> Result<(), String> {
        while let Some(mut child) = root
            .checked_mul(2)
            .and_then(|n| n.checked_add(1))
            .filter(|child| *child < end)
        {
            budget.check()?;
            if child + 1 < end && compare(&values[child], &values[child + 1]).is_lt() {
                child += 1;
            }
            if !compare(&values[root], &values[child]).is_lt() {
                break;
            }
            values.swap(root, child);
            root = child;
        }
        Ok(())
    }
    for root in (0..values.len() / 2).rev() {
        sift(values, root, values.len(), budget, &compare)?;
    }
    for end in (1..values.len()).rev() {
        budget.check()?;
        values.swap(0, end);
        sift(values, 0, end, budget, &compare)?;
    }
    budget.check()
}

pub struct Records<'a, 'b, 'c> {
    remaining: &'a [u8],
    budget: &'b Budget<'c>,
}

impl<'a, 'b, 'c> Records<'a, 'b, 'c> {
    pub fn new(bytes: &'a [u8], budget: &'b Budget<'c>) -> Self {
        Self {
            remaining: bytes,
            budget,
        }
    }

    // Sample input: b"# branch.head main\0? src/lib.rs\0".
    pub fn next(&mut self) -> Result<Option<&'a [u8]>, String> {
        let mut offset = 0;
        for chunk in self.remaining.chunks(CHECK_CHUNK_BYTES) {
            self.budget.check()?;
            if let Some(end) = chunk.iter().position(|byte| *byte == 0) {
                let record = &self.remaining[..offset + end];
                self.remaining = &self.remaining[offset + end + 1..];
                return Ok(Some(record));
            }
            offset += chunk.len();
        }
        if self.remaining.is_empty() {
            Ok(None)
        } else {
            Err("Git status returned a truncated record.".into())
        }
    }
}

#[cfg(test)]
#[path = "gitbudget_tests.rs"]
mod tests;
