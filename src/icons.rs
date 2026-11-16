//! The icons the file manager of the platform shows for files and folders

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

#[cfg(target_os = "windows")]
use image::ImageEncoder;
use pyo3::prelude::*;

use crate::{panel_parent_ui, UI};

thread_local! {
    // (path, pixels) -> uri of the icon given to the image loaders, None if there is no icon
    static ICONS: RefCell<HashMap<(String, u32, bool), Option<String>>> = RefCell::new(HashMap::new());
    // the folders of Windows are drawn blue, like in the Finder
    static BLUE_FOLDERS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Draws the icons of the folders in blue, in the style of the Finder of macOS, instead of the
/// yellow of the Explorer of Windows. It changes the icons given by file_icon and drawn by the
/// file views. It has no effect on macOS (already blue) nor on Linux. Default is False
///
/// Args:
///     enabled (bool): True for blue folders, False for the native ones
///
/// Example::
///
///     set_blue_folders(True)  # once, before the first frame
#[pyfunction]
#[pyo3(signature = (enabled=true))]
fn set_blue_folders(enabled: bool) {
    BLUE_FOLDERS.with(|b| b.set(enabled));
}

/// The icon of a file as a PNG, from NSWorkspace like the Finder shows it
#[cfg(target_os = "macos")]
fn native_icon_png(path: &str, pixels: u32) -> Option<Vec<u8>> {
    use objc2::AnyThread;
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSWorkspace};
    use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize, NSString};

    let image = NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(path));
    let side = f64::from(pixels);
    let mut rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(side, side));
    // the representation of the icon that fits this size
    let cg_image = unsafe { image.CGImageForProposedRect_context_hints(&mut rect, None, None) }?;
    let bitmap = NSBitmapImageRep::initWithCGImage(NSBitmapImageRep::alloc(), &cg_image);
    let png = unsafe {
        bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
    }?;
    Some(png.to_vec())
}

/// The icon of a file as a PNG, from the shell like the Explorer shows it
#[cfg(target_os = "windows")]
fn native_icon_png(path: &str, pixels: u32) -> Option<Vec<u8>> {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::SIZE;
    use windows::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
        BI_RGB, DIB_RGB_COLORS,
    };
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{
        IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_ICONONLY, SIIGBF_BIGGERSIZEOK,
    };

    unsafe {
        // already initialized by the window system on the GUI thread: the error is harmless
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        // the shell wants backslashes
        let wide = HSTRING::from(path.replace('/', "\\"));
        let factory: IShellItemImageFactory = SHCreateItemFromParsingName(&wide, None).ok()?;
        let side = pixels as i32;
        let hbitmap = factory
            .GetImage(SIZE { cx: side, cy: side }, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK)
            .ok()?;

        let mut bitmap = BITMAP::default();
        GetObjectW(
            hbitmap.into(),
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bitmap as *mut _ as *mut _),
        );
        let (width, height) = (bitmap.bmWidth, bitmap.bmHeight.abs());
        if width <= 0 || height <= 0 {
            let _ = DeleteObject(hbitmap.into());
            return None;
        }

        let mut info = BITMAPINFO::default();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height, // top-down rows
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };
        let mut data = vec![0u8; (width * height * 4) as usize];
        let dc = GetDC(None);
        let lines = GetDIBits(
            dc,
            hbitmap,
            0,
            height as u32,
            Some(data.as_mut_ptr() as *mut _),
            &mut info,
            DIB_RGB_COLORS,
        );
        ReleaseDC(None, dc);
        let _ = DeleteObject(hbitmap.into());
        if lines == 0 {
            return None;
        }

        // BGRA -> RGBA, and opaque when the icon carries no alpha at all
        let opaque = data.chunks_exact(4).all(|px| px[3] == 0);
        for px in data.chunks_exact_mut(4) {
            px.swap(0, 2);
            if opaque {
                px[3] = 255;
            }
        }
        if BLUE_FOLDERS.with(|b| b.get()) && std::path::Path::new(path).is_dir() {
            for px in data.chunks_exact_mut(4) {
                yellow_to_blue(px);
            }
        }

        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&data, width as u32, height as u32, image::ExtendedColorType::Rgba8)
            .ok()?;
        Some(png)
    }
}

/// Turns a yellow pixel (RGBA) of the folder of Windows into the blue of the Finder: the hue
/// moves, the saturation and the brightness stay, so the shades of the folder are kept
#[cfg(target_os = "windows")]
fn yellow_to_blue(px: &mut [u8]) {
    let (r, g, b) = (px[0] as f32 / 255.0, px[1] as f32 / 255.0, px[2] as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    if max <= 0.0 || delta / max < 0.2 {
        return; // grey, white or transparent: nothing to color
    }
    let hue = if max == r {
        60.0 * (((g - b) / delta).rem_euclid(6.0))
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    if !(20.0..=70.0).contains(&hue) {
        return;
    }
    // yellow (45) to the blue of the Finder (about 208), with a little of the spread kept
    let hue = 208.0 + (hue - 45.0) * 0.3;
    let sat = (delta / max * 1.15).min(1.0);
    let h = hue / 60.0;
    let c = max * sat;
    let x = c * (1.0 - (h.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = max - c;
    px[0] = ((r + m) * 255.0).round() as u8;
    px[1] = ((g + m) * 255.0).round() as u8;
    px[2] = ((b + m) * 255.0).round() as u8;
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn native_icon_png(_path: &str, _pixels: u32) -> Option<Vec<u8>> {
    None
}

/// Returns the native icon of a file or folder, as the file manager of the platform shows it,
/// to draw with image(). Returns None when the platform has no icon to give: draw your own then.
///
/// macOS gives the icons of the Finder and Windows the ones of the Explorer (the shell, so the
/// icons of the file types installed on the machine, thumbnails excluded). Linux gives none.
///
/// Args:
///     path (str): path of the file or folder
///
///     size (float): size of the icon in points, it is rendered for the pixel density of the screen.
///         Default is 32
///
/// Example::
///
///     icon = file_icon("/Applications", 48)
///     if icon:
///         image(icon, max_width=48, max_height=48)
///     else:
///         label("📁", size=40)
#[pyfunction]
#[pyo3(signature = (path, size=32.0))]
unsafe fn file_icon(path: &str, size: f32) -> PyResult<Option<String>> {
    let ctx = panel_parent_ui(&UI)?.ctx().clone(); // no widget: the central panel is not opened
    Ok(icon_uri(&ctx, path, size))
}

/// The uri of the native icon of a file, made once per path and size, None if the platform has none
pub(crate) fn icon_uri(ctx: &eframe::egui::Context, path: &str, size: f32) -> Option<String> {
    let pixels = (size * ctx.pixels_per_point()).round().clamp(8.0, 1024.0) as u32;
    let blue = BLUE_FOLDERS.with(|b| b.get());
    let key = (path.to_string(), pixels, blue);

    ICONS.with_borrow_mut(|icons| {
        icons
            .entry(key)
            .or_insert_with(|| {
                let png = native_icon_png(path, pixels)?;
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                (path, pixels, blue).hash(&mut hasher);
                let uri = format!("bytes://file-icon-{:016x}.png", hasher.finish());
                ctx.include_bytes(uri.clone(), png);
                Some(uri)
            })
            .clone()
    })
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(file_icon, m)?)?;
    m.add_function(wrap_pyfunction!(set_blue_folders, m)?)?;
    Ok(())
}
