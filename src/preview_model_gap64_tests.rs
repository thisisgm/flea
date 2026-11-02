//! The production checked reader sees a cancellation raised after decoding has started.
use super::*;
use std::{
    cell::Cell,
    io::{Cursor, Read},
};

struct Trip<'a> {
    bytes: Cursor<Vec<u8>>,
    read: &'a Cell<usize>,
    cancel: &'a AtomicBool,
    trip: bool,
}

impl Read for Trip<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        let count = bytes.len().min(4);
        let read = self.bytes.read(&mut bytes[..count])?;
        self.read.set(self.read.get() + read);
        if self.trip && self.read.get() >= 8 {
            self.cancel.store(true, Ordering::Relaxed);
        }
        Ok(read)
    }
}

#[test]
fn sol_gap64_fl3d_cancel_during_decode_stops_the_next_underlying_read() {
    let mut bytes = b"FL3D\x01\0\0\0".to_vec();
    bytes.extend_from_slice(&2u32.to_le_bytes());
    bytes.extend_from_slice(b"{}");
    bytes.extend_from_slice(&[0; 40]);
    let dir = crate::backend::testdir::TestDir::new("gap64-fl3d-cancel");
    let path = dir.join("empty.fl3d");
    std::fs::write(&path, &bytes).unwrap();
    assert!(read_cancellable(&path, &AtomicBool::new(false)).is_ok());
    assert!(matches!(
        read_cancellable(&path, &AtomicBool::new(true)),
        Err(error) if error == "model render cancelled"
    ));
    for trip in [false, true] {
        let cancel = AtomicBool::new(false);
        let read = Cell::new(0);
        let input = Trip {
            bytes: Cursor::new(bytes.clone()),
            read: &read,
            cancel: &cancel,
            trip,
        };
        let result = decode(input, &cancel);
        if trip {
            assert_eq!(result.unwrap_err(), "model render cancelled");
            assert_eq!(
                read.get(),
                8,
                "no underlying read after in-progress cancellation"
            );
        } else {
            assert!(
                result.is_ok(),
                "the same valid model decodes without cancellation: {result:?}"
            );
            assert_eq!(read.get(), bytes.len());
        }
    }
}
