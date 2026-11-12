r"""Create a pinnable Windows shortcut for a pyegui app (the Finder example by default).

    python scripts\make_shortcut_windows.py                  # Start menu: Finder
    python scripts\make_shortcut_windows.py --desktop        # + Desktop
    python scripts\make_shortcut_windows.py examples\app.py --name Atelier --icon my.ico --app-id pyegui.Atelier

The shortcut starts the pythonw.exe of the venv (no console window) on the script, and carries
the same AppUserModelID as the running app (the script calls
SetCurrentProcessExplicitAppUserModelID with it, see examples/finder.py), so that pinning to the
taskbar groups the window under the icon of the shortcut instead of Python's.

Run it with a Python that has pywin32 (`pip install pywin32`). It is only a tool of the
repository: pyegui itself does not depend on it.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
DEFAULT_SCRIPT = REPO / "examples" / "finder.py"
DEFAULT_ICON = REPO / "examples" / "app_icon" / "finder.ico"
DEFAULT_NAME = "Finder"
DEFAULT_APP_ID = "pyegui.Finder"


def shortcut_spec(script: Path, name: str, icon: Path, app_id: str, python: Path) -> dict[str, str]:
    """Everything the .lnk needs; pure function (testable on any OS)."""
    return {
        "target": str(python.with_name("pythonw.exe")),
        "arguments": f'"{script}"',
        "workdir": str(script.parent),
        "icon": str(icon),
        "description": f"Lance {name}",
        "app_id": app_id,
    }


def repair_broken_pythonw(python: Path) -> str | None:
    """Some ``uv venv`` versions give the venv's ``pythonw.exe`` the same (console) launcher
    as ``python.exe`` instead of a real GUI one: the shortcut then still flashes a console
    window. Detected as: the two files are byte-identical. Repaired by copying the genuine
    ``pythonw.exe`` of the base interpreter (found via ``pyvenv.cfg``) next to it, with its
    sibling ``*.dll``. Returns a message to print, or None if nothing needed fixing."""
    import filecmp
    import shutil

    pythonw = python.with_name("pythonw.exe")
    if not python.exists() or not pythonw.exists():
        return None
    if not filecmp.cmp(python, pythonw, shallow=False):
        return None  # already a distinct binary: assume it is a real one
    cfg = python.parent.parent / "pyvenv.cfg"
    if not cfg.exists():
        return None
    home = next(
        (
            line.split("=", 1)[1].strip()
            for line in cfg.read_text().splitlines()
            if line.strip().startswith("home")
        ),
        None,
    )
    if not home:
        return None
    base_pythonw = Path(home) / "pythonw.exe"
    if not base_pythonw.exists() or filecmp.cmp(base_pythonw, pythonw, shallow=False):
        return None
    shutil.copyfile(base_pythonw, pythonw)
    for dll in Path(home).glob("*.dll"):
        shutil.copyfile(dll, pythonw.with_name(dll.name))
    return f"pythonw.exe of the venv repaired (it was a copy of python.exe): {pythonw}"


def create_shortcut(path: Path, spec: dict[str, str]) -> None:
    from win32com.client import Dispatch
    from win32com.propsys import propsys, pscon
    from win32com.shell import shellcon

    shortcut = Dispatch("WScript.Shell").CreateShortCut(str(path))
    shortcut.TargetPath = spec["target"]
    shortcut.Arguments = spec["arguments"]
    shortcut.WorkingDirectory = spec["workdir"]
    shortcut.IconLocation = spec["icon"]
    shortcut.Description = spec["description"]
    shortcut.save()

    store = propsys.SHGetPropertyStoreFromParsingName(
        str(path), None, shellcon.GPS_READWRITE, propsys.IID_IPropertyStore
    )
    store.SetValue(pscon.PKEY_AppUserModel_ID, propsys.PROPVARIANTType(spec["app_id"]))
    store.Commit()


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("script", nargs="?", type=Path, default=DEFAULT_SCRIPT, help="the app to start (default: examples/finder.py)")
    ap.add_argument("--name", default=DEFAULT_NAME, help="name of the shortcut (default: Finder)")
    ap.add_argument("--icon", type=Path, default=DEFAULT_ICON, help=".ico file of the shortcut")
    ap.add_argument("--app-id", default=DEFAULT_APP_ID, help="AppUserModelID, the one the app sets itself")
    ap.add_argument("--desktop", action="store_true", help="also create a shortcut on the Desktop")
    ap.add_argument("--python", type=Path, default=None, help=r"python.exe of the venv (default: <repo>\.venv\Scripts\python.exe)")
    args = ap.parse_args(argv)
    if sys.platform != "win32":
        print("This script only works on Windows.")
        return 1
    script = args.script.resolve()
    python = args.python or REPO / ".venv" / "Scripts" / "python.exe"
    if not script.exists():
        print(f"Not found: {script}")
        return 1
    if not python.exists():
        print(f"Not found: {python}\nCreate the venv first: py -3 -m venv .venv && .venv\\Scripts\\pip install maturin && .venv\\Scripts\\maturin develop --release")
        return 1
    repaired = repair_broken_pythonw(python)
    if repaired:
        print(repaired)
    spec = shortcut_spec(script, args.name, args.icon.resolve(), args.app_id, python)

    from win32com.shell import shell, shellcon

    folders = [shell.SHGetFolderPath(0, shellcon.CSIDL_PROGRAMS, None, 0)]
    if args.desktop:
        folders.append(shell.SHGetFolderPath(0, shellcon.CSIDL_DESKTOPDIRECTORY, None, 0))
    for folder in folders:
        path = Path(folder) / f"{args.name}.lnk"
        create_shortcut(path, spec)
        print(f"Shortcut created: {path}")
    print(f"To pin it: Start menu -> {args.name} -> right click -> Pin to taskbar.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
