use super::Fixture;

// Each process test gets a private PATH and no controlling terminal, without changing the test harness environment.
pub(super) fn chafa_child(name: &str, body: &str) -> bool {
    if std::env::var_os("FLEA_TUI04_FIX1_CHAFA").is_some() {
        return false;
    }
    let root = Fixture::new();
    for name in ["bin", "home", "runtime", "cache", "config", "state"] {
        std::fs::create_dir(root.path(name)).unwrap();
    }
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(root.path("runtime"), std::fs::Permissions::from_mode(0o700)).unwrap();
    let script = root.path("bin/chafa");
    let mut writer = std::process::Command::new("/bin/sh")
        .args(["-c", "cat > \"$1\" && chmod 755 \"$1\"", "sh"])
        .arg(&script)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    writer
        .stdin
        .take()
        .unwrap()
        .write_all(format!("#!/bin/sh\nset -eu\n{body}\n").as_bytes())
        .unwrap();
    assert!(writer.wait().unwrap().success());
    let output = std::process::Command::new("/usr/bin/setsid")
        .arg("-w")
        .arg(std::env::current_exe().unwrap())
        .args([
            "--exact",
            &format!("tui::model3d::transport_tests::{name}"),
            "--nocapture",
        ])
        .env_clear()
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", root.path("bin").display()),
        )
        .env("HOME", root.path("home"))
        .env("LANG", "C.UTF-8")
        .env("FLEA_TUI04_FIX1_CHAFA", script)
        .env("XDG_RUNTIME_DIR", root.path("runtime"))
        .env("XDG_CACHE_HOME", root.path("cache"))
        .env("XDG_CONFIG_HOME", root.path("config"))
        .env("XDG_STATE_HOME", root.path("state"))
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
    true
}

pub(super) const REAL_TUI: &str = r#"
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
