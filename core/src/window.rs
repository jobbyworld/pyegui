//! Native effects behind a window: backdrops (mica, acrylic, vibrancy, liquid glass), the blur of
//! the window server and the corners of the window. Shared by the Python extension and the Rust apps.

/// Applies a native blur effect behind the window
pub fn apply_backdrop(
    window: &impl raw_window_handle::HasWindowHandle,
    backdrop: &str,
) -> Result<(), String> {
    use window_vibrancy::*;

    // "_light" and "_dark" force the tint, otherwise it follows the theme of the system
    let (name, dark) = match backdrop.rsplit_once('_') {
        Some((name, "light")) => (name, Some(false)),
        Some((name, "dark")) => (name, Some(true)),
        _ => (backdrop, None),
    };

    // on macOS the effect view must sit behind the view egui renders into, see backdrop_host
    #[cfg(target_os = "macos")]
    let host = backdrop_host(window)?;
    #[cfg(target_os = "macos")]
    let window = &host;

    let result = match name {
        "mica" => apply_mica(window, dark),
        "tabbed" => apply_tabbed(window, dark),
        "acrylic" => {
            if let Some(dark) = dark {
                set_dark_mode(window, dark)?;
            }
            apply_acrylic(window, None)
        }
        "blur" => apply_blur(window, None),
        "vibrancy" => {
            apply_vibrancy(window, NSVisualEffectMaterial::UnderWindowBackground, None, None)
        }
        // the glass is rounded like the corners of the window, which clip the content of egui:
        // a square glass shows in the corners, around the rounded content
        #[cfg(target_os = "macos")]
        "liquid_glass" => apply_liquid_glass(window, LiquidGlassOptions::default().radius(corner_radius(window))),
        // barely blurred: the windows behind stay readable
        #[cfg(target_os = "macos")]
        "liquid_glass_clear" => apply_liquid_glass(
            window,
            LiquidGlassOptions::new(NSGlassEffectViewStyle::Clear).radius(corner_radius(window)),
        ),
        _ => return Err("not supported on this platform".to_string()),
    };
    result.map_err(|e| e.to_string())
}

/// The window handle given to window-vibrancy on macOS.
///
/// window-vibrancy adds its effect view as a subview of the handle's view. egui draws with
/// OpenGL directly into the layer of winit's view, and AppKit always draws subviews above their
/// parent's own content, so the effect would cover the whole UI. Pointing the handle at the
/// superview (the window frame) puts the effect view behind winit's view instead.
#[cfg(target_os = "macos")]
struct BackdropHost(raw_window_handle::AppKitWindowHandle);

#[cfg(target_os = "macos")]
impl raw_window_handle::HasWindowHandle for BackdropHost {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        // the frame view lives as long as the window that lends us its handle
        Ok(unsafe {
            raw_window_handle::WindowHandle::borrow_raw(raw_window_handle::RawWindowHandle::AppKit(
                self.0,
            ))
        })
    }
}

#[cfg(target_os = "macos")]
fn backdrop_host(window: &impl raw_window_handle::HasWindowHandle) -> Result<BackdropHost, String> {
    use objc2_app_kit::NSView;

    let raw_window_handle::RawWindowHandle::AppKit(handle) =
        window.window_handle().map_err(|e| e.to_string())?.as_raw()
    else {
        return Err("not an AppKit window".to_string());
    };
    let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
    let superview = unsafe { view.superview() }.ok_or("the window has no frame view")?;
    let ptr = std::ptr::NonNull::from(&*superview).cast();
    Ok(BackdropHost(raw_window_handle::AppKitWindowHandle::new(ptr)))
}

/// A window found by `claim_new_window`: its content view, as a handle for the other functions
#[cfg(target_os = "macos")]
pub struct NativeWindow(raw_window_handle::AppKitWindowHandle);

#[cfg(target_os = "macos")]
impl raw_window_handle::HasWindowHandle for NativeWindow {
    fn window_handle(&self) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(raw_window_handle::RawWindowHandle::AppKit(self.0)) })
    }
}

/// Marks the window as known, so that `claim_new_window` does not return it
#[cfg(target_os = "macos")]
pub fn mark_window(window: &impl raw_window_handle::HasWindowHandle) {
    if let Ok(raw_window_handle::RawWindowHandle::AppKit(handle)) = window.window_handle().map(|h| h.as_raw()) {
        unsafe { mark_view(handle.ns_view.as_ptr().cast()) };
    }
}

/// Puts the marker in the frame view of the window of this content view; false if it was there
#[cfg(target_os = "macos")]
unsafe fn mark_view(view: *mut objc2::runtime::AnyObject) -> bool {
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject, Bool as ObjcBool};
    use objc2_foundation::{NSRect, NSString};
    let host: *mut AnyObject = msg_send![&*view, superview];
    if host.is_null() {
        return false;
    }
    let identifier = NSString::from_str("pyegui.window-known");
    let subviews: *mut AnyObject = msg_send![&*host, subviews];
    let count: usize = msg_send![&*subviews, count];
    for i in 0..count {
        let subview: *mut AnyObject = msg_send![&*subviews, objectAtIndex: i];
        let id: *mut AnyObject = msg_send![&*subview, identifier];
        if !id.is_null() {
            let same: ObjcBool = msg_send![&*id, isEqualToString: &*identifier];
            if same.as_bool() {
                return false;
            }
        }
    }
    let Some(class) = AnyClass::get(c"NSView") else { return false };
    let new: *mut AnyObject = msg_send![class, alloc];
    let new: *mut AnyObject = msg_send![new, initWithFrame: NSRect::ZERO];
    let _: () = msg_send![&*new, setIdentifier: &*identifier];
    let _: () = msg_send![&*host, addSubview: new];
    true
}

/// The visible window of the app that was not marked yet (a window opened since): the windows of a
/// viewport of egui have no handle in their update function. Returns it marked
#[cfg(target_os = "macos")]
pub fn claim_new_window() -> Option<NativeWindow> {
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject, Bool as ObjcBool};
    unsafe {
        let app: *mut AnyObject = msg_send![AnyClass::get(c"NSApplication")?, sharedApplication];
        let windows: *mut AnyObject = msg_send![&*app, windows];
        let count: usize = msg_send![&*windows, count];
        for i in 0..count {
            let window: *mut AnyObject = msg_send![&*windows, objectAtIndex: i];
            let visible: ObjcBool = msg_send![&*window, isVisible];
            let main: ObjcBool = msg_send![&*window, canBecomeMainWindow];
            if !visible.as_bool() || !main.as_bool() {
                continue;
            }
            let view: *mut AnyObject = msg_send![&*window, contentView];
            if view.is_null() {
                continue;
            }
            if mark_view(view) {
                let ptr = std::ptr::NonNull::new(view.cast())?;
                return Some(NativeWindow(raw_window_handle::AppKitWindowHandle::new(ptr)));
            }
        }
    }
    None
}

/// The radius of the corners of the window (16 points for a window with a title bar on macOS 26),
/// read from the private _cornerRadius of NSWindow when it has it, else 0
#[cfg(target_os = "macos")]
pub fn corner_radius(window: &impl raw_window_handle::HasWindowHandle) -> f64 {
    use objc2::runtime::{AnyObject, Bool as ObjcBool, Sel};
    use objc2::{msg_send, sel};

    let Ok(handle) = window.window_handle() else { return 0.0 };
    let raw_window_handle::RawWindowHandle::AppKit(handle) = handle.as_raw() else { return 0.0 };
    unsafe {
        let view = handle.ns_view.as_ptr().cast::<AnyObject>();
        let ns_window: *mut AnyObject = msg_send![&*view, window];
        if ns_window.is_null() {
            return 0.0;
        }
        let selector: Sel = sel!(_cornerRadius);
        let responds: ObjcBool = msg_send![&*ns_window, respondsToSelector: selector];
        if !responds.as_bool() {
            return 0.0;
        }
        let radius: f64 = msg_send![&*ns_window, _cornerRadius];
        radius.max(0.0)
    }
}

/// Blurs what is behind the window with a CABackdropLayer (the layer of the system glass views)
/// in a view put behind the content, rounded like the window: the blur of the window server
/// (CGSSetWindowBackgroundBlurRadius) covers the whole rectangle of the window and draws a square
/// frame around its rounded corners. Radius 0 removes the view. The classes are looked up by name
/// at run time: on a system without them, an error is returned and nothing changes
#[cfg(target_os = "macos")]
pub fn set_window_blur(window: &impl raw_window_handle::HasWindowHandle, radius: u32) -> Result<(), String> {
    set_window_filter(window, radius, 1.0, 1.0)
}

/// The blur of `set_window_blur` with the contrast and the saturation of what is seen through the glass
/// (1.0 changes nothing). The view is removed when all three change nothing
#[cfg(target_os = "macos")]
pub fn set_window_filter(
    window: &impl raw_window_handle::HasWindowHandle,
    radius: u32,
    contrast: f32,
    saturation: f32,
) -> Result<(), String> {
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject, Bool as ObjcBool};
    use objc2_foundation::{NSRect, NSString};

    let class = |name: &std::ffi::CStr| {
        AnyClass::get(name).ok_or_else(|| format!("no class {}", name.to_string_lossy()))
    };
    let raw_window_handle::RawWindowHandle::AppKit(handle) =
        window.window_handle().map_err(|e| e.to_string())?.as_raw()
    else {
        return Err("not an AppKit window".to_string());
    };
    unsafe {
        let view = handle.ns_view.as_ptr().cast::<AnyObject>();
        // the frame view of the window, where the backdrops are put behind the view of egui
        let host: *mut AnyObject = msg_send![&*view, superview];
        if host.is_null() {
            return Err("the window has no frame view".to_string());
        }
        let identifier = NSString::from_str("pyegui.backdrop-blur");

        // the view of a previous call
        let subviews: *mut AnyObject = msg_send![&*host, subviews];
        let count: usize = msg_send![&*subviews, count];
        let mut blur_view: *mut AnyObject = std::ptr::null_mut();
        for i in 0..count {
            let subview: *mut AnyObject = msg_send![&*subviews, objectAtIndex: i];
            let id: *mut AnyObject = msg_send![&*subview, identifier];
            if !id.is_null() {
                let same: ObjcBool = msg_send![&*id, isEqualToString: &*identifier];
                if same.as_bool() {
                    blur_view = subview;
                }
            }
        }
        let neutral = (contrast - 1.0).abs() < 0.005 && (saturation - 1.0).abs() < 0.005;
        if radius == 0 && neutral {
            if !blur_view.is_null() {
                let _: () = msg_send![&*blur_view, removeFromSuperview];
            }
            return Ok(());
        }

        if blur_view.is_null() {
            let bounds: NSRect = msg_send![&*host, bounds];
            let new: *mut AnyObject = msg_send![class(c"NSView")?, alloc];
            let new: *mut AnyObject = msg_send![new, initWithFrame: bounds];
            let _: () = msg_send![&*new, setIdentifier: &*identifier];
            // the view follows the size of the window (NSViewWidthSizable | NSViewHeightSizable)
            let _: () = msg_send![&*new, setAutoresizingMask: 18usize];
            // a layer-hosting view: its layer is the backdrop, set before wantsLayer
            let layer: *mut AnyObject = msg_send![class(c"CABackdropLayer")?, layer];
            let _: () = msg_send![&*layer, setWindowServerAware: true];
            let _: () = msg_send![&*layer, setCornerRadius: corner_radius(window)];
            let _: () = msg_send![&*layer, setMasksToBounds: true];
            let _: () = msg_send![&*new, setLayer: layer];
            let _: () = msg_send![&*new, setWantsLayer: true];
            // below every other view of the frame: under the glass and the content
            let below: isize = -1; // NSWindowBelow
            let nothing: *mut AnyObject = std::ptr::null_mut();
            let _: () = msg_send![&*host, addSubview: new, positioned: below, relativeTo: nothing];
            blur_view = new;
        }

        let layer: *mut AnyObject = msg_send![&*blur_view, layer];
        let filters: *mut AnyObject = msg_send![class(c"NSMutableArray")?, array];
        if radius > 0 {
            let filter: *mut AnyObject =
                msg_send![class(c"CAFilter")?, filterWithType: &*NSString::from_str("gaussianBlur")];
            if filter.is_null() {
                return Err("no gaussianBlur filter".to_string());
            }
            let value: *mut AnyObject = msg_send![class(c"NSNumber")?, numberWithDouble: radius as f64];
            let _: () = msg_send![&*filter, setValue: value, forKey: &*NSString::from_str("inputRadius")];
            // the edges of the window are not darkened by the blur
            let yes: *mut AnyObject = msg_send![class(c"NSNumber")?, numberWithBool: true];
            let _: () = msg_send![&*filter, setValue: yes, forKey: &*NSString::from_str("inputNormalizeEdges")];
            let _: () = msg_send![&*filters, addObject: filter];
        }
        if (saturation - 1.0).abs() >= 0.005 {
            let filter: *mut AnyObject =
                msg_send![class(c"CAFilter")?, filterWithType: &*NSString::from_str("colorSaturate")];
            if !filter.is_null() {
                let value: *mut AnyObject = msg_send![class(c"NSNumber")?, numberWithDouble: saturation as f64];
                let _: () = msg_send![&*filter, setValue: value, forKey: &*NSString::from_str("inputAmount")];
                let _: () = msg_send![&*filters, addObject: filter];
            }
        }
        if (contrast - 1.0).abs() >= 0.005 {
            let filter: *mut AnyObject =
                msg_send![class(c"CAFilter")?, filterWithType: &*NSString::from_str("colorMatrix")];
            if !filter.is_null() {
                // the contrast scales the colors around the middle gray: a CAColorMatrix (4 rows of
                // r, g, b, a and a bias)
                let bias = 0.5 * (1.0 - contrast);
                let matrix: [f32; 20] = [
                    contrast, 0.0, 0.0, 0.0, bias, //
                    0.0, contrast, 0.0, 0.0, bias, //
                    0.0, 0.0, contrast, 0.0, bias, //
                    0.0, 0.0, 0.0, 1.0, 0.0,
                ];
                let value: *mut AnyObject = msg_send![
                    class(c"NSValue")?,
                    valueWithBytes: matrix.as_ptr().cast::<std::ffi::c_void>(),
                    objCType: c"{CAColorMatrix=ffffffffffffffffffff}".as_ptr()
                ];
                let _: () = msg_send![&*filter, setValue: value, forKey: &*NSString::from_str("inputColorMatrix")];
                let _: () = msg_send![&*filters, addObject: filter];
            }
        }
        let _: () = msg_send![&*layer, setFilters: filters];
    }
    Ok(())
}

/// Sets the dark mode of the window, which changes the tint of the system backdrops
#[cfg(target_os = "windows")]
pub fn set_dark_mode(window: &impl raw_window_handle::HasWindowHandle, dark: bool) -> Result<(), String> {
    use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};

    let raw_window_handle::RawWindowHandle::Win32(handle) =
        window.window_handle().map_err(|e| e.to_string())?.as_raw()
    else {
        return Err("not a win32 window".to_string());
    };
    let value: u32 = dark.into();

    let result = unsafe {
        DwmSetWindowAttribute(
            handle.hwnd.get() as _,
            DWMWA_USE_IMMERSIVE_DARK_MODE as _,
            (&raw const value).cast(),
            size_of::<u32>() as _,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(format!("DwmSetWindowAttribute failed with {:#x}", result))
    }
}

#[cfg(not(target_os = "windows"))]
pub fn set_dark_mode(_window: &impl raw_window_handle::HasWindowHandle, _dark: bool) -> Result<(), String> {
    Err("not supported on this platform".to_string())
}

/// Windows 11: asks for the rounded corners of the system for a window without a title bar of the
/// system (the ones of a window with a frame), and a shadow around it. A maximized window is square.
#[cfg(target_os = "windows")]
pub fn set_rounded_corners(window: &impl raw_window_handle::HasWindowHandle) -> Result<(), String> {
    use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND};

    let raw_window_handle::RawWindowHandle::Win32(handle) =
        window.window_handle().map_err(|e| e.to_string())?.as_raw()
    else {
        return Err("not a win32 window".to_string());
    };
    let value: i32 = DWMWCP_ROUND;
    let result = unsafe {
        DwmSetWindowAttribute(
            handle.hwnd.get() as _,
            DWMWA_WINDOW_CORNER_PREFERENCE as _,
            (&raw const value).cast(),
            size_of::<i32>() as _,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(format!("DwmSetWindowAttribute failed with {:#x}", result))
    }
}

#[cfg(not(target_os = "windows"))]
pub fn set_rounded_corners(_window: &impl raw_window_handle::HasWindowHandle) -> Result<(), String> {
    Err("not supported on this platform".to_string())
}


/// Puts the three buttons of the window (close, minimize, zoom) at a place of the title bar:
/// the centre of the close button is `left` points from the left edge of the window and `top`
/// points from its top, the next ones follow every `spacing` points. Without a toolbar the system
/// puts them higher and closer to the edge than a window with a unified toolbar (Claude,
/// Finder); the title bar view is moved when AppKit lays it out again, so call it at every frame
/// (it does nothing when they are already there).
#[cfg(target_os = "macos")]
pub fn place_window_buttons(
    window: &impl raw_window_handle::HasWindowHandle,
    left: f64,
    top: f64,
    spacing: f64,
) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use objc2_foundation::{NSPoint, NSRect};

    let Ok(handle) = window.window_handle() else { return };
    let raw_window_handle::RawWindowHandle::AppKit(handle) = handle.as_raw() else { return };
    unsafe {
        let view = handle.ns_view.as_ptr().cast::<AnyObject>();
        let ns_window: *mut AnyObject = msg_send![&*view, window];
        if ns_window.is_null() {
            return;
        }
        // NSWindowCloseButton, NSWindowMiniaturizeButton, NSWindowZoomButton
        for (index, kind) in [0isize, 1, 2].into_iter().enumerate() {
            let button: *mut AnyObject = msg_send![&*ns_window, standardWindowButton: kind];
            if button.is_null() {
                continue;
            }
            let parent: *mut AnyObject = msg_send![&*button, superview];
            if parent.is_null() {
                continue;
            }
            let frame: NSRect = msg_send![&*button, frame];
            let bounds: NSRect = msg_send![&*parent, frame];
            let x = left + spacing * index as f64 - frame.size.width / 2.0;
            // the title bar view is at the top of the window, its origin is at the bottom
            let y = bounds.size.height - top - frame.size.height / 2.0;
            if (frame.origin.x - x).abs() > 0.1 || (frame.origin.y - y).abs() > 0.1 {
                let _: () = msg_send![&*button, setFrameOrigin: NSPoint { x, y }];
            }
        }
    }
}
