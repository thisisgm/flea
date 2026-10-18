use gpui::{px, App, GpuSpecs, Window};

#[derive(Default)]
pub(crate) struct Capture {
    pending: bool,
}

impl Capture {
    pub(crate) fn new(window: &mut Window, cx: &mut App) -> Self {
        let enabled = std::env::var("FLEA_GPUI_CAPTURE").as_deref() == Ok("1");
        if enabled {
            cx.set_reduce_motion(true);
            window.set_rem_size(px(14.));
        }
        Self { pending: enabled }
    }

    pub(crate) fn report(&mut self, window: &Window, cx: &App) {
        if !self.pending {
            return;
        }
        if let Some(specs) = window.gpu_specs() {
            let text_size = window.text_style().font_size.to_pixels(window.rem_size());
            eprintln!(
                "{} scale={} text_size={} reduced_motion={}",
                renderer(&specs),
                window.scale_factor(),
                f32::from(text_size),
                cx.reduce_motion()
            );
            self.pending = false;
        }
    }
}

fn renderer(specs: &GpuSpecs) -> String {
    // Pinned wgpu HAL leaves GL's driver empty; Vulkan supplies a name or "?".
    let backend = if specs.driver_name.is_empty() {
        "gl"
    } else {
        "vulkan"
    };
    format!(
        "GPUI_RENDERER backend={backend} adapter={:?} driver={:?} driver_info={:?} software={}",
        specs.device_name, specs.driver_name, specs.driver_info, specs.is_software_emulated
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_comes_from_the_selected_adapter_not_an_environment_request() {
        let mut specs = GpuSpecs {
            device_name: "llvmpipe (LLVM 22.1.0, 256 bits)".into(),
            driver_name: "llvmpipe".into(),
            driver_info: "Mesa 26.2.2".into(),
            is_software_emulated: true,
        };
        assert!(renderer(&specs).starts_with("GPUI_RENDERER backend=vulkan adapter=\"llvmpipe"));
        specs.driver_name = "?".into();
        assert!(renderer(&specs).starts_with("GPUI_RENDERER backend=vulkan "));
        specs.driver_name.clear();
        specs.driver_info = "OpenGL ES 3.2 Mesa 26.2.2".into();
        assert!(renderer(&specs).starts_with("GPUI_RENDERER backend=gl adapter=\"llvmpipe"));
        specs.device_name = "a\nb".into();
        assert!(!renderer(&specs).contains('\n'));
        assert!(renderer(&specs).contains("software=true"));
    }
}
