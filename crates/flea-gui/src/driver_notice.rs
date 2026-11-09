//! The desktop notice for a start that opened no window, in the user's language.
use flea::i18n::I18n;
use std::io::IsTerminal;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const FLAGS: [&str; 3] = [
    "--app-name=Flea",
    "--urgency=critical",
    "--icon=com.thisisgm.flea",
];
// The package names and the command stay literal in every language.
const BODY_ARGUMENTS: [&str; 3] = ["vulkan-swrast", "mesa", "flea --tui"];
// notify-send returns once the daemon has the notice; an absent daemon must not hold the exit for D-Bus's 25 s timeout.
const WAIT: Duration = Duration::from_secs(2);
const POLL: Duration = Duration::from_millis(10);

pub struct DriverNotice {
    summary: String,
    body: String,
}

impl DriverNotice {
    // A language whose catalog is missing or did not load reads the English built into the binary.
    pub fn new(catalog: &I18n) -> Self {
        Self {
            summary: catalog.tr("window.no_driver"),
            body: catalog.tr_args("window.no_driver_hint", &BODY_ARGUMENTS),
        }
    }

    // A launch from the app launcher, a key binding or the file-chooser portal has no terminal to read stderr.
    pub fn send(&self) {
        if let Some(command) = self.command(std::io::stderr().is_terminal()) {
            show(command);
        }
    }

    fn command(&self, stderr_is_terminal: bool) -> Option<Command> {
        if stderr_is_terminal {
            return None;
        }
        let mut command = Command::new("notify-send");
        command
            .args(FLAGS)
            .args([&self.summary, &self.body])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        Some(command)
    }
}

fn show(mut command: Command) {
    let Ok(mut child) = command.spawn() else {
        return;
    };
    let deadline = Instant::now() + WAIT;
    while matches!(child.try_wait(), Ok(None)) && Instant::now() < deadline {
        std::thread::sleep(POLL);
    }
}

#[cfg(test)]
#[path = "driver_notice_tests.rs"]
mod tests;
