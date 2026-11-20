//! "Open with Finder" for the short context menu of the Windows 11 Explorer.
//!
//! The Explorer loads this COM class (declared by the sparse package) and asks it for a title, an icon and,
//! when the user clicks, to `Invoke` on the selected item. The command line comes from `finder_open.cfg`, a
//! text file next to the DLL written by the installer: line 1 is the program (pythonw.exe), line 2 the script
//! (finder.py). The path of the item is passed as the only argument.
#![allow(non_snake_case)]

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicIsize, Ordering};

use windows::core::{implement, BOOL, Error, Interface, Ref, Result, GUID, HRESULT, PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, E_FAIL, E_NOTIMPL, E_POINTER, HMODULE,
    S_FALSE,
};
use windows::Win32::System::Com::{CoTaskMemFree, IBindCtx, IClassFactory, IClassFactory_Impl};
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::Threading::{CreateProcessW, CREATE_NO_WINDOW, PROCESS_INFORMATION, STARTUPINFOW};
use windows::Win32::UI::Shell::{
    IEnumExplorerCommand, IExplorerCommand, IExplorerCommand_Impl, IShellItem, IShellItemArray, SHStrDupW,
    ECF_DEFAULT, ECS_ENABLED, SIGDN_FILESYSPATH,
};

/// Identifies this command in the manifest (`com:Class Id`, `desktop4:... Clsid`).
const CLSID_FINDER_OPEN: GUID = GUID::from_u128(0x7b1c4e52_3a9d_4f6b_8e21_5d0a9c3f7b14);

const TITLE: &str = "Ouvrir avec Finder";

static MODULE: AtomicIsize = AtomicIsize::new(0);

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// The folder of this DLL.
fn module_dir() -> Option<PathBuf> {
    let mut buffer = [0u16; 1024];
    let module = HMODULE(MODULE.load(Ordering::Relaxed) as *mut c_void);
    let len = unsafe { GetModuleFileNameW(Some(module), &mut buffer) } as usize;
    if len == 0 {
        return None;
    }
    PathBuf::from(String::from_utf16_lossy(&buffer[..len])).parent().map(|p| p.to_path_buf())
}

/// (program, script) from `finder_open.cfg`.
fn read_config() -> Option<(String, String)> {
    let text = std::fs::read_to_string(module_dir()?.join("finder_open.cfg")).ok()?;
    let mut lines = text.lines().map(|l| l.trim().trim_start_matches('\u{feff}')).filter(|l| !l.is_empty());
    Some((lines.next()?.to_string(), lines.next()?.to_string()))
}

fn item_path(item: &IShellItem) -> Option<String> {
    unsafe {
        let name = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let text = name.to_string().ok();
        CoTaskMemFree(Some(name.0 as *const c_void));
        text
    }
}

fn first_path(items: Option<&IShellItemArray>) -> Option<String> {
    let items = items?;
    unsafe {
        if items.GetCount().ok()? == 0 {
            return None;
        }
        item_path(&items.GetItemAt(0).ok()?)
    }
}

fn launch(program: &str, script: &str, path: &str) -> Result<()> {
    let mut command = wide(&format!("\"{program}\" \"{script}\" \"{path}\""));
    let directory = std::path::Path::new(script)
        .parent()
        .map(|p| p.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<u16>>());
    let startup = STARTUPINFOW { cb: std::mem::size_of::<STARTUPINFOW>() as u32, ..Default::default() };
    let mut info = PROCESS_INFORMATION::default();
    unsafe {
        CreateProcessW(
            PCWSTR::null(),
            Some(PWSTR(command.as_mut_ptr())),
            None,
            None,
            false,
            CREATE_NO_WINDOW,
            None,
            directory.as_ref().map_or(PCWSTR::null(), |d| PCWSTR(d.as_ptr())),
            &startup,
            &mut info,
        )?;
        let _ = CloseHandle(info.hProcess);
        let _ = CloseHandle(info.hThread);
    }
    Ok(())
}

#[implement(IExplorerCommand)]
struct FinderOpen;

impl IExplorerCommand_Impl for FinderOpen_Impl {
    fn GetTitle(&self, _items: Ref<'_, IShellItemArray>) -> Result<PWSTR> {
        unsafe { SHStrDupW(PCWSTR(wide(TITLE).as_ptr())) }
    }

    fn GetIcon(&self, _items: Ref<'_, IShellItemArray>) -> Result<PWSTR> {
        let icon = module_dir().ok_or_else(|| Error::from(E_FAIL))?.join("finder.ico");
        unsafe { SHStrDupW(PCWSTR(wide(&icon.to_string_lossy()).as_ptr())) }
    }

    fn GetToolTip(&self, _items: Ref<'_, IShellItemArray>) -> Result<PWSTR> {
        Err(E_NOTIMPL.into())
    }

    fn GetCanonicalName(&self) -> Result<GUID> {
        Ok(CLSID_FINDER_OPEN)
    }

    fn GetState(&self, _items: Ref<'_, IShellItemArray>, _slow: BOOL) -> Result<u32> {
        Ok(ECS_ENABLED.0 as u32)
    }

    fn Invoke(&self, items: Ref<'_, IShellItemArray>, _ctx: Ref<'_, IBindCtx>) -> Result<()> {
        let path = first_path(items.as_ref()).ok_or_else(|| Error::from(E_FAIL))?;
        let (program, script) = read_config().ok_or_else(|| Error::from(E_FAIL))?;
        launch(&program, &script, &path)
    }

    fn GetFlags(&self) -> Result<u32> {
        Ok(ECF_DEFAULT.0 as u32)
    }

    fn EnumSubCommands(&self) -> Result<IEnumExplorerCommand> {
        Err(E_NOTIMPL.into())
    }
}

#[implement(IClassFactory)]
struct Factory;

impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(&self, outer: Ref<'_, windows::core::IUnknown>, iid: *const GUID, object: *mut *mut c_void) -> Result<()> {
        if object.is_null() {
            return Err(E_POINTER.into());
        }
        unsafe { *object = std::ptr::null_mut() };
        if outer.is_some() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        let command: IExplorerCommand = FinderOpen.into();
        unsafe { command.query(iid, object).ok() }
    }

    fn LockServer(&self, _lock: BOOL) -> Result<()> {
        Ok(())
    }
}

#[no_mangle]
extern "system" fn DllMain(module: HMODULE, _reason: u32, _reserved: *mut c_void) -> BOOL {
    MODULE.store(module.0 as isize, Ordering::Relaxed);
    BOOL(1)
}

#[no_mangle]
extern "system" fn DllGetClassObject(clsid: *const GUID, iid: *const GUID, object: *mut *mut c_void) -> HRESULT {
    unsafe {
        if clsid.is_null() || iid.is_null() || object.is_null() {
            return E_POINTER;
        }
        if *clsid != CLSID_FINDER_OPEN {
            return CLASS_E_CLASSNOTAVAILABLE;
        }
        let factory: IClassFactory = Factory.into();
        factory.query(iid, object)
    }
}

#[no_mangle]
extern "system" fn DllCanUnloadNow() -> HRESULT {
    // the hosting process (dllhost) is short lived and the package removal ends it: never unload on our own
    S_FALSE
}
