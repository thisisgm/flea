//! The software-render API calls shared by the playback loop.
use super::*;

pub(super) fn frame_due(api: &Api, context: Handle) -> Result<bool, String> {
    let mut info = FrameInfo {
        flags: 0,
        target_time: 0,
    };
    let parameter = Param {
        kind: RENDER_NEXT_FRAME_INFO,
        data: (&mut info as *mut FrameInfo).cast(),
    };
    // SAFETY: NEXT_FRAME_INFO fills an mpv_render_frame_info.
    check(
        unsafe { (api.render_info)(context, parameter) },
        "frame info",
    )?;
    Ok(info.flags & FRAME_INFO_PRESENT != 0)
}

pub(super) fn render(
    api: &Api,
    context: Handle,
    width: u32,
    height: u32,
    stride: usize,
    buffer: &mut [u8],
) -> Result<(), String> {
    let mut size = [width as c_int, height as c_int];
    let mut stride = stride;
    let mut parameters = [
        Param {
            kind: RENDER_SW_SIZE,
            data: size.as_mut_ptr().cast(),
        },
        Param {
            kind: RENDER_SW_FORMAT,
            data: c"bgr0".as_ptr().cast_mut().cast(),
        },
        Param {
            kind: RENDER_SW_STRIDE,
            data: (&mut stride as *mut usize).cast(),
        },
        Param {
            kind: RENDER_SW_POINTER,
            data: buffer.as_mut_ptr().cast(),
        },
        Param {
            kind: 0,
            data: ptr::null_mut(),
        },
    ];
    // SAFETY: the buffer holds stride * height bytes and outlives the call.
    check(
        unsafe { (api.render)(context, parameters.as_mut_ptr()) },
        "render",
    )
}
