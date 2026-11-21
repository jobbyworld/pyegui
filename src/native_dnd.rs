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
/// pointer event until the mouse button is released. Works on macOS and on Windows (on Windows the
/// call returns once the drag is over), returns False elsewhere or if there is nothing to drag.
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
pub(crate) fn held_modifiers() -> Option<(bool, bool, bool, bool)> {
    use objc2_app_kit::{NSEvent, NSEventModifierFlags as Flags};
    let flags = NSEvent::modifierFlags_class();
    Some((
        flags.contains(Flags::Shift),
        flags.contains(Flags::Control),
        flags.contains(Flags::Option),
        flags.contains(Flags::Command),
    ))
}

/// Windows: the keys held now, read from the system (the window gets no key event during a drag).
/// There is no Command key, it is Ctrl.
#[cfg(target_os = "windows")]
pub(crate) fn held_modifiers() -> Option<(bool, bool, bool, bool)> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_MENU, VK_SHIFT};
    let down = |key: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY| unsafe { GetAsyncKeyState(key.0 as i32) } < 0;
    let ctrl = down(VK_CONTROL);
    Some((down(VK_SHIFT), ctrl, down(VK_MENU), ctrl))
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) fn held_modifiers() -> Option<(bool, bool, bool, bool)> {
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

#[cfg(target_os = "windows")]
mod win {
    use std::path::PathBuf;

    use windows::core::{implement, BOOL, HRESULT};
    use windows::Win32::Foundation::{
        DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, HWND, LPARAM, POINT, S_OK, WPARAM,
    };
    use windows::Win32::Graphics::Gdi::ScreenToClient;
    use windows::Win32::System::Com::{CoTaskMemFree, IBindCtx, IDataObject};
    use windows::Win32::System::Ole::{
        DoDragDrop, IDropSource, IDropSource_Impl, OleInitialize, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_LINK, DROPEFFECT_MOVE,
        DROPEFFECT_NONE,
    };
    use windows::Win32::System::SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS};
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::Shell::Common::ITEMIDLIST;
    use windows::Win32::UI::Shell::{BHID_DataObject, SHCreateShellItemArrayFromIDLists, SHParseDisplayName};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumThreadWindows, GetCursorPos, GetWindow, IsWindowVisible, PostMessageW, GW_OWNER, WM_LBUTTONUP,
    };

    /// The source of the drag: it ends when the left button is released (the drop) or with Escape
    #[implement(IDropSource)]
    struct DragSource;

    impl IDropSource_Impl for DragSource_Impl {
        fn QueryContinueDrag(&self, escape_pressed: BOOL, key_state: MODIFIERKEYS_FLAGS) -> HRESULT {
            if escape_pressed.as_bool() {
                DRAGDROP_S_CANCEL
            } else if key_state.0 & MK_LBUTTON.0 == 0 {
                DRAGDROP_S_DROP
            } else {
                S_OK
            }
        }

        fn GiveFeedback(&self, _effect: DROPEFFECT) -> HRESULT {
            DRAGDROP_S_USEDEFAULTCURSORS
        }
    }

    /// The window handle given by the frame of eframe (0 until known)
    pub static WINDOW: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

    /// The window of the application: the visible top level window of the GUI thread. It is found
    /// from the thread, not from the active window, which is not ours while files are dragged over it
    fn window() -> Option<HWND> {
        let known = WINDOW.load(std::sync::atomic::Ordering::Relaxed);
        if known != 0 {
            return Some(HWND(known as *mut _));
        }
        unsafe extern "system" fn each(hwnd: HWND, found: LPARAM) -> BOOL {
            unsafe {
                if IsWindowVisible(hwnd).as_bool() && GetWindow(hwnd, GW_OWNER).is_err() {
                    *(found.0 as *mut Option<HWND>) = Some(hwnd);
                    return BOOL(0);
                }
            }
            BOOL(1)
        }
        let mut found: Option<HWND> = None;
        unsafe {
            let _ = EnumThreadWindows(GetCurrentThreadId(), Some(each), LPARAM(&mut found as *mut _ as isize));
        }
        found
    }

    /// The pointer in the points of the window (origin at the top left), read from the system: it is
    /// right while files are dragged over the window, when the window gets no pointer event
    pub fn pointer(pixels_per_point: f32) -> Option<(f32, f32)> {
        unsafe {
            let hwnd = window()?;
            let mut at = POINT::default();
            GetCursorPos(&mut at).ok()?;
            if !ScreenToClient(hwnd, &mut at).as_bool() {
                return None;
            }
            Some((at.x as f32 / pixels_per_point, at.y as f32 / pixels_per_point))
        }
    }

    /// The data of the drag: the files as the shell gives them (CF_HDROP and the shell id list), what
    /// the Explorer, the desktop and the other applications read
    fn data_object(paths: &[PathBuf]) -> Option<IDataObject> {
        use std::os::windows::ffi::OsStrExt;
        unsafe {
            let mut ids: Vec<*mut ITEMIDLIST> = Vec::new();
            for path in paths {
                // the shell does not take the extended prefix
                let text = path.to_string_lossy().replace('/', "\\");
                let text = text.strip_prefix(r"\\?\").unwrap_or(&text).to_string();
                let wide: Vec<u16> = std::ffi::OsString::from(text).encode_wide().chain(Some(0)).collect();
                let mut id: *mut ITEMIDLIST = std::ptr::null_mut();
                if SHParseDisplayName(windows::core::PCWSTR(wide.as_ptr()), None::<&IBindCtx>, &mut id, 0, None).is_ok() && !id.is_null() {
                    ids.push(id);
                }
            }
            let list: Vec<*const ITEMIDLIST> = ids.iter().map(|i| *i as *const ITEMIDLIST).collect();
            let object = if list.is_empty() {
                None
            } else {
                // a shell item array binds to the data object of the Explorer, for items of any folders
                SHCreateShellItemArrayFromIDLists(&list)
                    .and_then(|array| array.BindToHandler::<_, IDataObject>(None::<&IBindCtx>, &BHID_DataObject))
                    .ok()
            };
            for id in ids {
                CoTaskMemFree(Some(id as *const _));
            }
            object
        }
    }

    /// The drop target of the window. It replaces the one of winit, which only knows the files of
    /// the disk (CF_HDROP): this one also reads the virtual files of the applications that have no
    /// file yet, like the attachments dragged out of Outlook (a file group descriptor and the content
    /// of each file in a stream). Those are written in a temporary folder and given as paths.
    pub mod drop_in {
        use std::path::{Path, PathBuf};
        use std::sync::Mutex;

        use eframe::egui;
        use windows::core::{implement, Interface, Ref, Result as WinResult};
        use windows::Win32::Foundation::{HGLOBAL, HWND, POINTL};
        use windows::Win32::System::Com::{IBindCtx,
            IDataObject, DVASPECT_CONTENT, FORMATETC, STGMEDIUM, TYMED_HGLOBAL, TYMED_ISTREAM,
        };
        use windows::Win32::System::DataExchange::RegisterClipboardFormatW;
        use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
        use windows::Win32::System::Ole::{
            IDropTarget, IDropTarget_Impl, RegisterDragDrop, ReleaseStgMedium, RevokeDragDrop, CF_HDROP, DROPEFFECT, DROPEFFECT_COPY,
            DROPEFFECT_LINK, DROPEFFECT_MOVE, DROPEFFECT_NONE,
        };
        use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
        use windows::Win32::UI::Shell::{IDataObjectAsyncCapability, DragQueryFileW, CFSTR_FILECONTENTS, CFSTR_FILEDESCRIPTORW, FILEGROUPDESCRIPTORW, HDROP};

        #[derive(Default)]
        struct Pending {
            hovered: Vec<PathBuf>,
            dropped: Vec<PathBuf>,
            changed: bool,
        }

        static PENDING: Mutex<Pending> = Mutex::new(Pending { hovered: Vec::new(), dropped: Vec::new(), changed: false });
        static CTX: Mutex<Option<egui::Context>> = Mutex::new(None);
        static INSTALLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

        /// Diagnostic, only when PYEGUI_DROP_LOG is set: appends a line to pyegui-drop.log in the
        /// temporary folder
        fn log(text: &str) {
            use std::io::Write;
            if std::env::var_os("PYEGUI_DROP_LOG").is_none() {
                return;
            }
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(std::env::temp_dir().join("pyegui-drop.log")) {
                let _ = writeln!(f, "{text}");
            }
        }

        /// Diagnostic: the formats a drag offers
        fn log_formats(data: &IDataObject) {
            use windows::Win32::System::Com::DATADIR_GET;
            use windows::Win32::System::DataExchange::GetClipboardFormatNameW;
            unsafe {
                let Ok(list) = data.EnumFormatEtc(DATADIR_GET.0 as u32) else {
                    log("EnumFormatEtc failed");
                    return;
                };
                loop {
                    let mut one = [FORMATETC::default()];
                    let mut got = 0u32;
                    if list.Next(&mut one, Some(&mut got)).0 != 0 || got == 0 {
                        break;
                    }
                    let mut name = [0u16; 128];
                    let n = GetClipboardFormatNameW(one[0].cfFormat as u32, &mut name);
                    let name = String::from_utf16_lossy(&name[..n.max(0) as usize]);
                    log(&format!("  format {} '{}' tymed {} lindex {}", one[0].cfFormat, name, one[0].tymed, one[0].lindex));
                }
            }
        }

        static OURS: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

        fn wake() {
            if let Some(ctx) = CTX.lock().ok().and_then(|c| c.clone()) {
                ctx.request_repaint();
            }
        }

        fn format(name: windows::core::PCWSTR) -> u16 {
            unsafe { RegisterClipboardFormatW(name) as u16 }
        }

        fn medium(data: &IDataObject, format: u16, index: i32, tymed: u32) -> Option<STGMEDIUM> {
            let wanted = FORMATETC { cfFormat: format, ptd: std::ptr::null_mut(), dwAspect: DVASPECT_CONTENT.0 as u32, lindex: index, tymed };
            let got = unsafe { data.GetData(&wanted) };
            if let Err(e) = &got {
                log(&format!("  GetData format {format} index {index} failed: {e:?}"));
            }
            got.ok()
        }

        /// The files of the disk of a drag (CF_HDROP)
        fn disk_files(data: &IDataObject) -> Vec<PathBuf> {
            let mut files = Vec::new();
            let Some(mut m) = medium(data, CF_HDROP.0, -1, TYMED_HGLOBAL.0 as u32) else { return files };
            unsafe {
                let hdrop = HDROP(m.u.hGlobal.0);
                let count = DragQueryFileW(hdrop, u32::MAX, None);
                log(&format!("  CF_HDROP gives {count} files, tymed {}", m.tymed));
                for i in 0..count {
                    let len = DragQueryFileW(hdrop, i, None) as usize;
                    let mut buffer = vec![0u16; len + 1];
                    let n = DragQueryFileW(hdrop, i, Some(&mut buffer)) as usize;
                    files.push(PathBuf::from(String::from_utf16_lossy(&buffer[..n])));
                }
                ReleaseStgMedium(&mut m);
            }
            files
        }

        /// True if the drag offers files, even if their content is not ready: Chromium (the new Outlook,
        /// the browsers) lists CF_HDROP but only fills it when the drop is done
        fn offers_files(data: &IDataObject) -> bool {
            let has = |format: u16| {
                let wanted = FORMATETC { cfFormat: format, ptd: std::ptr::null_mut(), dwAspect: DVASPECT_CONTENT.0 as u32, lindex: -1, tymed: TYMED_HGLOBAL.0 as u32 };
                unsafe { data.QueryGetData(&wanted).0 == 0 }
            };
            has(CF_HDROP.0) || has(format(CFSTR_FILEDESCRIPTORW))
        }

        /// The names of the virtual files of a drag (the file group descriptor)
        fn virtual_names(data: &IDataObject) -> Vec<String> {
            let mut names = Vec::new();
            let Some(mut m) = medium(data, format(CFSTR_FILEDESCRIPTORW), -1, TYMED_HGLOBAL.0 as u32) else { return names };
            unsafe {
                let global: HGLOBAL = m.u.hGlobal;
                let group = GlobalLock(global) as *const FILEGROUPDESCRIPTORW;
                if !group.is_null() {
                    let count = (*group).cItems as usize;
                    let first = std::ptr::addr_of!((*group).fgd) as *const windows::Win32::UI::Shell::FILEDESCRIPTORW;
                    for i in 0..count {
                        let item = std::ptr::read_unaligned(first.add(i));
                        let name = item.cFileName;
                        let len = name.iter().position(|c| *c == 0).unwrap_or(name.len());
                        names.push(String::from_utf16_lossy(&name[..len]));
                    }
                    let _ = GlobalUnlock(global);
                }
                ReleaseStgMedium(&mut m);
            }
            names
        }

        /// The content of the virtual file number `index`
        fn virtual_content(data: &IDataObject, index: i32) -> Option<Vec<u8>> {
            let mut m = medium(data, format(CFSTR_FILECONTENTS), index, (TYMED_ISTREAM.0 | TYMED_HGLOBAL.0) as u32)?;
            let mut bytes = Vec::new();
            unsafe {
                if m.tymed == TYMED_ISTREAM.0 as u32 {
                    if let Some(stream) = m.u.pstm.as_ref() {
                        let mut buffer = vec![0u8; 1 << 16];
                        loop {
                            let mut read = 0u32;
                            let result = stream.Read(buffer.as_mut_ptr().cast(), buffer.len() as u32, Some(&mut read));
                            if result.is_err() || read == 0 {
                                break;
                            }
                            bytes.extend_from_slice(&buffer[..read as usize]);
                        }
                    }
                } else if m.tymed == TYMED_HGLOBAL.0 as u32 {
                    let global: HGLOBAL = m.u.hGlobal;
                    let size = GlobalSize(global);
                    let at = GlobalLock(global) as *const u8;
                    if !at.is_null() {
                        bytes.extend_from_slice(std::slice::from_raw_parts(at, size));
                        let _ = GlobalUnlock(global);
                    }
                }
                ReleaseStgMedium(&mut m);
            }
            Some(bytes)
        }

        /// A name the disk takes: no folder, no forbidden character
        fn safe_name(name: &str) -> String {
            let last = name.rsplit(['/', '\\']).next().unwrap_or(name);
            let clean: String = last.chars().map(|c| if r#"<>:"|?*"#.contains(c) || c.is_control() { '_' } else { c }).collect();
            if clean.trim().is_empty() { "file".into() } else { clean }
        }

        /// The virtual files written in a folder of the temporary directory, as paths
        fn extract(data: &IDataObject) -> Vec<PathBuf> {
            let names = virtual_names(data);
            if names.is_empty() {
                return vec![];
            }
            let folder: PathBuf = std::env::temp_dir()
                .join(format!("pyegui-drops-{}", std::process::id()))
                .join(COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed).to_string());
            if std::fs::create_dir_all(&folder).is_err() {
                return vec![];
            }
            let mut files = Vec::new();
            for (i, name) in names.iter().enumerate() {
                let Some(bytes) = virtual_content(data, i as i32) else { continue };
                let path: &Path = &folder.join(safe_name(name));
                if std::fs::write(path, bytes).is_ok() {
                    files.push(path.to_path_buf());
                }
            }
            files
        }

        /// Copy when the source allows it, else what it allows
        fn effect_for(allowed: DROPEFFECT) -> DROPEFFECT {
            for effect in [DROPEFFECT_COPY, DROPEFFECT_MOVE, DROPEFFECT_LINK] {
                if allowed.0 & effect.0 != 0 {
                    return effect;
                }
            }
            DROPEFFECT_NONE
        }

        /// A file given to egui
        #[derive(Debug)]
        struct Dropped(PathBuf);

        impl egui::DroppedFile for Dropped {
            fn path(&self) -> &Path {
                &self.0
            }

            fn bytes(&self) -> Result<Vec<u8>, String> {
                std::fs::read(&self.0).map_err(|e| e.to_string())
            }
        }

        #[implement(IDropTarget)]
        struct Target;

        impl IDropTarget_Impl for Target_Impl {
            fn DragEnter(&self, data: Ref<IDataObject>, _keys: MODIFIERKEYS_FLAGS, _at: &POINTL, effect: *mut DROPEFFECT) -> WinResult<()> {
                let mut shown = Vec::new();
                if let Some(data) = data.as_ref() {
                    log("DragEnter");
                    log_formats(data);
                    shown = disk_files(data);
                    if shown.is_empty() {
                        shown = virtual_names(data).into_iter().map(PathBuf::from).collect();
                    }
                    if shown.is_empty() && offers_files(data) {
                        shown = vec![PathBuf::from("file")]; // the files are known at the drop
                    }
                }
                let accepted = !shown.is_empty();
                if let Ok(mut p) = PENDING.lock() {
                    p.hovered = shown;
                    p.changed = true;
                }
                unsafe { *effect = if accepted { effect_for(*effect) } else { DROPEFFECT_NONE } };
                wake();
                Ok(())
            }

            fn DragOver(&self, _keys: MODIFIERKEYS_FLAGS, _at: &POINTL, effect: *mut DROPEFFECT) -> WinResult<()> {
                static ONCE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
                if !ONCE.swap(true, std::sync::atomic::Ordering::Relaxed) {
                    log("DragOver");
                }
                let accepted = PENDING.lock().map(|p| !p.hovered.is_empty()).unwrap_or(false);
                unsafe { *effect = if accepted { effect_for(*effect) } else { DROPEFFECT_NONE } };
                wake(); // the window gets no pointer event: the views read the pointer at each frame
                Ok(())
            }

            fn DragLeave(&self) -> WinResult<()> {
                if let Ok(mut p) = PENDING.lock() {
                    p.hovered.clear();
                    p.changed = true;
                }
                wake();
                Ok(())
            }

            fn Drop(&self, data: Ref<IDataObject>, _keys: MODIFIERKEYS_FLAGS, _at: &POINTL, effect: *mut DROPEFFECT) -> WinResult<()> {
                let mut files = Vec::new();
                if let Some(data) = data.as_ref() {
                    // Chromium (the new Outlook) prepares its files in the background: it wants the
                    // asynchronous protocol the Explorer uses
                    let asynchronous: Option<IDataObjectAsyncCapability> = data.cast().ok();
                    if let Some(a) = &asynchronous {
                        unsafe {
                            log(&format!("  async capability, mode {:?}", a.GetAsyncMode()));
                            let _ = a.SetAsyncMode(true);
                            let _ = a.StartOperation(None::<&IBindCtx>);
                        }
                    }
                    files = disk_files(data);
                    if files.is_empty() {
                        files = extract(data);
                    }
                    if let Some(a) = &asynchronous {
                        unsafe {
                            let result = if files.is_empty() { windows::Win32::Foundation::E_FAIL } else { windows::Win32::Foundation::S_OK };
                            let _ = a.EndOperation(result, None::<&IBindCtx>, DROPEFFECT_COPY.0);
                        }
                    }
                }
                let accepted = !files.is_empty();
                log(&format!("Drop: {} files {:?}", files.len(), files));
                if let Ok(mut p) = PENDING.lock() {
                    p.hovered.clear();
                    p.dropped = files;
                    p.changed = true;
                }
                unsafe { *effect = if accepted { effect_for(*effect) } else { DROPEFFECT_NONE } };
                wake();
                Ok(())
            }
        }

        fn install(hwnd: HWND) -> bool {
            unsafe {
                let revoked = RevokeDragDrop(hwnd); // the target of winit
                let target: IDropTarget = Target.into();
                let registered = RegisterDragDrop(hwnd, &target);
                OURS.store(windows::core::Interface::as_raw(&target) as isize, std::sync::atomic::Ordering::Relaxed);
                std::mem::forget(target); // the window keeps it
                log(&format!("revoke {:?} register {:?}", revoked, registered));
                registered.is_ok()
            }
        }

        /// Called with the input of each frame: installs the target the first time, and gives the files
        /// being dragged over the window and the files dropped to egui
        pub fn pump(ctx: &egui::Context, raw: &mut egui::RawInput) {
            if !INSTALLED.load(std::sync::atomic::Ordering::Relaxed) {
                // only the window given by eframe: a guess could be another window of the thread
                let known = super::WINDOW.load(std::sync::atomic::Ordering::Relaxed);
                if known != 0 {
                    let hwnd = HWND(known as *mut _);
                    let ok = install(hwnd);
                    log(&format!("install on {:?}: {}", hwnd.0, ok));
                    if ok {
                        INSTALLED.store(true, std::sync::atomic::Ordering::Relaxed);
                        if let Ok(mut c) = CTX.lock() {
                            *c = Some(ctx.clone());
                        }
                    }
                }
                return;
            }
            let Ok(mut p) = PENDING.lock() else { return };
            if !p.changed {
                return;
            }
            raw.hovered_files = p.hovered.iter().map(|path| egui::HoveredFile { path: Some(path.clone()), mime: String::new() }).collect();
            raw.dropped_files = std::mem::take(&mut p.dropped)
                .into_iter()
                .map(|path| std::sync::Arc::new(Dropped(path)) as egui::DroppedFileHandle)
                .collect();
            if p.hovered.is_empty() {
                p.changed = false;
            }
        }
    }

    /// Starts the drag and returns when it is over (the drop, or Escape): the system keeps the mouse
    /// until then. Returns True if there was something to drag.
    pub fn start(paths: &[PathBuf]) -> bool {
        unsafe {
            let _ = OleInitialize(None); // the window did it already: then this does nothing
            let Some(data) = data_object(paths) else { return false };
            let source: IDropSource = DragSource.into();
            let mut effect = DROPEFFECT_NONE;
            let _ = DoDragDrop(&data, &source, DROPEFFECT_COPY | DROPEFFECT_MOVE | DROPEFFECT_LINK, &mut effect);

            // the release of the button went to the drag, not to the window: tell the window, or egui
            // takes the button for pressed until the next click
            if let Some(hwnd) = window() {
                let mut at = POINT::default();
                if GetCursorPos(&mut at).is_ok() && ScreenToClient(hwnd, &mut at).as_bool() {
                    let lparam = ((at.y as i16 as u16 as isize) << 16) | (at.x as i16 as u16 as isize);
                    let _ = PostMessageW(Some(hwnd), WM_LBUTTONUP, WPARAM(0), LPARAM(lparam));
                }
            }
            true
        }
    }
}

/// Windows: the window of the frame, the one the drop target is installed on
#[cfg(target_os = "windows")]
pub(crate) fn set_window(window: &impl raw_window_handle::HasWindowHandle) {
    if let Ok(handle) = window.window_handle() {
        if let raw_window_handle::RawWindowHandle::Win32(h) = handle.as_raw() {
            win::WINDOW.store(h.hwnd.get(), std::sync::atomic::Ordering::Relaxed);
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn set_window(_window: &impl raw_window_handle::HasWindowHandle) {}

/// Windows: files and attachments dragged from other applications go through our own drop target
#[cfg(target_os = "windows")]
pub(crate) fn pump_drop_input(ctx: &eframe::egui::Context, raw: &mut eframe::egui::RawInput) {
    win::drop_in::pump(ctx, raw);
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn pump_drop_input(_ctx: &eframe::egui::Context, _raw: &mut eframe::egui::RawInput) {}

#[cfg(target_os = "windows")]
pub(crate) fn start_drag(paths: &[PathBuf]) -> bool {
    let started = win::start(paths);
    if started {
        LAST_DRAG.with(|l| *l.borrow_mut() = paths.to_vec());
    }
    started
}

/// The pointer in the points of the window, from the system (Windows)
#[cfg(target_os = "windows")]
pub(crate) fn native_pointer() -> Option<eframe::egui::Pos2> {
    let ppp = unsafe { panel_parent_ui(&UI) }.ok()?.ctx().pixels_per_point();
    win::pointer(ppp).map(|(x, y)| eframe::egui::pos2(x, y))
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) fn native_pointer() -> Option<eframe::egui::Pos2> {
    None
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn start_drag(_paths: &[PathBuf]) -> bool {
    false
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // the file views of the core take part in the drag and drop of the system through these
    pyegui_core::files::set_native_dnd(pyegui_core::files::NativeDnd { pointer: native_pointer, start_drag });
    m.add_function(wrap_pyfunction!(start_file_drag, m)?)?;
    m.add_function(wrap_pyfunction!(dropped_files, m)?)?;
    m.add_function(wrap_pyfunction!(dragged_files, m)?)?;
    m.add_function(wrap_pyfunction!(system_modifiers, m)?)?;
    m.add_function(wrap_pyfunction!(hovered_files, m)?)?;
    Ok(())
}
