"""Open with Finder in the SHORT context menu of the Windows 11 Explorer (no "Show more options").

    python scripts/shellext_windows.py install   [--python PYTHONW] [--script FINDER.PY]
    python scripts/shellext_windows.py uninstall
    python scripts/shellext_windows.py status

How it works: `shellext/` is a DLL (Rust, an IExplorerCommand) declared by a *sparse* package (an
AppxManifest.xml with FileExplorerContextMenus for files, folders and the background of a folder). The
package is registered for the current user with `Add-AppxPackage -Register ... -ExternalLocation`: that needs
the Developer Mode of Windows (the sideloading of apps), not an administrator and not a certificate.

What `install` changes, all under the current user:
  * %LOCALAPPDATA%\\pyegui\\shellext\\ : the DLL, the icon, the command line (finder_open.cfg), the manifest;
  * the package "pyegui.FinderOpen" (Get-AppxPackage pyegui.FinderOpen).
Nothing else (no certificate, no registry key written by this script, no system setting). `uninstall` removes
the package and the folder. Both are safe to run twice. The registry entries of `finder.py` ("Afficher plus
d'options") are not touched: they stay as the fallback of the long menu.
"""

import argparse
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CRATE = ROOT / "shellext"
DLL_NAME = "pyegui_shellext.dll"
STUB_NAME = "pyegui_shellext_stub.exe"  # the manifest of an application needs an .exe name; never run
BUILT_DLL = CRATE / "target" / "release" / DLL_NAME
BUILT_STUB = CRATE / "target" / "release" / STUB_NAME
ICON = ROOT / "examples" / "app_icon" / "finder.ico"
LOGO = ROOT / "examples" / "app_icon" / "finder_256.png"
PACKAGE = "pyegui.FinderOpen"
CLSID = "7B1C4E52-3A9D-4F6B-8E21-5D0A9C3F7B14"  # the same as CLSID_FINDER_OPEN in shellext/src/lib.rs
INSTALL_DIR = Path(os.environ.get("LOCALAPPDATA", str(Path.home()))) / "pyegui" / "shellext"

MANIFEST = f"""<?xml version="1.0" encoding="utf-8"?>
<Package
  xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"
  xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10"
  xmlns:uap10="http://schemas.microsoft.com/appx/manifest/uap/windows10/10"
  xmlns:desktop4="http://schemas.microsoft.com/appx/manifest/desktop/windows10/4"
  xmlns:desktop5="http://schemas.microsoft.com/appx/manifest/desktop/windows10/5"
  xmlns:com="http://schemas.microsoft.com/appx/manifest/com/windows10"
  xmlns:rescap="http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities"
  IgnorableNamespaces="uap uap10 desktop4 desktop5 com rescap">
  <Identity Name="{PACKAGE}" Publisher="CN=pyegui" Version="1.0.0.0" ProcessorArchitecture="neutral" />
  <Properties>
    <DisplayName>Ouvrir avec Finder</DisplayName>
    <PublisherDisplayName>pyegui</PublisherDisplayName>
    <Logo>logo.png</Logo>
    <uap10:AllowExternalContent>true</uap10:AllowExternalContent>
  </Properties>
  <Resources><Resource Language="en-us" /></Resources>
  <Dependencies>
    <TargetDeviceFamily Name="Windows.Desktop" MinVersion="10.0.22000.0" MaxVersionTested="10.0.26100.0" />
  </Dependencies>
  <Capabilities><rescap:Capability Name="runFullTrust" /></Capabilities>
  <Applications>
    <Application Id="FinderOpen" Executable="{STUB_NAME}" uap10:TrustLevel="mediumIL" uap10:RuntimeBehavior="win32App">
      <uap:VisualElements AppListEntry="none" DisplayName="Ouvrir avec Finder" Description="Open with Finder"
        BackgroundColor="transparent" Square150x150Logo="logo.png" Square44x44Logo="logo.png" />
      <Extensions>
        <desktop4:Extension Category="windows.fileExplorerContextMenus">
          <desktop4:FileExplorerContextMenus>
            <desktop5:ItemType Type="*"><desktop5:Verb Id="FinderOpenFile" Clsid="{CLSID}" /></desktop5:ItemType>
            <desktop5:ItemType Type="Directory"><desktop5:Verb Id="FinderOpenFolder" Clsid="{CLSID}" /></desktop5:ItemType>
            <desktop5:ItemType Type="Directory\\Background"><desktop5:Verb Id="FinderOpenBackground" Clsid="{CLSID}" /></desktop5:ItemType>
          </desktop4:FileExplorerContextMenus>
        </desktop4:Extension>
        <com:Extension Category="windows.comServer">
          <com:ComServer>
            <com:SurrogateServer DisplayName="Ouvrir avec Finder">
              <com:Class Id="{CLSID}" Path="{DLL_NAME}" ThreadingModel="STA" />
            </com:SurrogateServer>
          </com:ComServer>
        </com:Extension>
      </Extensions>
    </Application>
  </Applications>
</Package>
"""


def powershell(script):
    """runs a PowerShell script, returns (exit code, output)"""
    done = subprocess.run(
        ["powershell", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script],
        capture_output=True, text=True, encoding="utf-8", errors="replace",
    )
    return done.returncode, (done.stdout + done.stderr).strip()


def is_installed():
    code, out = powershell(f"(Get-AppxPackage -Name {PACKAGE} | Measure-Object).Count")
    return code == 0 and out.strip() == "1"


def remove_package():
    code, out = powershell(f"Get-AppxPackage -Name {PACKAGE} | Remove-AppxPackage")
    if code != 0:
        raise SystemExit(f"Remove-AppxPackage failed: {out}")


def build_dll():
    """the DLL built by cargo (the Rust toolchain is needed only when it is not built yet)"""
    if BUILT_DLL.exists() and BUILT_STUB.exists():
        return BUILT_DLL
    if not shutil.which("cargo"):
        raise SystemExit(f"{BUILT_DLL} is missing and cargo is not installed: build it with `cargo build --release` in shellext/")
    print("building the DLL (cargo build --release)...")
    done = subprocess.run(["cargo", "build", "--release"], cwd=CRATE)
    if done.returncode != 0 or not (BUILT_DLL.exists() and BUILT_STUB.exists()):
        raise SystemExit("cargo build failed")
    return BUILT_DLL


def remove_folder():
    """deletes the install folder; the DLL can stay loaded for a moment by the process that hosted it"""
    for attempt in range(10):
        if not INSTALL_DIR.exists():
            break
        shutil.rmtree(INSTALL_DIR, ignore_errors=True)
        if INSTALL_DIR.exists():
            time.sleep(0.5)
    if INSTALL_DIR.exists():
        raise SystemExit(f"{INSTALL_DIR} could not be removed (a file is in use)")
    parent = INSTALL_DIR.parent
    if parent.exists() and not any(parent.iterdir()):
        parent.rmdir()


def default_pythonw():
    python = Path(sys.executable)
    pythonw = python.with_name("pythonw.exe")
    return pythonw if pythonw.exists() else python


def install(python, script):
    if sys.platform != "win32":
        raise SystemExit("Windows only")
    for path in (python, script, ICON, LOGO):
        if not Path(path).exists():
            raise SystemExit(f"missing: {path}")
    dll = build_dll()
    if is_installed():  # an update: the package must go first, its DLL is in use
        remove_package()
    INSTALL_DIR.mkdir(parents=True, exist_ok=True)
    try:
        shutil.copy2(dll, INSTALL_DIR / DLL_NAME)
    except PermissionError:  # still loaded by a hosting process of the previous registration
        time.sleep(1.0)
        shutil.copy2(dll, INSTALL_DIR / DLL_NAME)
    shutil.copy2(BUILT_STUB, INSTALL_DIR / STUB_NAME)
    shutil.copy2(ICON, INSTALL_DIR / "finder.ico")
    shutil.copy2(LOGO, INSTALL_DIR / "logo.png")
    (INSTALL_DIR / "finder_open.cfg").write_text(f"{Path(python).resolve()}\n{Path(script).resolve()}\n", encoding="utf-8")
    (INSTALL_DIR / "AppxManifest.xml").write_text(MANIFEST, encoding="utf-8")
    code, out = powershell(
        f"Add-AppxPackage -Register '{INSTALL_DIR / 'AppxManifest.xml'}' -ExternalLocation '{INSTALL_DIR}'"
    )
    if code != 0:
        remove_folder()
        raise SystemExit(f"Add-AppxPackage failed (is the Developer Mode of Windows on?): {out}")
    print("installed: 'Ouvrir avec Finder' is in the short context menu (restart explorer.exe if it does not show up).")


def uninstall():
    if sys.platform != "win32":
        raise SystemExit("Windows only")
    if is_installed():
        remove_package()
    remove_folder()
    print("removed: package and folder.")


def status():
    if sys.platform != "win32":
        print("none")
        return "none"
    state = "installed" if is_installed() and (INSTALL_DIR / DLL_NAME).exists() else "none"
    print(state)
    return state


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("action", choices=["install", "uninstall", "status"])
    parser.add_argument("--python", default=str(default_pythonw()), help="the program that runs finder.py (default: pythonw.exe of this environment)")
    parser.add_argument("--script", default=str(ROOT / "examples" / "finder.py"), help="the Finder script")
    args = parser.parse_args()
    if args.action == "install":
        install(args.python, args.script)
    elif args.action == "uninstall":
        uninstall()
    else:
        status()


if __name__ == "__main__":
    main()
