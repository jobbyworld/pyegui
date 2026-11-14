//! The icons the file manager of the platform shows for files and folders

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

#[cfg(target_os = "windows")]
use image::ImageEncoder;
use pyo3::prelude::*;

use crate::{current_ui, UI};

thread_local! {
    // (path, pixels) -> uri of the icon given to the image loaders, None if there is no icon
    static ICONS: RefCell<HashMap<(String, u32), Option<String>>> = RefCell::new(HashMap::new());
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

        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&data, width as u32, height as u32, image::ExtendedColorType::Rgba8)
            .ok()?;
        Some(png)
    }
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
    let ctx = current_ui(&UI)?.ctx().clone();
    Ok(icon_uri(&ctx, path, size))
}

/// The uri of the native icon of a file, made once per path and size, None if the platform has none
pub(crate) fn icon_uri(ctx: &eframe::egui::Context, path: &str, size: f32) -> Option<String> {
    let pixels = (size * ctx.pixels_per_point()).round().clamp(8.0, 1024.0) as u32;
    let key = (path.to_string(), pixels);

    ICONS.with_borrow_mut(|icons| {
        icons
            .entry(key)
            .or_insert_with(|| {
                let png = native_icon_png(path, pixels)?;
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                (path, pixels).hash(&mut hasher);
                let uri = format!("bytes://file-icon-{:016x}.png", hasher.finish());
                ctx.include_bytes(uri.clone(), png);
                Some(uri)
            })
            .clone()
    })
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(file_icon, m)?)?;
    Ok(())
}
