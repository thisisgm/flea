// Test-only: a program's execute bit through a copy and the two moves the ops run, read where each stage makes the file.
use crate::backend::copyfile::{copy_any, move_any};
use crate::backend::renamecompat::copy_then_remove;
use crate::backend::testdir::TestDir;
use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::AtomicBool;

fn quiet<'a>(flag: &'a AtomicBool, sink: &'a mut dyn FnMut(u64, u64)) -> crate::backend::copyfile::Progress<'a> {
    crate::backend::copyfile::Progress { cancel: flag, on_bytes: sink, partial: None, tree: None, manifest: None, durability: None, for_move: false }
}

// The owner execute bit survives a copy, a same-device move and the cross-device move a rename falls back to.
#[test]
fn a_program_keeps_its_owner_execute_bit_through_copy_and_move() {
    let d = TestDir::new("copyexecbit");
    let owner_x = |stage: &str, path: &std::path::Path| {
        let mode = std::fs::symlink_metadata(path).expect("the file exists").permissions().mode() & 0o7777;
        assert_ne!(mode & 0o100, 0, "the {stage} left the owner execute bit on the file (mode {mode:o})");
    };
    let src = d.file("Godot_linux.x86_64", "\x7fELF\x02\x01\x01");
    std::fs::set_permissions(&src, std::fs::Permissions::from_mode(0o755)).expect("the source is 0755");
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    owner_x("source", &src);
    let copied = d.join("copied");
    copy_any(&src, &copied, &mut quiet(&flag, &mut sink)).expect("the copy lands");
    owner_x("copy", &copied);
    let moved = d.join("moved");
    move_any(&copied, &moved, &mut quiet(&flag, &mut sink)).expect("the same-device move lands");
    owner_x("move", &moved);
    let crossed = d.join("crossed");
    copy_then_remove(&moved, &crossed).expect("the cross-device move lands");
    owner_x("crossdevice", &crossed);
}
