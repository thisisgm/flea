//! Real helper output must belong to its run, including a successful quiet run.
use super::process::{Event, Job, Outcome, Runner, ThreadRunner};
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub(crate) struct Root(pub PathBuf);

impl Root {
    pub(crate) fn new() -> Self {
        let output = std::process::Command::new("mktemp")
            .args(["-d", "/tmp/flea-bfx-net.XXXXXXXX"])
            .output()
            .unwrap();
        assert!(output.status.success());
        let path = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
        std::fs::write(path.join(".flea-test-sandbox"), "bfx-net").unwrap();
        Self(path)
    }

    pub(crate) fn script(&self, name: &str, body: &str) -> String {
        use std::os::unix::fs::PermissionsExt;
        let path = self.0.join(name);
        std::fs::write(&path, body).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        path.to_str().unwrap().to_owned()
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        if self.0.starts_with("/tmp") && self.0.join(".flea-test-sandbox").is_file() {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
}

pub(crate) fn capture(job: Job) -> Outcome {
    let handle = ThreadRunner.start(job);
    let start = Instant::now();
    loop {
        if let Ok(Event::Exited(out)) = handle.events.try_recv() {
            return out;
        }
        if start.elapsed() > Duration::from_secs(3) {
            handle.kill();
            panic!("fixture helper failed to exit within three seconds");
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

pub(crate) fn answer(mut job: Job, stdout: &str, stderr: &str) -> Outcome {
    job.argv = vec![
        "sh".into(),
        "-c".into(),
        "printf %s \"$BFX_NET_OUT\"; printf %s \"$BFX_NET_ERR\" >&2".into(),
    ];
    job.env.push(("BFX_NET_OUT".into(), stdout.into()));
    job.env.push(("BFX_NET_ERR".into(), stderr.into()));
    capture(job)
}

#[test]
fn bfx_net_quiet_rerun_of_one_helper_clears_both_streams() {
    let root = Root::new();
    let script = root.script(
        "helper",
        "#!/bin/sh\nif [ ! -f \"$1/ran\" ]; then\n  touch \"$1/ran\"\n  printf previous-response\n  printf previous-diagnostic >&2\nfi\n",
    );
    for (stdout, stderr) in [("previous-response", "previous-diagnostic"), ("", "")] {
        let out = capture(Job::plain(&[&script, root.0.to_str().unwrap()]));
        assert_eq!(
            out,
            Outcome {
                code: 0,
                stdout: stdout.into(),
                stderr: stderr.into(),
            }
        );
    }
}

#[test]
fn bfx_net_quiet_mount_and_device_listings_remove_previous_rows() {
    use super::{devices::Devices, listing::Listing};
    let mount = "Mount(0): Fixture -> smb://fixture/share/\n";
    let mut listing = Listing::new();
    for expected in [mount, ""] {
        let super::listing::Effect::Start(job) = listing.poll(0).remove(0) else {
            panic!("listing must start its helper");
        };
        let out = answer(job, expected, "");
        listing.exited(&out, 0);
        assert_eq!(listing.text, expected);
        let rows = flea::ui_logic_pl2::rail::rebuild(&listing.text, &[], "");
        assert_eq!(rows.is_empty(), expected.is_empty());
    }
    let mut devices = Devices::new("fixture");
    let row = r#"{"blockdevices":[{"name":"sdb1","path":"/dev/sdb1","label":"Fixture","mountpoints":["/fixture/usb"],"rm":true,"size":4096,"type":"part","fstype":"vfat"}]}"#;
    for expected in [row, ""] {
        let super::devices::Effect::Start(_, job) = devices.poll(0).remove(0) else {
            panic!("devices must start its helper");
        };
        let out = answer(job, expected, "");
        devices.list_exited(&out);
        assert_eq!(devices.listing, expected);
        assert_eq!(devices.entries.is_empty(), expected.is_empty());
    }
}

#[test]
fn bfx_net_kill_ends_and_closes_a_run_whose_pipes_are_held_outside_its_group() {
    let root = Root::new();
    // yes ends only when the read end of its pipe closes, and the marker records that it did.
    let script = root.script(
        "helper",
        r#"#!/bin/sh
setsid sh -c 'printf "%s\n" "$$" > "$1/holder.pid"; yes; touch "$1/holder.closed"' holder "$1" &
wait
"#,
    );
    let handle = ThreadRunner.start(Job::plain(&[&script, root.0.to_str().unwrap()]));
    let pid = root.0.join("holder.pid");
    let started = Instant::now();
    while !std::fs::read_to_string(&pid).is_ok_and(|text| text.ends_with('\n')) {
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "the helper never started its pipe holder"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    let holder: i32 = std::fs::read_to_string(&pid)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    handle.kill();
    let killed = Instant::now();
    let ended = loop {
        if let Ok(Event::Exited(_)) = handle.events.try_recv() {
            break true;
        }
        if killed.elapsed() > Duration::from_secs(5) {
            break false;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let closed = root.0.join("holder.closed");
    let exited = Instant::now();
    while ended && !closed.is_file() && exited.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(5));
    }
    let closed = closed.is_file();
    if !closed {
        assert!(holder > 1 && root.0.join(".flea-test-sandbox").is_file());
        let _ = std::process::Command::new("/bin/kill")
            .args(["-KILL", "--", &format!("-{holder}")])
            .status();
    }
    assert!(
        ended,
        "a killed run must end while a process outside its group holds its pipes"
    );
    assert!(
        closed,
        "an ended run must close its end of a pipe held outside its group"
    );
}
