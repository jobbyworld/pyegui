//! Drag and drop with the file manager of the system: files dropped on the window from the Finder
//! (the Explorer), and files dragged out of the window to the Finder (the Explorer, the desktop,
//! another application).

use std::path::PathBuf;

use pyo3::prelude::*;

use crate::{panel_parent_ui, UI};

/// Starts a drag of files out of the window, to the Finder or to any application that takes files.
/// Call it on the frame a drag starts on an item (``response.drag_started`` of an
/// ``interactive`` area, or a drag of the file views with ``drag_out=True``): the mouse button has
/// to be down. The drag goes on by itself, with the icons of the files under the pointer, and the
/// drop is done by the receiver (a copy in the Finder, a move with Cmd held). The window gets no
/// pointer event until the mouse button is released. Works on macOS, returns False elsewhere or if
/// there is nothing to drag.
///
/// Args:
///     paths (list[str]): the files and folders to drag
///
/// Returns:
///     bool: True if the drag started
///
/// Example::
///
///     r = interactive("row", lambda: label(name))
///     if r.drag_started:
///         start_file_drag([path])
#[pyfunction]
fn start_file_drag(paths: Vec<String>) -> bool {
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).filter(|p| p.exists()).collect();
    if paths.is_empty() {
        return false;
    }
    start_drag(&paths)
}

/// The modifier keys held now, read from the system. Unlike ``modifiers`` it is right while files
/// are dragged, when the window gets no keyboard event: it is what decides if a drop copies or
/// moves. ``command`` is the Command key on macOS and Ctrl elsewhere; ``alt`` is Option on macOS.
///
/// Returns:
///     dict: the keys shift, ctrl, alt and command, each a bool
///
/// Example::
///
///     if dropped_files():
///         copy = system_modifiers()["alt"]  # Option copies on a same disk, on macOS
#[pyfunction]
unsafe fn system_modifiers(py: Python<'_>) -> PyResult<Bound<'_, pyo3::types::PyDict>> {
    let dict = pyo3::types::PyDict::new(py);
    let (shift, ctrl, alt, command) = match held_modifiers() {
        Some(held) => held,
        None => {
            let held = panel_parent_ui(&UI)?.ctx().input(|i| i.modifiers);
            (held.shift, held.ctrl, held.alt, held.command)
        }
    };
    dict.set_item("shift", shift)?;
    dict.set_item("ctrl", ctrl)?;
    dict.set_item("alt", alt)?;
    dict.set_item("command", command)?;
    Ok(dict)
}

#[cfg(target_os = "macos")]
fn held_modifiers() -> Option<(bool, bool, bool, bool)> {
    use objc2_app_kit::{NSEvent, NSEventModifierFlags as Flags};
    let flags = NSEvent::modifierFlags_class();
    Some((
        flags.contains(Flags::Shift),
        flags.contains(Flags::Control),
        flags.contains(Flags::Option),
        flags.contains(Flags::Command),
    ))
}

#[cfg(not(target_os = "macos"))]
fn held_modifiers() -> Option<(bool, bool, bool, bool)> {
    None
}

thread_local! {
    // the files of the last drag started by this window
    static LAST_DRAG: std::cell::RefCell<Vec<PathBuf>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The files of the last drag started by this window (``start_file_drag``, the drag of the file
/// views). When ``dropped_files`` gives these same files, the drop comes from the window itself:
/// the app moves them instead of copying them, as the Finder does inside a disk.
///
/// Returns:
///     list[str]: the full paths, empty before a first drag
///
/// Example::
///
///     dropped = dropped_files()
///     if dropped and sorted(dropped) == sorted(dragged_files()):
///         move(dropped, folder)
#[pyfunction]
fn dragged_files() -> Vec<String> {
    LAST_DRAG.with(|l| l.borrow().iter().map(|p| p.to_string_lossy().into_owned()).collect())
}

/// The files dropped on the window this frame, from the Finder (the Explorer or another
/// application). Empty most of the time. The drop is a copy request: it is up to the app to copy
/// or move them, see ``hovered_files`` to show where they would go.
///
/// Returns:
///     list[str]: the full paths
///
/// Example::
///
///     for path in dropped_files():
///         shutil.copy(path, current_folder)
#[pyfunction]
unsafe fn dropped_files() -> PyResult<Vec<String>> {
    let ctx = panel_parent_ui(&UI)?.ctx().clone();
    Ok(ctx.input(|i| {
        i.raw
            .dropped_files
            .iter()
            .map(|f| f.path().to_string_lossy().into_owned())
            .collect()
    }))
}

/// The files being dragged over the window, from the Finder (the Explorer or another application),
/// not dropped yet. Empty when nothing is dragged over the window. It is the moment to show a
/// frame or a message "Drop here".
///
/// Returns:
///     list[str]: the full paths
///
/// Example::
///
///     if hovered_files():
///         draw_rect(rect, stroke=(accent, 2))
#[pyfunction]
unsafe fn hovered_files() -> PyResult<Vec<String>> {
    let ctx = panel_parent_ui(&UI)?.ctx().clone();
    Ok(ctx.input(|i| {
        i.raw
            .hovered_files
            .iter()
            .filter_map(|f| f.path.as_ref())
            .map(|p| p.to_string_lossy().into_owned())
            .collect()
    }))
}

#[cfg(target_os = "macos")]
mod mac {
    use std::path::PathBuf;

    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2::{define_class, msg_send, AnyThread, MainThreadMarker, MainThreadOnly};
    use objc2_app_kit::{
        NSApplication, NSDragOperation, NSEvent, NSEventType, NSDraggingContext, NSDraggingItem, NSDraggingSession, NSDraggingSource,
        NSPasteboardWriting, NSWorkspace,
    };
    use objc2_foundation::{NSArray, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString, NSURL};

    define_class!(
        /// The source of the drag: copy, move and link are allowed outside the window, nothing inside
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "PyeguiDragSource"]
        struct DragSource;

        unsafe impl NSObjectProtocol for DragSource {}

        unsafe impl NSDraggingSource for DragSource {
            #[unsafe(method(draggingSession:sourceOperationMaskForDraggingContext:))]
            fn source_operation_mask(&self, _session: &NSDraggingSession, context: NSDraggingContext) -> NSDragOperation {
                if context == NSDraggingContext::OutsideApplication {
                    NSDragOperation::Copy | NSDragOperation::Move | NSDragOperation::Link | NSDragOperation::Generic
                } else {
                    // on the window itself: dropped files are told by dropped_files like the ones of the Finder
                    NSDragOperation::Move | NSDragOperation::Copy | NSDragOperation::Generic
                }
            }
        }
    );

    thread_local! {
        // the session does not keep its source alive
        static SOURCE: std::cell::RefCell<Option<Retained<DragSource>>> = const { std::cell::RefCell::new(None) };
    }

    /// The pointer in the points of the window (origin at the top left), read from the system: it
    /// is right while files are dragged over the window, when the window gets no pointer event
    pub fn pointer() -> Option<(f32, f32)> {
        let mtm = MainThreadMarker::new()?;
        let app = NSApplication::sharedApplication(mtm);
        let window = app.keyWindow().or_else(|| app.mainWindow())?;
        let view = window.contentView()?;
        let at = view.convertPoint_fromView(window.convertPointFromScreen(NSEvent::mouseLocation()), None);
        let size = view.bounds().size;
        // a flipped view has its origin at the top left already
        let y = if view.isFlipped() { at.y } else { size.height - at.y };
        Some((at.x as f32, y as f32))
    }

    pub fn start(paths: &[PathBuf]) -> bool {
        let Some(mtm) = MainThreadMarker::new() else { return false };
        let app = NSApplication::sharedApplication(mtm);
        let Some(window) = app.keyWindow().or_else(|| app.mainWindow()) else { return false };
        let Some(view) = window.contentView() else { return false };
        // the frame is drawn from a redraw, not from the mouse event: the session needs a mouse
        // event, so one is made at the pointer
        let screen = NSEvent::mouseLocation();
        let in_window = window.convertPointFromScreen(screen);
        let Some(event) = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
            NSEventType::LeftMouseDragged,
            in_window,
            NSEvent::modifierFlags_class(),
            0.0,
            window.windowNumber(),
            None,
            0,
            1,
            1.0,
        ) else {
            return false;
        };

        let source = SOURCE.with(|s| {
            s.borrow_mut()
                .get_or_insert_with(|| unsafe { msg_send![DragSource::alloc(mtm), init] })
                .clone()
        });

        // the icons are drawn around the pointer, a little shifted one from the other
        let at = view.convertPoint_fromView(in_window, None);
        let workspace = NSWorkspace::sharedWorkspace();
        let mut items = Vec::new();
        for (n, path) in paths.iter().enumerate() {
            let Some(path) = path.to_str() else { continue };
            let url = NSURL::fileURLWithPath(&NSString::from_str(path));
            let writer: &ProtocolObject<dyn NSPasteboardWriting> = ProtocolObject::from_ref(&*url);
            let item = NSDraggingItem::initWithPasteboardWriter(NSDraggingItem::alloc(), writer);
            let icon = workspace.iconForFile(&NSString::from_str(path));
            let shift = 6.0 * n.min(5) as f64;
            let frame = NSRect::new(NSPoint::new(at.x - 24.0 + shift, at.y - 24.0 - shift), NSSize::new(48.0, 48.0));
            unsafe { item.setDraggingFrame_contents(frame, Some(&icon)) };
            items.push(item);
        }
        if items.is_empty() {
            return false;
        }
        let items = NSArray::from_retained_slice(&items);
        let source: &ProtocolObject<dyn NSDraggingSource> = ProtocolObject::from_ref(&*source);
        // the binding takes a nil for a bug: ask for an optional session
        let session: Option<Retained<NSDraggingSession>> =
            unsafe { msg_send![&*view, beginDraggingSessionWithItems: &*items, event: &*event, source: source] };
        session.is_some()
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn start_drag(paths: &[PathBuf]) -> bool {
    let started = mac::start(paths);
    if started {
        LAST_DRAG.with(|l| *l.borrow_mut() = paths.to_vec());
    }
    started
}

/// The pointer in the points of the window, from the system (macOS), None elsewhere
#[cfg(target_os = "macos")]
pub(crate) fn native_pointer() -> Option<eframe::egui::Pos2> {
    mac::pointer().map(|(x, y)| eframe::egui::pos2(x, y))
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn native_pointer() -> Option<eframe::egui::Pos2> {
    None
}

#[cfg(not(target_os = "macos"))]
fn start_drag(_paths: &[PathBuf]) -> bool {
    false
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(start_file_drag, m)?)?;
    m.add_function(wrap_pyfunction!(dropped_files, m)?)?;
    m.add_function(wrap_pyfunction!(dragged_files, m)?)?;
    m.add_function(wrap_pyfunction!(system_modifiers, m)?)?;
    m.add_function(wrap_pyfunction!(hovered_files, m)?)?;
    Ok(())
}
