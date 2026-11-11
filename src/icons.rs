//! The icons the file manager of the platform shows for files and folders

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

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

#[cfg(not(target_os = "macos"))]
fn native_icon_png(_path: &str, _pixels: u32) -> Option<Vec<u8>> {
    None
}

/// Returns the native icon of a file or folder, as the file manager of the platform shows it,
/// to draw with image(). Returns None when the platform has no icon to give: draw your own then.
///
/// For now only macOS gives icons, the ones of the Finder.
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
    let pixels = (size * ctx.pixels_per_point()).round().clamp(8.0, 1024.0) as u32;
    let key = (path.to_string(), pixels);

    Ok(ICONS.with_borrow_mut(|icons| {
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
    }))
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(file_icon, m)?)?;
    Ok(())
}
