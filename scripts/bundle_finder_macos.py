"""Make Finder.app (and a .dmg) from the executable of finder-rs, the pure Rust Finder.

    cargo build --profile dist -p finder-rs
    python scripts/bundle_finder_macos.py                       # -> dist/Finder.app and dist/Finder.dmg
    python scripts/bundle_finder_macos.py --binary target/release/finder --out /tmp/out --no-dmg

The bundle holds the executable, an Info.plist and the icon: nothing else to install. It is signed ad hoc
(`codesign -s -`), so on another Mac the first start asks the user to confirm in System Settings > Privacy.
"""

from __future__ import annotations

import argparse
import plistlib
import shutil
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
ICON = REPO / "examples" / "app_icon" / "finder.icns"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--binary", type=Path, default=REPO / "target" / "dist" / "finder")
    parser.add_argument("--out", type=Path, default=REPO / "dist")
    parser.add_argument("--name", default="Finder")
    parser.add_argument("--bundle-id", default="org.pyegui.FinderRs")
    parser.add_argument("--version", default="0.1.0")
    parser.add_argument("--no-dmg", action="store_true")
    args = parser.parse_args()

    if not args.binary.is_file():
        print(f"executable not found: {args.binary} (cargo build --profile dist -p finder-rs)", file=sys.stderr)
        return 1
    app = args.out / f"{args.name}.app"
    if app.exists():
        shutil.rmtree(app)
    (app / "Contents" / "MacOS").mkdir(parents=True)
    (app / "Contents" / "Resources").mkdir()
    shutil.copy2(args.binary, app / "Contents" / "MacOS" / "finder")
    plist = {
        "CFBundleName": args.name,
        "CFBundleDisplayName": args.name,
        "CFBundleIdentifier": args.bundle_id,
        "CFBundleVersion": args.version,
        "CFBundleShortVersionString": args.version,
        "CFBundleExecutable": "finder",
        "CFBundlePackageType": "APPL",
        "LSMinimumSystemVersion": "11.0",
        "NSHighResolutionCapable": True,
        "NSSupportsAutomaticGraphicsSwitching": True,
        # the Finder is asked to put files in the trash, a terminal runs in the folders: say so once
        "NSAppleEventsUsageDescription": "Finder moves files to the Trash through the system Finder.",
    }
    if ICON.is_file():
        shutil.copy2(ICON, app / "Contents" / "Resources" / "finder.icns")
        plist["CFBundleIconFile"] = "finder"
    with open(app / "Contents" / "Info.plist", "wb") as f:
        plistlib.dump(plist, f)
    subprocess.run(["codesign", "--force", "--deep", "-s", "-", str(app)], check=True)
    print(f"-> {app}")
    if not args.no_dmg:
        dmg = args.out / f"{args.name}.dmg"
        dmg.unlink(missing_ok=True)
        subprocess.run(["hdiutil", "create", "-volname", args.name, "-srcfolder", str(app), "-ov", "-format", "UDZO", str(dmg)], check=True, stdout=subprocess.DEVNULL)
        print(f"-> {dmg}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
