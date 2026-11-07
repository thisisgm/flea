// One conversion: the PDF run, and the jailed converter run every office job shares.
use super::*;

impl Service {
    pub(super) fn run(&self, job: &Job) -> Result<PathBuf, Fail> {
        let local = |e: std::io::Error| Fail::Local(crate::error::io_message(&e));
        let dest = self.store.pdf_path(&job.key);
        // Checked before the converter runs, so a document this cache cannot replace costs no child; publish checks again.
        crate::cachewrite::require(&dest).map_err(local)?;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.store.dir)
            .map_err(local)?;
        let mut work = Work::new(&self.store.dir, "office").map_err(|e| Fail::Local(e.msg))?;
        let target = job.head.map_or_else(|| "pdf".to_string(), head_target);
        let argv = (self.build)(&job.program, &target, &job.input, &work.dir);
        self.jailed(job, argv, &mut work, Instant::now() + self.timeout)?;
        let produced = produced_pdf(&work.dir).ok_or_else(|| {
            Fail::Converter(format!("{PROGRAM} wrote no PDF of {}", job.input.display()))
        })?;
        let mut body = std::fs::File::open(&produced).map_err(local)?;
        Store::publish(&dest, job.mtime, &mut body).map_err(local)?;
        self.store.prune(&self.store.dir);
        Ok(dest)
    }

    // One converter run in the jail, stopped at end, on quit or on the job's cancel.
    pub(super) fn jailed(
        &self,
        job: &Job,
        argv: Vec<String>,
        work: &mut Work,
        end: Instant,
    ) -> Result<(), Fail> {
        let (stop, finished, timed_out) = (
            AtomicBool::new(false),
            AtomicBool::new(false),
            AtomicBool::new(false),
        );
        let ran = std::thread::scope(|s| {
            s.spawn(|| {
                while !finished.load(Ordering::Relaxed) {
                    let late = Instant::now() >= end;
                    if late
                        || self.quit.load(Ordering::Relaxed)
                        || job
                            .cancel
                            .as_ref()
                            .is_some_and(|c| c.load(Ordering::Relaxed))
                    {
                        timed_out.store(late, Ordering::Relaxed);
                        stop.store(true, Ordering::Relaxed);
                        break;
                    }
                    std::thread::sleep(WATCH_STEP);
                }
            });
            let ran = (self.runner)(argv, &job.input, work, &stop);
            finished.store(true, Ordering::Relaxed);
            ran
        });
        if let Err(e) = ran {
            return Err(if timed_out.load(Ordering::Relaxed) {
                Fail::Local(format!(
                    "the conversion took longer than {} seconds",
                    self.timeout.as_secs()
                ))
            } else if self.quit.load(Ordering::Relaxed)
                || job
                    .cancel
                    .as_ref()
                    .is_some_and(|c| c.load(Ordering::Relaxed))
            {
                Fail::Local("cancelled".to_string())
            } else {
                Fail::Converter(e.msg)
            });
        }
        Ok(())
    }
}

// Only the converter's own failure is remembered for the file; a timeout, a local error or a cancel fails this request and leaves no marker.
pub(super) enum Fail {
    Converter(String),
    Local(String),
}

// The one regular, non-empty PDF the converter left in the work folder.
pub(super) fn produced_pdf(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.extension().is_some_and(|x| x == "pdf")
                && std::fs::symlink_metadata(p).is_ok_and(|m| m.is_file() && m.len() > 0)
        })
}
