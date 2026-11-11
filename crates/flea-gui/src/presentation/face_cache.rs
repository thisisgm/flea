//! The resolved monospace family, kept in Flea's state directory keyed by the fontconfig inputs, so a launch spends no fc-match time until an input changes.
use super::face;
use flea::backend::regfile::open_if_regular;
use std::{
    io::{self, Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::Path,
    process::Command,
};

const MAGIC: &str = "flea-fc-match 1";

/// FNV-1a over each input's presence, length and bytes: a changed, added or removed fontconfig file changes the key.
fn key(inputs: &[Option<String>]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut feed = |byte: u8| hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    for input in inputs {
        feed(u8::from(input.is_some()));
        if let Some(text) = input {
            text.len().to_le_bytes().into_iter().for_each(&mut feed);
            text.bytes().for_each(&mut feed);
        }
    }
    hash
}

fn lookup(path: &Path, key: u64) -> Option<String> {
    let mut text = String::new();
    open_if_regular(path, 0)
        .ok()?
        .take(8192)
        .read_to_string(&mut text)
        .ok()?;
    let mut lines = text.lines();
    let header = lines.next()?.strip_prefix(MAGIC)?.trim();
    let family = lines.next()?.trim();
    (u64::from_str_radix(header, 16).ok()? == key && !family.is_empty() && lines.next().is_none())
        .then(|| family.to_owned())
}

/// An exclusive temporary file renamed into place, so a crash or a second launch never leaves a half-written record.
fn store(path: &Path, key: u64, family: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
    }
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    let _ = std::fs::remove_file(&temporary);
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .and_then(|mut file| file.write_all(format!("{MAGIC} {key:016x}\n{family}\n").as_bytes()))
        .and_then(|()| std::fs::rename(&temporary, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    written
}

/// The cached family when the inputs match, else one bounded probe whose answer is cached. A failed store is logged, never fatal.
#[cfg(test)]
pub(super) fn family(
    inputs: &[Option<String>],
    cache: &Path,
    command: impl FnOnce() -> Command,
) -> io::Result<String> {
    family_within(inputs, cache, command, face::PATIENT)
}

pub(super) fn family_within(
    inputs: &[Option<String>],
    cache: &Path,
    command: impl FnOnce() -> Command,
    deadline: std::time::Duration,
) -> io::Result<String> {
    let key = key(inputs);
    if let Some(family) = lookup(cache, key) {
        return Ok(family);
    }
    let family = face::probe_within(command(), deadline)?;
    if let Err(error) = store(cache, key, &family) {
        eprintln!("flea-gui: font cache {}: {error}", cache.display());
    }
    Ok(family)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::fixture::{Fixture, SHELL_GUARD};

    fn probe(
        fixture: &Fixture,
        cache: &Path,
        count: &Path,
        answer: &str,
    ) -> impl FnOnce() -> Command {
        fixture.check(cache).unwrap();
        fixture.check(count).unwrap();
        let root = fixture.path().to_owned();
        let count = count.to_owned();
        let script =
            format!("{SHELL_GUARD}\nguard \"$1\" \"$2\" \"$3\"\necho x >> \"$2\"\n{answer}");
        move || {
            let mut command = Command::new("/bin/sh");
            command.args(["-c", &script, "probe"]);
            command.arg(root).arg(count).arg(Fixture::parent());
            command
        }
    }

    fn spawned(count: &Path) -> usize {
        std::fs::read_to_string(count)
            .unwrap_or_default()
            .lines()
            .count()
    }

    #[test]
    fn u01_the_family_is_probed_once_per_font_input_and_failures_are_never_cached() {
        let fixture = Fixture::new("cache");
        let dir = fixture.path();
        let cache = dir.join("state/flea/fc-match-monospace");
        let count = dir.join("count");
        fixture.check(&cache).unwrap();
        let fonts = |text: &str| [Some(text.to_owned()), None];
        // A miss runs the probe once and stores its answer.
        assert_eq!(
            family(
                &fonts("a"),
                &cache,
                probe(&fixture, &cache, &count, "printf 'Stub Mono'")
            )
            .unwrap(),
            "Stub Mono"
        );
        assert_eq!(spawned(&count), 1);
        // The same inputs are a hit: the stub that would answer differently never runs.
        assert_eq!(
            family(
                &fonts("a"),
                &cache,
                probe(&fixture, &cache, &count, "printf 'Other'")
            )
            .unwrap(),
            "Stub Mono"
        );
        assert_eq!(spawned(&count), 1);
        // A changed input runs it once more, and an absent input differs from an empty one.
        assert_eq!(
            family(
                &fonts("b"),
                &cache,
                probe(&fixture, &cache, &count, "printf 'Changed'")
            )
            .unwrap(),
            "Changed"
        );
        assert_eq!(
            family(
                &[Some(String::new()), None],
                &cache,
                probe(&fixture, &cache, &count, "printf 'Empty'")
            )
            .unwrap(),
            "Empty"
        );
        assert_eq!(
            family(
                &[None, None],
                &cache,
                probe(&fixture, &cache, &count, "printf 'Absent'")
            )
            .unwrap(),
            "Absent"
        );
        assert_eq!(spawned(&count), 4);
        // A failed probe is an error and leaves the record alone; the next launch probes again.
        assert_eq!(
            family(
                &fonts("c"),
                &cache,
                probe(&fixture, &cache, &count, "exit 1")
            )
            .unwrap_err()
            .kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(
            family(
                &fonts("c"),
                &cache,
                probe(&fixture, &cache, &count, "printf 'Later'")
            )
            .unwrap(),
            "Later"
        );
        assert_eq!(spawned(&count), 6);
        // A damaged record is a miss, and an unwritable state directory costs only the cache.
        fixture.write(&cache, "garbage");
        assert_eq!(
            family(
                &fonts("c"),
                &cache,
                probe(&fixture, &cache, &count, "printf 'Again'")
            )
            .unwrap(),
            "Again"
        );
        let blocked = dir.join("count/flea/entry");
        assert_eq!(
            family(
                &fonts("c"),
                &blocked,
                probe(&fixture, &blocked, &count, "printf 'Uncached'")
            )
            .unwrap(),
            "Uncached"
        );
        fixture.cleanup();
    }
}
