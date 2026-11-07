//! One renderer-selection sentence, using GPUI's actual adapter and any rejected Vulkan attempt, and each refused adapter for a failed start.
use log::{Level, LevelFilter, Log, Metadata, Record, SetLoggerError};
use std::cell::RefCell;

const RENDERER: &str = "gpui_wgpu::wgpu_renderer";
const CONTEXT: &str = "gpui_wgpu::wgpu_context";
static LOGGER: BackendLog = BackendLog;

thread_local! {
    // GPUI tries both backends synchronously on the window-opening thread.
    static VULKAN_REJECTION: RefCell<Option<String>> = const { RefCell::new(None) };
    static REFUSALS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

// The adapters GPUI tested and refused since the last selection, for the report when no window opens.
pub fn take_refusals() -> Vec<String> {
    REFUSALS.with(|held| held.take())
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
        if let Some(refusal) = refused(&message) {
            REFUSALS.with(|held| held.borrow_mut().push(refusal));
            return;
        }
        if let Some((adapter, backend)) = selected(&message) {
            REFUSALS.with(|held| held.borrow_mut().clear());
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

// Sample input: "  Adapter llvmpipe (Gl) failed: Failed to create wgpu device: Limit 'max_storage_buffer_binding_size' value 134217728 is better than allowed 0, trying next...".
fn refused(message: &str) -> Option<String> {
    let tested = message.trim_start().strip_prefix("Adapter ")?;
    let (adapter, reason) = tested
        .strip_suffix(", trying next...")?
        .split_once(" failed: ")?;
    Some(format!(
        "{} was refused: {}",
        one_line(adapter),
        one_line(reason)
    ))
}

fn one_line(message: &str) -> String {
    message.split_whitespace().collect::<Vec<_>>().join(" ")
}
