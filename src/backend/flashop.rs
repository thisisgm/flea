// The flash dispatcher: one flash at a time beside transfers, the worker behind flashdone.
use crate::backend::flash::{flashdone_line, flashimage_line, flashstarted_line, flashtargets_line, Drive};
use crate::backend::flashdesc::{check_descriptor, gone};
use crate::backend::flashmeter::Meter;
use crate::backend::flashslot::{FlashMsg, Slot};
use crate::backend::flashverify::wipe_ends;
use crate::backend::flashwrite::{Fail, Job};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// One answer line, written and flushed, so no path leaves it waiting in a buffer.
pub(crate) fn answer(out: &mut dyn std::io::Write, line: &str) {
    writeln!(out, "{line}").ok();
    out.flush().ok();
}

// Classifies one image and answers its flashimage line; a mismatch refuses the row with no kind.
pub fn handle_flashimage(out: &mut dyn std::io::Write, path: &str) {
    let probe = crate::backend::flashimage::probe(&flea::bytename::view(path));
    answer(out, &flashimage_line(path, &probe.kind, probe.bytes, probe.unpacked, probe.known, &probe.member));
}

// Lists every eligible drive with the refusals the image causes, then drops the daemon's system disks.
pub fn handle_flashtargets(out: &mut dyn std::io::Write, image: &str) {
    let (exe, ui) = crate::backend::flasheligible::roots_for_production();
    let loop_root = crate::backend::flasheligible::seam_root();
    let roots = crate::backend::flasheligible::Roots { sys: Path::new("/sys"), dev: Path::new("/dev"), exe: &exe, ui: &ui, loop_root: loop_root.as_deref() };
    let mountinfo = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    let swaps = std::fs::read_to_string("/proc/swaps").unwrap_or_default();
    let (image_path, image_bytes) = flash_measure(image);
    let drives = crate::backend::flasheligible::list_drives(&roots, &mountinfo, &swaps, image_path.as_deref(), image_bytes);
    let kept = crate::backend::flashhelper::surviving(&drives.iter().map(|d| d.dev.clone()).collect::<Vec<_>>());
    let drives: Vec<Drive> = drives.into_iter().filter(|d| kept.contains(&d.dev)).collect();
    answer(out, &flashtargets_line(&drives));
}

// The unpacked size when the probe knows it, else the file bytes as the lower bound for small.
fn flash_measure(image: &str) -> (Option<std::path::PathBuf>, Option<u64>) {
    if image.is_empty() {
        return (None, None);
    }
    let path = flea::bytename::exact(image);
    let probe = crate::backend::flashimage::probe(&path);
    if probe.kind.is_empty() {
        return (Some(path), None);
    }
    let want = if probe.known { probe.unpacked } else { probe.bytes };
    (Some(path), Some(want))
}

pub struct FlashReq {
    pub id: usize,
    pub image: String,
    pub dev: String,
    pub seq: u64,
    pub serial: String,
    pub bytes: u64,
}

// The five flash requests, dispatched on the flash slot alone; cancel only sets its flag and the flash answers its own flashdone.
pub fn request(out: &mut dyn std::io::Write, flash: &mut super::flashslot::Ctx, req: super::flash::Req) {
    match req {
        super::flash::Req::Image { path } => handle_flashimage(out, &path),
        super::flash::Req::Targets { image } => handle_flashtargets(out, &image),
        super::flash::Req::Flash { id, image, dev, seq, serial, bytes } => start_flash(out, &flash.slot, &flash.tx,
            FlashReq { id, image, dev, seq: seq as u64, serial, bytes }),
        super::flash::Req::Cancel { id } => flash.slot.cancel(id),
        super::flash::Req::Eject { dev, seq } => crate::backend::flasheject::handle_flasheject(out, &dev, seq as u64),
    }
}

// Starts the flash worker, or refuses when one already runs; every ending is that worker's flashdone.
pub fn start_flash(out: &mut dyn std::io::Write, slot: &Slot, tx: &std::sync::mpsc::Sender<FlashMsg>, req: FlashReq) {
    let Some(flag) = slot.claim(req.id) else {
        let line = crate::backend::proto::error_line(&crate::error::FleaError {
            where_: "flash".to_string(),
            path: req.dev.clone(),
            msg: "a flash is already running".to_string(),
        });
        answer(out, &line);
        return;
    };
    let tx = tx.clone();
    std::thread::spawn(move || run_flash(req, flag, &tx));
}

fn send(tx: &std::sync::mpsc::Sender<FlashMsg>, id: usize, line: String, terminal: bool) {
    let _ = tx.send(FlashMsg { id, line, terminal });
}

// How one flash ended: the one flashdone line's fields.
pub(crate) struct Ending {
    pub ok: bool,
    pub phase: &'static str,
    pub at: u64,
    pub err: &'static str,
    pub blank: bool,
}

impl Ending {
    fn failed(phase: &'static str, err: &'static str, blank: bool) -> Ending {
        Ending { ok: false, phase, at: 0, err, blank }
    }
}

// Where a flash reads the box: sysfs and /dev, the mount and swap tables, and the helper it asks for the descriptor.
pub(crate) struct Host<'a> {
    pub sys: &'a Path,
    pub dev: &'a Path,
    pub exe: &'a Path,
    pub ui: &'a Path,
    pub mountinfo: &'a Path,
    pub swaps: &'a Path,
    pub helper: &'a str,
}

fn run_flash(req: FlashReq, cancel: Arc<AtomicBool>, tx: &std::sync::mpsc::Sender<FlashMsg>) {
    let (exe, ui) = crate::backend::flasheligible::roots_for_production();
    let helper = crate::backend::flashhelper::helper_path();
    let host = Host { sys: Path::new("/sys"), dev: Path::new("/dev"), exe: &exe, ui: &ui,
        mountinfo: Path::new("/proc/self/mountinfo"), swaps: Path::new("/proc/swaps"), helper: &helper };
    run_flash_on(req, cancel, tx, &host);
}

pub(crate) fn run_flash_on(req: FlashReq, cancel: Arc<AtomicBool>, tx: &std::sync::mpsc::Sender<FlashMsg>, host: &Host) {
    let id = req.id;
    let done = |end: &Ending| send(tx, id, flashdone_line(id, end.ok, end.phase, end.at, end.err, end.blank), true);
    // The probe first: a row the pane never sends is refused before anything is touched.
    let probe = crate::backend::flashimage::probe(&flea::bytename::view(&req.image));
    if probe.kind.is_empty() {
        done(&Ending::failed("probe", "io", false));
        return;
    }
    let total = if probe.known { probe.unpacked } else { probe.bytes };
    send(tx, id, flashstarted_line(id, total, probe.known), false);
    let mut meter = Meter::new(id, tx, probe.known, total, probe.bytes);
    // Layers 1 to 4 against fresh sysfs, then layer 2 asked again, before the helper opens anything.
    meter.beat("prepare", 0, 0, true);
    let loop_root = crate::backend::flasheligible::seam_root();
    let roots = crate::backend::flasheligible::Roots {
        sys: host.sys,
        dev: host.dev,
        exe: host.exe,
        ui: host.ui,
        loop_root: loop_root.as_deref(),
    };
    let mountinfo = std::fs::read_to_string(host.mountinfo).unwrap_or_default();
    let swaps = std::fs::read_to_string(host.swaps).unwrap_or_default();
    let chosen = crate::backend::flasheligible::Chosen {
        dev: &req.dev,
        seq: req.seq,
        serial: &req.serial,
        bytes: req.bytes,
    };
    if crate::backend::flasheligible::identity_changed(
        &roots,
        &mountinfo,
        &swaps,
        &chosen,
        Some(&*flea::bytename::view(&req.image)),
        Some(total),
    ) {
        done(&Ending::failed("prepare", "changed", false));
        return;
    }
    // A cancel that landed during prepare ends here, before any prompt rises for a dead flash.
    if cancel.load(Ordering::Relaxed) {
        done(&Ending::failed("prepare", "cancelled", false));
        return;
    }
    if !crate::backend::flashhelper::confirm(host.helper, &req.dev) {
        done(&Ending::failed("prepare", "helper", false));
        return;
    }
    // The one prompt, then the exclusive descriptor, or the reason it never came.
    let dest = match crate::backend::flashhelper::open_target(host.helper, &req, &mut || meter.beat("password", 0, 0, true)) {
        Ok(file) => file,
        Err(err) => {
            done(&Ending::failed("password", err, false));
            return;
        }
    };
    // Layer 6 on the descriptor itself, so a swapped or mounted disk ends changed before a byte moves.
    if let Err(err) = check_descriptor(&dest, &req) {
        done(&Ending::failed("prepare", err, false));
        return;
    }
    if cancel.load(Ordering::Relaxed) {
        done(&Ending::failed("password", "cancelled", false));
        return;
    }
    let image = flea::bytename::view(&req.image);
    let mut job = Job::new(
        &image,
        &probe.kind,
        &probe.member,
        &dest,
        req.bytes,
        &cancel,
    );
    job.probe_total = total;
    job.probe_known = probe.known;
    let end = flash_body(&job, &req.dev, &mut meter);
    // The descriptor closes before the ending is sent, so udev re-reads the table while the card reads it.
    drop(dest);
    done(&end);
}

// One sentence for the log naming the phase and the disk, with the offset inside the detail.
fn io_line(phase: &str, dev: &str, detail: &str) -> String {
    format!("flea: the flash {phase} on /dev/{dev} failed ({detail})")
}

// The last io line this thread logged, so a test reads which sentence an ending chose.
#[cfg(test)]
thread_local! {
    static LAST_IO_LINE: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

// A failed read or write: gone when the kernel dropped the disk, else a wiped drive that says io.
fn io_ending(phase: &'static str, dev: &str, detail: &str, job: &Job) -> Ending {
    eprintln!("{}", io_line(phase, dev, detail));
    #[cfg(test)]
    LAST_IO_LINE.with(|line| *line.borrow_mut() = io_line(phase, dev, detail));
    if gone(Path::new("/sys"), dev) {
        return Ending::failed(phase, "gone", false);
    }
    Ending::failed(phase, "io", wipe_ends(job.dest, job.drive_bytes))
}

// What each way a write ends leaves behind on the drive; the unpacker's own words stay in the log.
fn write_ending(fail: Fail, dev: &str, job: &Job) -> Ending {
    let wiped = || wipe_ends(job.dest, job.drive_bytes);
    match fail {
        Fail::Cancelled => Ending::failed("write", "cancelled", wiped()),
        Fail::NoSpace => Ending::failed("write", "nospace", wiped()),
        Fail::Decompress(e) => {
            eprintln!("flea: the unpacker failed on {} ({e})", job.image.display());
            Ending::failed("write", "decompress", wiped())
        }
        Fail::Sandbox => Ending::failed("write", "sandbox", false),
        Fail::Io(e) => io_ending("write", dev, &e, job),
        Fail::Mismatch(at) => io_ending("write", dev, &format!("a mismatch at {at} while writing"), job),
    }
}

// The log sentence for a stream whose length differs from its known probe total; None when the two agree.
fn size_refusal(written: u64, total: u64) -> Option<&'static str> {
    if written < total {
        Some("the image ended short")
    } else if written > total {
        Some("the image held more data than its header listed")
    } else {
        None
    }
}

// Everything after the descriptor is proven: write, flush, verify, and what each ending leaves behind.
pub(crate) fn flash_body(job: &Job, dev: &str, meter: &mut Meter) -> Ending {
    let (table, written) = match crate::backend::flashwrite::write_stream(job, &mut |w, i| meter.beat("write", w, i, false)) {
        Ok(done) => done,
        Err(fail) => return write_ending(fail, dev, job),
    };
    // A stream that disagrees with its known probe total never ends ok, short or long, however clean the write.
    if let Some(detail) = job.probe_known.then(|| size_refusal(written, job.probe_total)).flatten() {
        return io_ending("write", dev, detail, job);
    }
    // The written total is the size now, so flush and verify measure against it, packed or not.
    meter.settle(written);
    meter.beat("flush", written, written, true);
    if let Err(e) = crate::backend::flashio::flush(job.dest) {
        return io_ending("flush", dev, &e.to_string(), job);
    }
    // A cancel that lands in the flush wipes: the image is not yet proven, and only a verify may keep it.
    if job.cancel.load(Ordering::Relaxed) {
        return Ending::failed("flush", "cancelled", wipe_ends(job.dest, job.drive_bytes));
    }
    match crate::backend::flashverify::verify(job.dest, &table, written, job.cancel, &mut |w| meter.beat("verify", w, w, false)) {
        Ok(()) => Ending { ok: true, phase: "verify", at: 0, err: "", blank: false },
        // FlasherRun callout 6: a cancelled verify leaves the image flashed and offers Eject alone.
        Err(Fail::Cancelled) => Ending::failed("verify", "cancelled", false),
        Err(Fail::Mismatch(at)) => Ending { ok: false, phase: "verify", at, err: "verify", blank: wipe_ends(job.dest, job.drive_bytes) },
        Err(Fail::Io(e)) => io_ending("verify", dev, &e, job),
        Err(_) => io_ending("verify", dev, "the verify ended unexpectedly", job),
    }
}

#[cfg(test)]
#[path = "flashop_tests.rs"]
mod tests;
