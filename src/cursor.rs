//! Custom mouse cursors: `set_cursor` shows a cursor made from a file (`.cur`, `.ani`, `.ico`, `.png`
//! or `.svg`) while the pointer is over the window, for the frames that ask for it.
//!
//! egui only knows a fixed set of cursors, so on Windows the cursor is made and shown with the system:
//! the window is subclassed to answer `WM_SETCURSOR` with ours while a frame asks for one, and a
//! `WM_SETCURSOR` is posted when the choice changes (winit would otherwise put its own cursor back
//! until the mouse moves). Other platforms keep the cursors of egui: `set_cursor` returns False there.

use pyo3::prelude::*;

#[cfg(target_os = "windows")]
mod win {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
    use std::sync::Mutex;

    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        CreateBitmap, CreateDIBSection, DeleteObject, GetDC, ReleaseDC, ScreenToClient, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        HGDIOBJ,
    };
    use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateIconIndirect, GetClientRect, GetCursorPos, GetSystemMetrics, LoadImageW, PostMessageW, SetCursor, WindowFromPoint, HCURSOR, HTCLIENT,
        ICONINFO, IMAGE_CURSOR, LR_LOADFROMFILE, SM_CXCURSOR, WM_MOUSEMOVE, WM_SETCURSOR,
    };

    /// The window of the frame, and the cursor to show: `NEXT` is what the frame being drawn asks for,
    /// `ACTIVE` what the window shows (0 for the cursor of egui).
    static WINDOW: AtomicIsize = AtomicIsize::new(0);
    static NEXT: AtomicIsize = AtomicIsize::new(0);
    static ACTIVE: AtomicIsize = AtomicIsize::new(0);
    static INSTALLED: AtomicBool = AtomicBool::new(false);
    const SUBCLASS_ID: usize = 0x70_79_65_67; // "pyeg"

    /// The cursors made, by what they are made from: a handle is made once and kept for the whole run.
    static MADE: Mutex<Option<HashMap<String, isize>>> = Mutex::new(None);

    unsafe extern "system" fn subclass(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
        if msg == WM_SETCURSOR && (lparam.0 & 0xFFFF) as u32 == HTCLIENT {
            let handle = ACTIVE.load(Ordering::Relaxed);
            if handle != 0 {
                unsafe { SetCursor(Some(HCURSOR(handle as *mut _))) };
                return LRESULT(1);
            }
        }
        unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
    }

    pub fn set_window(hwnd: isize) {
        WINDOW.store(hwnd, Ordering::Relaxed);
        if hwnd != 0 && !INSTALLED.swap(true, Ordering::Relaxed) {
            unsafe {
                let _ = SetWindowSubclass(HWND(hwnd as *mut _), Some(subclass), SUBCLASS_ID, 0);
            }
        }
    }

    /// A frame starts: it asks for nothing yet.
    pub fn begin_frame() {
        NEXT.store(0, Ordering::Relaxed);
    }

    /// A frame ends: when what it asked for is not what is shown, the window is told to choose again.
    pub fn end_frame() {
        let next = NEXT.load(Ordering::Relaxed);
        if ACTIVE.swap(next, Ordering::Relaxed) == next {
            return;
        }
        let hwnd = WINDOW.load(Ordering::Relaxed);
        if hwnd == 0 {
            return;
        }
        unsafe {
            let hwnd = HWND(hwnd as *mut _);
            // only while the pointer is over the client area of this window
            let mut at = POINT::default();
            if GetCursorPos(&mut at).is_err() || WindowFromPoint(at) != hwnd {
                return;
            }
            let mut client = RECT::default();
            let mut local = at;
            if !ScreenToClient(hwnd, &mut local).as_bool() || GetClientRect(hwnd, &mut client).is_err() {
                return;
            }
            if local.x < client.left || local.y < client.top || local.x >= client.right || local.y >= client.bottom {
                return;
            }
            let lparam = ((WM_MOUSEMOVE as isize) << 16) | HTCLIENT as isize;
            let _ = PostMessageW(Some(hwnd), WM_SETCURSOR, WPARAM(hwnd.0 as usize), LPARAM(lparam));
        }
    }

    /// Asks for a cursor for this frame: `0` for the one of egui.
    pub fn ask(handle: isize) {
        NEXT.store(handle, Ordering::Relaxed);
    }

    /// The size of the cursors of the system, 32 pixels at 100% of scaling.
    pub fn system_size() -> u32 {
        let size = unsafe { GetSystemMetrics(SM_CXCURSOR) };
        if size > 0 { size as u32 } else { 32 }
    }

    /// The cursor made from `key`, made by `make` the first time.
    pub fn cached(key: &str, make: impl FnOnce() -> Result<isize, String>) -> Result<isize, String> {
        let mut made = MADE.lock().unwrap_or_else(|e| e.into_inner());
        let map = made.get_or_insert_with(HashMap::new);
        if let Some(handle) = map.get(key) {
            return Ok(*handle);
        }
        let handle = make()?;
        map.insert(key.to_owned(), handle);
        Ok(handle)
    }

    /// A cursor of a `.cur` or `.ani` file, with the hotspot the file has.
    pub fn from_cursor_file(path: &str, size: Option<u32>) -> Result<isize, String> {
        let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
        let side = size.unwrap_or(0) as i32; // 0: the size of the file
        let handle = unsafe { LoadImageW(None, PCWSTR(wide.as_ptr()), IMAGE_CURSOR, side, side, LR_LOADFROMFILE) }.map_err(|e| format!("cannot load {path}: {e}"))?;
        Ok(handle.0 as isize)
    }

    /// A cursor from the pixels of a picture: straight (not premultiplied) RGBA, `width` x `height`.
    pub fn from_rgba(width: u32, height: u32, rgba: &[u8], hotspot: (i32, i32)) -> Result<isize, String> {
        if width == 0 || height == 0 || rgba.len() != width as usize * height as usize * 4 {
            return Err("the cursor picture has a wrong size".into());
        }
        unsafe {
            let screen = GetDC(None);
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width as i32,
                    biHeight: -(height as i32), // from the top
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
            let color = CreateDIBSection(Some(screen), &info, DIB_RGB_COLORS, &mut bits, None, 0);
            ReleaseDC(None, screen);
            let color = color.map_err(|e| format!("cannot make the bitmap of the cursor: {e}"))?;
            if bits.is_null() {
                let _ = DeleteObject(HGDIOBJ(color.0));
                return Err("cannot make the bitmap of the cursor".into());
            }
            // BGRA, as the system draws a cursor with an alpha channel
            let out = std::slice::from_raw_parts_mut(bits as *mut u8, rgba.len());
            for (dst, src) in out.chunks_exact_mut(4).zip(rgba.chunks_exact(4)) {
                dst.copy_from_slice(&[src[2], src[1], src[0], src[3]]);
            }
            // the mask is not read when the colors have an alpha channel, but it has to exist
            let mask = CreateBitmap(width as i32, height as i32, 1, 1, None);
            let icon = ICONINFO { fIcon: false.into(), xHotspot: hotspot.0.max(0) as u32, yHotspot: hotspot.1.max(0) as u32, hbmMask: mask, hbmColor: color };
            let made = CreateIconIndirect(&icon);
            let _ = DeleteObject(HGDIOBJ(color.0));
            let _ = DeleteObject(HGDIOBJ(mask.0));
            made.map(|h| h.0 as isize).map_err(|e| format!("cannot make the cursor: {e}"))
        }
    }
}

/// The pixels of an SVG at `size` pixels on its longest side: straight RGBA, and the scale of the picture.
#[cfg(target_os = "windows")]
fn render_svg(data: &[u8], size: u32) -> Result<(u32, u32, Vec<u8>, f32), String> {
    let tree = usvg::Tree::from_data(data, &usvg::Options::default()).map_err(|e| format!("not a readable SVG: {e}"))?;
    let (w, h) = (tree.size().width(), tree.size().height());
    if !(w > 0.0 && h > 0.0) {
        return Err("the SVG has no size".into());
    }
    let k = size as f32 / w.max(h);
    let (pw, ph) = (((w * k).round() as u32).max(1), ((h * k).round() as u32).max(1));
    let mut pixmap = resvg::tiny_skia::Pixmap::new(pw, ph).ok_or("the cursor is too big")?;
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(k, k), &mut pixmap.as_mut());
    // the pixmap is premultiplied: the system wants straight alpha
    let mut rgba = pixmap.take();
    for px in rgba.chunks_exact_mut(4) {
        let a = u32::from(px[3]);
        if a > 0 && a < 255 {
            for c in &mut px[..3] {
                *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    Ok((pw, ph, rgba, k))
}

/// Makes the cursor of a file: `.cur` and `.ani` keep their own hotspot, a picture or an SVG gets `hotspot`
/// (in its own pixels, or units for an SVG) scaled with the picture.
#[cfg(target_os = "windows")]
fn load_file(path: &str, hotspot: (i32, i32), size: Option<u32>) -> Result<isize, String> {
    let extension = std::path::Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match extension.as_str() {
        "cur" | "ani" => win::from_cursor_file(path, size),
        "svg" => {
            let data = std::fs::read(path).map_err(|e| format!("cannot read {path}: {:?}", e.kind()))?;
            svg_cursor(&data, hotspot, size)
        }
        _ => {
            let picture = image::open(path).map_err(|e| format!("cannot read {path}: {e}"))?.to_rgba8();
            let (mut w, mut h) = picture.dimensions();
            let mut k = 1.0f32;
            let picture = match size {
                Some(side) if side > 0 && w.max(h) != side => {
                    k = side as f32 / w.max(h) as f32;
                    (w, h) = (((w as f32 * k).round() as u32).max(1), ((h as f32 * k).round() as u32).max(1));
                    image::imageops::resize(&picture, w, h, image::imageops::FilterType::Triangle)
                }
                _ => picture,
            };
            win::from_rgba(w, h, picture.as_raw(), ((hotspot.0 as f32 * k).round() as i32, (hotspot.1 as f32 * k).round() as i32))
        }
    }
}

#[cfg(target_os = "windows")]
fn svg_cursor(data: &[u8], hotspot: (i32, i32), size: Option<u32>) -> Result<isize, String> {
    let (w, h, rgba, k) = render_svg(data, size.unwrap_or_else(win::system_size))?;
    win::from_rgba(w, h, &rgba, ((hotspot.0 as f32 * k).round() as i32, (hotspot.1 as f32 * k).round() as i32))
}

/// The window of the frame, where the cursor is shown (Windows).
#[cfg(target_os = "windows")]
pub(crate) fn begin_frame(window: &impl raw_window_handle::HasWindowHandle) {
    if let Ok(handle) = window.window_handle() {
        if let raw_window_handle::RawWindowHandle::Win32(h) = handle.as_raw() {
            win::set_window(h.hwnd.get());
        }
    }
    win::begin_frame();
}

/// What the frame asked for is shown (Windows).
#[cfg(target_os = "windows")]
pub(crate) fn end_frame() {
    win::end_frame();
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn begin_frame(_window: &impl raw_window_handle::HasWindowHandle) {}

#[cfg(not(target_os = "windows"))]
pub(crate) fn end_frame() {}

/// An open hand and a closed hand, drawn for the pan of the PDF viewer: 32 x 32, the hotspot at the middle.
#[cfg(target_os = "windows")]
const HAND_OPEN: &str = include_str!("cursors/hand_open.svg");
#[cfg(target_os = "windows")]
const HAND_CLOSED: &str = include_str!("cursors/hand_closed.svg");

/// Shows the open hand (or the closed one while `closed`) for this frame. Returns False when the platform
/// has no custom cursors: the caller then picks one of egui.
#[cfg(target_os = "windows")]
pub(crate) fn hand(closed: bool) -> bool {
    let (key, svg) = if closed { ("pyegui:hand-closed", HAND_CLOSED) } else { ("pyegui:hand-open", HAND_OPEN) };
    match win::cached(key, || svg_cursor(svg.as_bytes(), (16, 16), None)) {
        Ok(handle) => {
            win::ask(handle);
            true
        }
        Err(_) => false,
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn hand(_closed: bool) -> bool {
    false
}

/// Shows a custom cursor, made from a picture, for this frame, while the pointer is over the window.
/// Call it at every frame that wants the cursor (like the cursor of a widget): a frame that does not
/// call it shows the usual cursor again.
///
/// The file is `.cur` or `.ani` (a Windows cursor, with its own hotspot), `.ico` or `.png` (a picture
/// that may be transparent), or `.svg` (drawn at the size of the cursors of the system, sharp at any
/// scale). The cursor is made once for each file and size and kept.
///
/// Only on Windows for now: elsewhere nothing is shown and False is returned, so the app can fall back
/// on the usual cursor, which the widgets pick by themselves.
///
/// Args:
///     source (str | None): path of the file, or None for the usual cursor
///
///     hotspot (tuple[int, int]): the point of the picture that is the pointer, in its pixels (the units
///         of an SVG); ignored for `.cur` and `.ani`, which have their own. Default is the top left
///
///     size (int): the size in pixels of the longest side, the size of the cursors of the system by
///         default for an SVG (32 at 100% of scaling) and the size of the picture otherwise
///
/// Returns True when the cursor is shown, False when it could not be made (the file is not found or not
/// readable, or the platform has no custom cursors).
///
/// Example::
///
///     def update_func(ctx):
///         button_clicked("Draw")
///         if pointer_over_the_canvas():
///             set_cursor("pen.svg", hotspot=(2, 30))
#[pyfunction]
#[pyo3(signature = (source, hotspot=(0, 0), size=None), text_signature = "(source, hotspot=(0, 0), size=None)")]
fn set_cursor(source: Option<&str>, hotspot: (i32, i32), size: Option<u32>) -> PyResult<bool> {
    #[cfg(target_os = "windows")]
    {
        let Some(path) = source else {
            win::ask(0);
            return Ok(true);
        };
        let key = format!("{path}|{}|{}|{}", hotspot.0, hotspot.1, size.unwrap_or(0));
        return match win::cached(&key, || load_file(path, hotspot, size)) {
            Ok(handle) => {
                win::ask(handle);
                Ok(true)
            }
            Err(error) => {
                log::warn!("set_cursor: {error}");
                Ok(false)
            }
        };
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (source, hotspot, size);
        Ok(false)
    }
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(set_cursor, m)?)?;
    Ok(())
}
