//! One renderer-selection sentence, using GPUI's actual adapter and any rejected Vulkan attempt.
use log::{Level, LevelFilter, Log, Metadata, Record, SetLoggerError};
use std::cell::RefCell;

const RENDERER: &str = "gpui_wgpu::wgpu_renderer";
const CONTEXT: &str = "gpui_wgpu::wgpu_context";
static LOGGER: BackendLog = BackendLog;

thread_local! {
    // GPUI tries both backends synchronously on the window-opening thread.
    static VULKAN_REJECTION: RefCell<Option<String>> = const { RefCell::new(None) };
}

pub fn install() -> Result<(), SetLoggerError> {
    log::set_logger(&LOGGER)?;
    log::set_max_level(LevelFilter::Info);
    Ok(())
}

struct BackendLog;

impl Log for BackendLog {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        matches!(
            (metadata.target(), metadata.level()),
            (RENDERER, Level::Warn) | (CONTEXT, Level::Info)
        )
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let message = record.args().to_string();
        if record.target() == RENDERER {
            // Sample input: "No usable Backends(VULKAN) adapter for this window: Failed to create surface: ...".
            if let Some(reason) =
                message.strip_prefix("No usable Backends(VULKAN) adapter for this window: ")
            {
                VULKAN_REJECTION.with(|held| *held.borrow_mut() = Some(one_line(reason)));
            }
            return;
        }
        if let Some((adapter, backend)) = selected(&message) {
            let rejection = VULKAN_REJECTION.with(|held| held.borrow_mut().take());
            if let Some(reason) = rejection.filter(|_| backend == "Gl") {
                eprintln!("flea: Vulkan is unusable, {reason}, so the window starts on OpenGL ({adapter})");
            } else {
                let backend = if backend == "Gl" { "OpenGL" } else { backend };
                eprintln!("flea: the window starts on {backend} ({adapter})");
            }
        }
    }

    fn flush(&self) {}
}

// Sample input: "Selected GPU adapter: \"AMD Radeon RX 7900 XTX\" (Vulkan)".
fn selected(message: &str) -> Option<(String, &str)> {
    let selected = message.strip_prefix("Selected GPU adapter: ")?;
    let (adapter, backend) = selected.rsplit_once(" (")?;
    Some((one_line(adapter), backend.strip_suffix(')')?))
}

fn one_line(message: &str) -> String {
    message.split_whitespace().collect::<Vec<_>>().join(" ")
}
