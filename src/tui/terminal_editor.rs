// An editor as its own foreground job: it takes the terminal, and close always hands it back.
use super::*;

const SIGTTIN: i32 = 21;

pub(super) struct EditorJob {
    child: Child,
    foreground: i32,
    handler: usize,
    finished: bool,
    restored: bool,
}
impl EditorJob {
    pub(super) fn start(mut command: Command) -> io::Result<Self> {
        let foreground = unsafe { tcgetpgrp(0) };
        if foreground < 0 { return Err(io::Error::last_os_error()); }
        const IGNORE_SIGNAL: usize = 1;
        let handler = unsafe { signal(SIGTTOU, IGNORE_SIGNAL) };
        let child = command.process_group(0).spawn();
        let child = match child {
            Ok(child) => child,
            Err(error) => { unsafe { signal(SIGTTOU, handler); } return Err(crate::tui::launch::missing(error, &command)); }
        };
        let job = Self { child, foreground, handler, finished: false, restored: false };
        if unsafe { tcsetpgrp(0, job.child.id() as i32) } != 0 { return Err(io::Error::last_os_error()); }
        Ok(job)
    }
    pub(super) fn status(&mut self) -> io::Result<Option<std::process::ExitStatus>> {
        const NO_HANG: i32 = 1;
        const UNTRACED: i32 = 2;
        let mut status = 0;
        let result = unsafe { waitpid(self.child.id() as i32, &mut status, NO_HANG | UNTRACED) };
        if result < 0 {
            let error = io::Error::last_os_error();
            return if error.kind() == io::ErrorKind::Interrupted { Ok(None) } else { Err(error) };
        }
        if result == 0 { return Ok(None); }
        let status = std::process::ExitStatus::from_raw(status);
        // Only a pre-handoff terminal access stop (SIGTTIN, SIGTTOU) is continued; Ctrl+Z's SIGTSTP is the user suspending it.
        match status.stopped_signal() {
            Some(SIGTTIN | SIGTTOU) => {
                crate::procsignal::signal_group(self.child.id() as i32, SIGCONT)?;
                return Ok(None);
            }
            Some(_) => return Err(crate::tui::words::error("tui.terminal_editor.editor_suspended_editing_cancelled")),
            None => {}
        }
        self.finished = true;
        Ok(Some(status))
    }
    pub(super) fn close(&mut self) -> io::Result<()> {
        self.restored = true;
        let mut cleanup = Ok(());
        if !self.finished {
            const NO_SUCH_PROCESS: i32 = 3;
            match crate::procsignal::signal_group(self.child.id() as i32, SIGKILL) {
                Err(error) if error.raw_os_error() != Some(NO_SUCH_PROCESS) => cleanup = Err(error),
                _ => cleanup = self.child.wait().map(|_| ()),
            }
        }
        let restored = if unsafe { tcsetpgrp(0, self.foreground) } == 0 { Ok(()) } else { Err(io::Error::last_os_error()) };
        unsafe { signal(SIGTTOU, self.handler); }
        cleanup.and(restored)
    }
}
impl Drop for EditorJob {
    fn drop(&mut self) {
        if !self.restored { if let Err(error) = self.close() { eprintln!("flea: editor cleanup: {}", error); } }
    }
}
