//! C ABI for the browser engine. This is the only crate the Swift shell links against.
//!
//! Ownership: `browser_engine_new` returns an owning handle that must be released with
//! `browser_engine_free`. The pixel pointer in the `Framebuffer` returned by
//! `browser_engine_render` is owned by the engine and stays valid until the next
//! `browser_engine_render` or `browser_engine_free` call on the same handle.

use std::ffi::c_char;
use std::ffi::CStr;

/// Opaque engine handle. cbindgen emits this as a forward-declared struct.
pub struct Engine(engine::Engine);

/// A borrowed view of the engine's RGBA8 (straight-alpha) framebuffer.
/// `stride` is bytes per row. A null `pixels` means "nothing rendered".
#[repr(C)]
pub struct Framebuffer {
    pub pixels: *const u8,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
}

impl Framebuffer {
    fn empty() -> Self {
        Framebuffer { pixels: std::ptr::null(), width: 0, height: 0, stride: 0 }
    }
}

/// Create a new engine. Release with [`browser_engine_free`].
#[no_mangle]
pub extern "C" fn browser_engine_new() -> *mut Engine {
    Box::into_raw(Box::new(Engine(engine::Engine::new())))
}

/// Free an engine created by [`browser_engine_new`]. Null is a no-op.
///
/// # Safety
/// `engine` must be a pointer returned by [`browser_engine_new`] and not used afterwards.
#[no_mangle]
pub unsafe extern "C" fn browser_engine_free(engine: *mut Engine) {
    if !engine.is_null() {
        drop(Box::from_raw(engine));
    }
}

/// Set the logical viewport size (points) and backing scale (e.g. 2.0 on Retina).
///
/// # Safety
/// `engine` must be a valid handle from [`browser_engine_new`].
#[no_mangle]
pub unsafe extern "C" fn browser_engine_set_viewport(
    engine: *mut Engine,
    width: u32,
    height: u32,
    scale: f32,
) {
    if let Some(e) = engine.as_mut() {
        e.0.set_viewport(width, height, scale);
    }
}

/// Navigate to `url` (NUL-terminated UTF-8). Returns 0 on success, negative on error:
/// -1 fetch/network failure, -2 bad arguments.
///
/// # Safety
/// `engine` must be a valid handle; `url` must be a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn browser_engine_load_url(engine: *mut Engine, url: *const c_char) -> i32 {
    let Some(e) = engine.as_mut() else { return -2 };
    if url.is_null() {
        return -2;
    }
    match CStr::from_ptr(url).to_str() {
        Ok(s) => e.0.load_url(s),
        Err(_) => -2,
    }
}

/// Paint the current state and return a borrowed view of the framebuffer.
/// Valid until the next render/free on this handle.
///
/// # Safety
/// `engine` must be a valid handle from [`browser_engine_new`].
#[no_mangle]
pub unsafe extern "C" fn browser_engine_render(engine: *mut Engine) -> Framebuffer {
    let Some(e) = engine.as_mut() else { return Framebuffer::empty() };
    let fb = e.0.render();
    Framebuffer {
        pixels: fb.pixels.as_ptr(),
        width: fb.width,
        height: fb.height,
        stride: fb.stride,
    }
}
