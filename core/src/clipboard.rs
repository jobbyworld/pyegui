//! The clipboard of the system for files: a list of paths read and written as the file managers do
//! (NSPasteboard file URLs on macOS, CF_HDROP and "Preferred DropEffect" on Windows). On the other
//! systems nothing is read or written: the caller keeps its own list.
//!
//! Cut: Windows marks it in the clipboard (drop effect move). macOS has no cut for files: the
//! pasteboard is remembered by its change count, and the files are known as cut while it did not change.

use std::path::PathBuf;

/// Puts the files in the clipboard of the system. False if the system could not take them.
pub fn write_files(paths: &[PathBuf], cut: bool) -> bool {
    imp::write(paths, cut)
}

/// The files in the clipboard of the system, and whether they were cut. Empty if there is none.
pub fn read_files() -> (Vec<PathBuf>, bool) {
    imp::read()
}

/// True if the clipboard of the system holds files.
pub fn has_files() -> bool {
    imp::has()
}

#[cfg(target_os = "macos")]
mod imp {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicIsize, Ordering};

    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, ProtocolObject};
    use objc2::ClassType;
    use objc2_app_kit::{NSPasteboard, NSPasteboardWriting};
    use objc2_foundation::{NSArray, NSString, NSURL};

    /// Change count of the pasteboard just after a cut (-1: none)
    static CUT: AtomicIsize = AtomicIsize::new(-1);

    fn urls(board: &NSPasteboard) -> Vec<Retained<NSURL>> {
        let classes = NSArray::from_slice(&[NSURL::class()]);
        let Some(objects) = (unsafe { board.readObjectsForClasses_options(&classes, None) }) else { return Vec::new() };
        (0..objects.count())
            .map(|i| objects.objectAtIndex(i))
            .filter_map(|o: Retained<AnyObject>| o.downcast::<NSURL>().ok())
            .filter(|u| u.isFileURL())
            .collect()
    }

    pub fn write(paths: &[PathBuf], cut: bool) -> bool {
        let board = NSPasteboard::generalPasteboard();
        let owned: Vec<Retained<NSURL>> = paths
            .iter()
            .filter_map(|p| p.to_str())
            .map(|p| NSURL::fileURLWithPath(&NSString::from_str(p)))
            .collect();
        if owned.is_empty() {
            return false;
        }
        let writers: Vec<&ProtocolObject<dyn NSPasteboardWriting>> = owned.iter().map(|u| ProtocolObject::from_ref(&**u)).collect();
        board.clearContents();
        let ok = board.writeObjects(&NSArray::from_slice(&writers));
        CUT.store(if ok && cut { board.changeCount() } else { -1 }, Ordering::SeqCst);
        ok
    }

    pub fn read() -> (Vec<PathBuf>, bool) {
        let board = NSPasteboard::generalPasteboard();
        let files = urls(&board).iter().filter_map(|u| u.path()).map(|p| PathBuf::from(p.to_string())).collect();
        (files, CUT.load(Ordering::SeqCst) == board.changeCount())
    }

    pub fn has() -> bool {
        !urls(&NSPasteboard::generalPasteboard()).is_empty()
    }
}

#[cfg(target_os = "windows")]
mod imp {
    use std::os::windows::ffi::OsStrExt;
    use std::path::PathBuf;

    use windows::core::w;
    use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW,
        SetClipboardData,
    };
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};
    use windows::Win32::System::Ole::CF_HDROP;
    use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};

    const DROPEFFECT_COPY: u32 = 1;
    const DROPEFFECT_MOVE: u32 = 2;

    /// The clipboard is open for the life of the guard, closed on drop
    struct Open;
    impl Open {
        fn new() -> Option<Open> {
            // another program may hold it for a moment
            for _ in 0..10 {
                if unsafe { OpenClipboard(Some(HWND::default())) }.is_ok() {
                    return Some(Open);
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            None
        }
    }
    impl Drop for Open {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseClipboard();
            }
        }
    }

    /// Puts bytes in a movable block of memory and gives it to the clipboard
    unsafe fn put(format: u32, bytes: &[u8]) -> bool {
        let Ok(block) = GlobalAlloc(GMEM_MOVEABLE, bytes.len()) else { return false };
        let ptr = GlobalLock(block) as *mut u8;
        if ptr.is_null() {
            return false;
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
        let _ = GlobalUnlock(block);
        SetClipboardData(format, Some(HANDLE(block.0))).is_ok()
    }

    pub fn write(paths: &[PathBuf], cut: bool) -> bool {
        if paths.is_empty() {
            return false;
        }
        // DROPFILES: offset of the list (20), point (8), not client area (4), wide characters (4), then
        // the paths each ended by a nul, and one more nul
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&20u32.to_le_bytes());
        bytes.extend_from_slice(&[0; 8]);
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        for p in paths {
            for unit in p.as_os_str().encode_wide() {
                bytes.extend_from_slice(&unit.to_le_bytes());
            }
            bytes.extend_from_slice(&[0, 0]);
        }
        bytes.extend_from_slice(&[0, 0]);
        let effect = if cut { DROPEFFECT_MOVE } else { DROPEFFECT_COPY };
        let Some(_open) = Open::new() else { return false };
        unsafe {
            if EmptyClipboard().is_err() || !put(CF_HDROP.0 as u32, &bytes) {
                return false;
            }
            let format = RegisterClipboardFormatW(w!("Preferred DropEffect"));
            put(format, &effect.to_le_bytes());
        }
        true
    }

    pub fn read() -> (Vec<PathBuf>, bool) {
        let mut files = Vec::new();
        let mut cut = false;
        let Some(_open) = Open::new() else { return (files, cut) };
        unsafe {
            if let Ok(handle) = GetClipboardData(CF_HDROP.0 as u32) {
                let hdrop = HDROP(handle.0);
                let count = DragQueryFileW(hdrop, u32::MAX, None);
                for i in 0..count {
                    let len = DragQueryFileW(hdrop, i, None) as usize;
                    let mut buffer = vec![0u16; len + 1];
                    let n = DragQueryFileW(hdrop, i, Some(&mut buffer)) as usize;
                    files.push(PathBuf::from(String::from_utf16_lossy(&buffer[..n])));
                }
            }
            let format = RegisterClipboardFormatW(w!("Preferred DropEffect"));
            if let Ok(handle) = GetClipboardData(format) {
                let block = HGLOBAL(handle.0);
                if GlobalSize(block) >= 4 {
                    let ptr = GlobalLock(block) as *const u32;
                    if !ptr.is_null() {
                        cut = ptr.read_unaligned() & DROPEFFECT_MOVE != 0;
                        let _ = GlobalUnlock(block);
                    }
                }
            }
        }
        (files, cut)
    }

    pub fn has() -> bool {
        unsafe { IsClipboardFormatAvailable(CF_HDROP.0 as u32).is_ok() }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod imp {
    use std::path::PathBuf;

    pub fn write(_paths: &[PathBuf], _cut: bool) -> bool {
        false
    }
    pub fn read() -> (Vec<PathBuf>, bool) {
        (Vec::new(), false)
    }
    pub fn has() -> bool {
        false
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use std::path::PathBuf;

    #[test]
    fn round_trip() {
        let files = vec![PathBuf::from("/etc/hosts"), PathBuf::from("/usr/bin/true")];
        assert!(super::write_files(&files, true));
        assert!(super::has_files());
        assert_eq!(super::read_files(), (files.clone(), true));
        assert!(super::write_files(&files, false));
        assert_eq!(super::read_files(), (files, false));
    }
}
