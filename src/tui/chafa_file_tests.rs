use super::test_support::{sandboxed, Fixture};
use super::*;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::time::Duration;

#[test]
fn held_writable_file_cannot_be_written_through_stdin_or_input_mount() {
    if !sandboxed() {
        return;
    }
    let fixture = Fixture::new();
    fixture.stub(
        r#"
bad=0
host_dir=${0%/bin/chafa}
[ ! -e "$host_dir/writable" ] || exit 6
exec 5</input
refuse() {
    path=$1
    if /bin/sh -c 'printf changed > "$1"' sh "$path"; then bad=1; fi
    if chmod 0666 "$path"; then bad=1; fi
}
for path in /input /proc/self/fd/0 /proc/self/fd/5; do refuse "$path"; done
for path in /proc/self/fd/*; do
    if [ "$path" -ef /input ]; then refuse "$path"; fi
done
printf '▀'
exit "$bad"
"#,
    );
    let path = fixture.file("writable", b"held");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    let input = File::options().read(true).write(true).open(&path).unwrap();
    let job = Job::start_on(
        input,
        1,
        1,
        ColorMode::Truecolor,
        fixture.path("bin").as_os_str(),
    )
    .unwrap()
    .unwrap();
    let result = job.result.recv_timeout(Duration::from_secs(5)).unwrap();
    let after = (
        std::fs::read(&path).unwrap(),
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
    );
    assert_eq!(
        after,
        (b"held".to_vec(), 0o644),
        "host contents and permissions are unchanged"
    );
    assert_eq!(result.unwrap().cells[0].symbol, '▀');
}

#[test]
fn nonregular_held_descriptor_is_refused_before_spawn() {
    let fixture = Fixture::new();
    fixture.stub("exit 99");
    let input = File::open(fixture.path("bin")).unwrap();
    assert!(Job::start_on(
        input,
        1,
        1,
        ColorMode::Truecolor,
        fixture.path("bin").as_os_str(),
    )
    .is_err());
}

#[test]
fn symlink_swapped_after_open_uses_held_descriptor_and_unlinked_input_fails_closed() {
    if !sandboxed() {
        return;
    }
    let fixture = Fixture::new();
    fixture.stub("IFS= read -r value < /input; [ \"$value\" = held ]; printf '▀'");
    let original = fixture.file("original", b"held\n");
    let replacement = fixture.file("replacement", b"wrong\n");
    let link = fixture.path("selected");
    symlink(&original, &link).unwrap();
    let input = crate::backend::regfile::open_if_regular(&link, 0).unwrap();
    let swapped = fixture.path("swapped");
    symlink(&replacement, &swapped).unwrap();
    std::fs::rename(fixture.path("swapped"), fixture.path("selected")).unwrap();
    let mut job = Job::start_on(
        input.try_clone().unwrap(),
        1,
        1,
        ColorMode::Truecolor,
        fixture.path("bin").as_os_str(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        job.result
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap()
            .cells[0]
            .symbol,
        '▀'
    );
    job.cancel();
    assert_eq!(std::fs::read(link).unwrap(), b"wrong\n");
    std::fs::remove_file(fixture.path("original")).unwrap();
    let mut deleted = Job::start_on(
        input,
        1,
        1,
        ColorMode::Truecolor,
        fixture.path("bin").as_os_str(),
    )
    .unwrap()
    .unwrap();
    assert!(deleted
        .result
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .is_err());
    deleted.cancel();
    assert_eq!(std::fs::read(replacement).unwrap(), b"wrong\n");
}
