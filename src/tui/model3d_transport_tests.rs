use super::Graphics;
use super::tests::*;
use crate::tui::{graphics::Protocol, model::Row, preview, stage, wire};

// Each process test gets a private PATH and no controlling terminal, without changing the test harness environment.
fn chafa_child(name: &str, body: &str) -> bool {
    if std::env::var_os("FLEA_TUI04_FIX1_CHAFA").is_some() { return false; }
    let root = Fixture::new();
    for name in ["bin", "home", "runtime", "cache", "config", "state"] { std::fs::create_dir(root.path(name)).unwrap(); }
    use std::os::unix::fs::PermissionsExt;
    use std::io::Write;
    std::fs::set_permissions(root.path("runtime"), std::fs::Permissions::from_mode(0o700)).unwrap();
    let script = root.path("bin/chafa");
    let mut writer = std::process::Command::new("/bin/sh").args(["-c", "cat > \"$1\" && chmod 755 \"$1\"", "sh"])
        .arg(&script).stdin(std::process::Stdio::piped()).spawn().unwrap();
    writer.stdin.take().unwrap().write_all(format!("#!/bin/sh\nset -eu\n{body}\n").as_bytes()).unwrap();
    assert!(writer.wait().unwrap().success());
    let output = std::process::Command::new("/usr/bin/setsid").arg("-w").arg(std::env::current_exe().unwrap())
        .args(["--exact", &format!("tui::model3d::transport_tests::{name}"), "--nocapture"])
        .env_clear().env("PATH", format!("{}:/usr/bin:/bin", root.path("bin").display()))
        .env("HOME", root.path("home")).env("LANG", "C.UTF-8").env("FLEA_TUI04_FIX1_CHAFA", script)
        .env("XDG_RUNTIME_DIR", root.path("runtime")).env("XDG_CACHE_HOME", root.path("cache"))
        .env("XDG_CONFIG_HOME", root.path("config")).env("XDG_STATE_HOME", root.path("state"))
        .stdin(std::process::Stdio::null()).output().unwrap();
    assert!(output.status.success(), "{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
    true
}

#[test]
fn tui_model3d_fix1_retiring_hold_preserves_next_image() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    if chafa_child("tui_model3d_fix1_retiring_hold_preserves_next_image", "printf '\\033[38;2;1;2;3m\\342\\226\\200\\033[0m\\n'") { return; }
    assert!(crate::tui::chafa::test_support::sandboxed());
    let root = Fixture::new();
    let image = root.path("next.png");
    std::fs::write(&image, b"P6\n2 2\n255\nabcdefghijkl").unwrap();
    for protocol in [Protocol::Kitty, Protocol::Sixel, Protocol::None] {
        let mut m = model(&root);
        let (mut wire, reader) = wire::echo_wire();
        let (mut graphics, mut shown) = (Graphics::new(), stage::Shown::default());
        graphics.protocol = protocol;
        m.capabilities.graphics = protocol;
        prime(&mut m, &root, &mut graphics, &mut shown, &mut wire);
        let (release, paused) = std::sync::mpsc::channel::<()>();
        m.model3d.holding = Some(super::Task::start(move |_| { let _ = paused.recv(); Err("retired hold".into()) }));
        let row = m.rows.get_mut(&0).unwrap();
        row.name = "next.png".into();
        row.kind = "PNG image".into();
        row.icon = "image-png".into();
        row.thumbnail = true;
        crate::tui::preview::load(&mut m, true);
        m.media_probe = Some((2, 2, 0));
        step(&mut m, &mut graphics, &mut shown, &mut wire, false);
        if protocol == Protocol::None {
            settle(|| { m.preview_state.still.poll(); !m.preview_state.still.running() });
            let key = m.preview_state.still.key().unwrap().clone();
            assert!(m.preview_state.still.grid(&key).is_some());
            step(&mut m, &mut graphics, &mut shown, &mut wire, false);
            assert!(m.preview_state.still.grid(&key).is_some(), "retiring model cleared the next chafa picture");
        } else {
            settle(|| { graphics.accept(); !graphics.bytes.is_empty() || !graphics.error.is_empty() });
            assert!(graphics.error.is_empty(), "{}", graphics.error);
            assert!(graphics.shows(&image));
            step(&mut m, &mut graphics, &mut shown, &mut wire, false);
            assert!(graphics.shows(&image), "retiring model cleared the next native picture");
        }
        drop(release);
        settle(|| { step(&mut m, &mut graphics, &mut shown, &mut wire, false); !m.model3d.active() });
        drop(graphics);
        wire::finish(wire, reader);
    }
}

#[test]
fn tui_model3d_fix2_manual_model_row_drops_previous_picture() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    if chafa_child("tui_model3d_fix2_manual_model_row_drops_previous_picture", "printf '\\033[38;2;1;2;3m\\342\\226\\200\\033[0m\\n'") { return; }
    assert!(crate::tui::chafa::test_support::sandboxed());
    let root = Fixture::new();
    let image = root.path("next.png");
    std::fs::write(&image, b"P6\n2 2\n255\nabcdefghijkl").unwrap();
    for protocol in [Protocol::Kitty, Protocol::Sixel, Protocol::None] {
        let mut m = model(&root);
        m.preview_auto = false;
        m.total = 2;
        m.rows.insert(1, Row { name: "next.png".into(), directory: false, size: 23, mode: 0o100644, link: String::new(), kind: "PNG image".into(), thumbnail: true, modified: 0, birth: None, icon: "image-png".into() });
        m.cursor = 1;
        preview::load(&mut m, true);
        m.media_probe = Some((2, 2, 0));
        let (mut wire, reader) = wire::echo_wire();
        let (mut graphics, mut shown) = (Graphics::new(), stage::Shown::default());
        graphics.protocol = protocol;
        m.capabilities.graphics = protocol;
        if protocol == Protocol::None {
            settle(|| { step(&mut m, &mut graphics, &mut shown, &mut wire, false); m.preview_state.still.key().is_some() && !m.preview_state.still.running() });
            let key = m.preview_state.still.key().unwrap().clone();
            assert!(m.preview_state.still.grid(&key).is_some(), "the previous chafa picture did not show");
        } else {
            settle(|| { step(&mut m, &mut graphics, &mut shown, &mut wire, false); graphics.accept(); graphics.shows(&image) || !graphics.error.is_empty() });
            assert!(graphics.error.is_empty(), "{}", graphics.error);
            assert!(graphics.shows(&image), "the previous native picture did not show");
        }
        m.cursor = 0;
        preview::load(&mut m, false);
        assert!(!m.preview_loaded, "a manual preview loaded the model row");
        step(&mut m, &mut graphics, &mut shown, &mut wire, false);
        if protocol == Protocol::None {
            assert!(m.preview_state.still.key().is_none() && !m.preview_state.still.running(), "model row kept the previous chafa picture");
        } else {
            assert!(!graphics.shows(&image), "model row kept the previous native picture");
        }
        m.model3d.stop(&mut wire).unwrap();
        drop(graphics);
        wire::finish(wire, reader);
    }
}

#[test]
fn tui_model3d_fix1_chafa_to_native_cancels_and_reaps() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    if chafa_child("tui_model3d_fix1_chafa_to_native_cancels_and_reaps", "trap '' TERM\nwhile :; do /usr/bin/sleep 60; done") { return; }
    assert!(crate::tui::chafa::test_support::sandboxed());
    let root = Fixture::new();
    let mut m = model(&root);
    let (mut wire, reader) = wire::echo_wire();
    let (mut graphics, mut shown) = (Graphics::new(), stage::Shown::default());
    let marker = std::env::var("FLEA_TUI04_FIX1_CHAFA").unwrap();
    let children = || std::fs::read_dir("/proc").unwrap().flatten().filter(|entry| {
        std::fs::read(entry.path().join("cmdline")).is_ok_and(|line| line.split(|b| *b == 0).any(|arg| arg == marker.as_bytes()))
    }).count();
    prime(&mut m, &root, &mut graphics, &mut shown, &mut wire);
    for protocol in [Protocol::Kitty, Protocol::Sixel] {
        graphics.clear();
        graphics.protocol = Protocol::None;
        m.capabilities.graphics = Protocol::None;
        settle(|| { step(&mut m, &mut graphics, &mut shown, &mut wire, false); children() > 0 });
        assert!(m.preview_state.still.running());
        let renders = m.model3d.renders;
        graphics.clear();
        graphics.protocol = protocol;
        m.capabilities.graphics = protocol;
        step(&mut m, &mut graphics, &mut shown, &mut wire, false);
        assert!(m.preview_state.still.key().is_none() && !m.preview_state.still.running(), "native transport left chafa active");
        settle(|| { step(&mut m, &mut graphics, &mut shown, &mut wire, false); children() == 0 && !graphics.bytes.is_empty() });
        assert_eq!(m.model3d.renders, renders, "a protocol change rerendered the same model");
    }
    m.model3d.stop(&mut wire).unwrap();
    drop(graphics);
    wire::finish(wire, reader);
}

#[test]
fn tui_model3d_all_transports_cancel_and_reap() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    assert!(crate::tui::chafa::test_support::sandboxed(), "model transport acceptance requires the real preview sandbox");
    let root = Fixture::new();
    for protocol in [Protocol::Kitty, Protocol::Sixel, Protocol::None] {
        let mut m = model(&root);
        let (mut wire, reader) = wire::echo_wire();
        let (mut graphics, mut shown) = (Graphics::new(), stage::Shown::default());
        graphics.protocol = protocol;
        m.capabilities.graphics = protocol;
        prime(&mut m, &root, &mut graphics, &mut shown, &mut wire);
        if protocol != Protocol::None || crate::tui::chafa::installed() {
            settle(|| { step(&mut m, &mut graphics, &mut shown, &mut wire, false); m.model3d.ready.is_some() });
            step(&mut m, &mut graphics, &mut shown, &mut wire, false);
            if protocol != Protocol::None {
                settle(|| { step(&mut m, &mut graphics, &mut shown, &mut wire, false); !graphics.bytes.is_empty() || m.preview_state.unshown.is_some() });
                assert!(m.preview_state.unshown.is_none(), "native encoding failed");
                assert!(!graphics.bytes.is_empty());
                if protocol == Protocol::Kitty { assert!(graphics.transmit.starts_with(b"\x1b_Ga=t,f=100,")); }
                else { assert!(graphics.bytes.windows(2).any(|w| w == b"\x1bP")); }
            }
        } else {
            for _ in 0..10 { step(&mut m, &mut graphics, &mut shown, &mut wire, false); }
            assert_eq!(m.model3d.renders, 0);
            assert_eq!(m.preview_metadata.len(), 2);
            assert!(m.error.is_empty() && m.preview_state.unshown.is_none());
        }
        step(&mut m, &mut graphics, &mut shown, &mut wire, true);
        assert!(graphics.bytes.is_empty() && graphics.transmit.is_empty());
        assert!(m.preview_state.still.key().is_none() && m.model3d.source.is_none());
        m.model3d.stop(&mut wire).unwrap();
        drop(graphics);
        drop(m);
        wire::finish(wire, reader);
    }
}

#[test]
fn tui_model3d_clean_environment_and_chafa_absent() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let root = Fixture::new();
    for name in ["bin", "home", "runtime", "cache", "config", "state"] { std::fs::create_dir(root.path(name)).unwrap(); }
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(root.path("runtime"), std::fs::Permissions::from_mode(0o700)).unwrap();
    for name in ["sh", "mktemp", "prlimit", "bwrap"] { std::os::unix::fs::symlink(format!("/usr/bin/{name}"), root.path(&format!("bin/{name}"))).unwrap(); }
    let output = std::process::Command::new("/usr/bin/setsid").arg("-w").arg(std::env::current_exe().unwrap())
        .args(["--exact", "tui::model3d::transport_tests::tui_model3d_all_transports_cancel_and_reap", "--nocapture"])
        .env_clear().env("PATH", root.path("bin")).env("HOME", root.path("home")).env("LANG", "C.UTF-8")
        .env("XDG_RUNTIME_DIR", root.path("runtime")).env("XDG_CACHE_HOME", root.path("cache"))
        .env("XDG_CONFIG_HOME", root.path("config")).env("XDG_STATE_HOME", root.path("state"))
        .stdin(std::process::Stdio::null()).output().unwrap();
    assert!(output.status.success(), "{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
}

#[test]
fn tui_model3d_real_tui_board_specimens_and_stepped_transport() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let root = Fixture::new();
    let executable = std::env::current_exe().unwrap();
    let binary = executable.parent().unwrap().parent().unwrap().join("flea");
    assert!(binary.is_file(), "cargo test must build the real TUI candidate");
    let output = std::process::Command::new("python3").args(["-B", "-c", REAL_TUI])
        .arg(binary).arg(root.path(""))
        .env("PYTHONPATH", std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests"))
        .stdin(std::process::Stdio::null()).output().unwrap();
    assert!(output.status.success(), "{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("PASS real --tui"));
}

const REAL_TUI: &str = r#"
import errno, fcntl, os, pty, re, select, signal, struct, subprocess, sys, termios, threading, time
from pathlib import Path
from tui_battery.bundle import models
binary, root = Path(sys.argv[1]), Path(sys.argv[2])
assert root.is_absolute() and (root / '.flea-test-sandbox').is_file()
for name in ('home', 'config', 'cache', 'state', 'data', 'runtime', 'Prints'):
    (root / name).mkdir()
os.chmod(root / 'runtime', 0o700)
(root / 'Prints/bracket.stl').write_bytes(models.stl(models.bracket(), 'bracket.stl'))
(root / 'Prints/shelf-kit.3mf').write_bytes(models.shelf_kit_3mf(1200000))
env = dict(PATH='/usr/bin:/bin', HOME=str(root / 'home'), LANG='C.UTF-8', TERM='xterm-kitty', COLORTERM='truecolor')
for variable, name in [('XDG_CONFIG_HOME','config'), ('XDG_CACHE_HOME','cache'), ('XDG_STATE_HOME','state'), ('XDG_DATA_HOME','data'), ('XDG_RUNTIME_DIR','runtime')]:
    env[variable] = str(root / name)
master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 31, 130, 1040, 496))
def session():
    os.setsid()
    fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
child = subprocess.Popen([str(binary), '--tui', '--select', str(root / 'Prints/bracket.stl')], stdin=slave, stdout=slave, stderr=slave, env=env, preexec_fn=session)
os.close(slave)
captured = bytearray()
query_tail = bytearray()
def pump(seconds):
    end = time.monotonic() + seconds
    while time.monotonic() < end and child.poll() is None:
        if not select.select([master], [], [], max(0, min(.02, end - time.monotonic())))[0]:
            continue
        try:
            chunk = os.read(master, 65536)
        except OSError as e:
            if e.errno == errno.EIO: break
            raise
        captured.extend(chunk)
        query_tail.extend(chunk)
        for match in list(re.finditer(rb'\x1b_G([^\x1b]*a=q[^\x1b]*);[^\x1b]*\x1b\\', query_tail)):
            identity = re.search(rb'(?:^|,)i=(\d+)', match[1])
            if identity: os.write(master, b'\x1b_Gi=' + identity[1] + b';OK\x1b\\')
        if b'\x1b[16t' in query_tail: os.write(master, b'\x1b[6;16;8t')
        if b'\x1b[14t' in query_tail: os.write(master, b'\x1b[4;496;1040t')
        if b'\x1b[c' in query_tail: os.write(master, b'\x1b[?62c')
        if b'\x1b[?u' in query_tail: os.write(master, b'\x1b[?0u')
        query_tail[:] = query_tail[-4:]
# Sample input: b'\x1b_Ga=t,f=100,i=42;AAAA\x1b\\\x1b_Ga=p,i=42,q=2\x1b\\'; each complete transmit/place pair counts once.
def images(): return len(re.findall(rb'\x1b_Ga=t,f=100,.*?\x1b_Ga=p,i=42,q=2\x1b\\', captured, re.S))
def until(check, why):
    end = time.monotonic() + 30
    while not check():
        assert child.poll() is None, (why, child.returncode, bytes(captured[-3000:]))
        assert time.monotonic() < end, (why, bytes(captured[-3000:]))
        pump(.03)
def press(data): os.write(master, data)
try:
    until(lambda: images() >= 1, 'STL column picture')
    assert b'4,212' in captured and '60.0'.encode() in captured, bytes(captured[-3000:])
    count = images()
    pump(.3)
    assert images() == count, 'column redrew at rest'
    press(b'j')
    until(lambda: b'shelf-kit.3mf' in captured and images() > count, '3MF column picture')
    count = images()
    press(b' ')
    until(lambda: b'Plate 1' in captured and b'Plate 2' in captured and images() > count, '3MF Quick Look plates')
    for _ in range(2):
        count = images()
        press(b'\x1b[C')
        until(lambda: images() > count, '15 degree orbit picture')
        assert images() == count + 1
    count = images()
    pump(.3)
    assert images() == count, 'Quick Look redrew at rest'
    for key in (b']', b'[', b'+', b'-', b'w', b'\x1b[C', b'0'):
        count = images()
        press(key)
        until(lambda: images() > count, 'plate/zoom/wire/reframe picture')
    count = images()
    fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack('HHHH', 27, 110, 880, 432))
    os.kill(child.pid, signal.SIGWINCH)
    until(lambda: images() > count, 'resized picture')
    count = images()
    # Force a scheduling pause: Escape and q must still be separate decoded keys.
    child.send_signal(signal.SIGSTOP)
    threading.Timer(.3, child.send_signal, args=(signal.SIGCONT,)).start()
    press(b'\x1b')
    until(lambda: images() > count, 'Escape restored the column picture')
    press(b'q')
    end = time.monotonic() + 10
    while child.poll() is None and time.monotonic() < end: pump(.03)
    child.wait(timeout=0)
    assert child.returncode == 0, child.returncode
    print('PASS real --tui: STL facts, 3MF plates, two orbit pictures, idle, controls, resize, clean quit')
finally:
    if child.poll() is None:
        child.kill()
        child.wait()
    os.close(master)
"#;

// D10: chafa 1.18 refuses PPM as an unknown format, so the half-block model still must reach it as a format it decodes.
#[test]
fn tui_model3d_half_blocks_reach_chafa_in_a_format_it_decodes() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    if chafa_child("tui_model3d_half_blocks_reach_chafa_in_a_format_it_decodes",
        "for a; do last=$a; done\n[ \"$(/usr/bin/head -c 4 \"$last\")\" = qoif ] || { echo 'Unknown file format' >&2; exit 2; }\nprintf '\\033[38;2;1;2;3m\\342\\226\\200\\033[0m\\n'") { return; }
    assert!(crate::tui::chafa::test_support::sandboxed());
    let root = Fixture::new();
    let mut m = model(&root);
    let (mut wire, reader) = wire::echo_wire();
    let (mut graphics, mut shown) = (Graphics::new(), stage::Shown::default());
    graphics.protocol = Protocol::None;
    m.capabilities.graphics = Protocol::None;
    prime(&mut m, &root, &mut graphics, &mut shown, &mut wire);
    settle(|| {
        step(&mut m, &mut graphics, &mut shown, &mut wire, false);
        m.model3d.failed || m.preview_state.still.key().cloned().is_some_and(|key| m.preview_state.still.grid(&key).is_some())
    });
    assert!(!m.model3d.failed, "chafa refused the model's still, so the pane said it cannot preview the model");
    m.model3d.stop(&mut wire).unwrap();
    drop(graphics);
    wire::finish(wire, reader);
}
