"""Two docks with their own sheet, a blue one and a green one, semi transparent over a glass backdrop.

    python examples/markdown_viewer.py [folder]

The sidebar lists the markdown files of a folder (docs/framework of this repository by default).
The viewer shows the selected one; the docks on the right give its outline, its source and a few
facts. The docks can be moved by their tab and closed; they are laid out again at the next run.
"""

import sys
import time
from pathlib import Path

from pyegui import *

FOLDER = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent / "docs" / "framework"

files = sorted(FOLDER.glob("*.md"))
current = Str(str(files[0]) if files else "")
started = Bool(False)
cache = {"path": None, "text": ""}


def text():
    """the text of the selected file, read again only when another one is selected"""
    if current.value != cache["path"]:
        cache.update(path=current.value, text=Path(current.value).read_text(encoding="utf-8") if current.value else "")
    return cache["text"]


# the sidebar


def sidebar():
    weak(FOLDER.name)
    if not files:
        weak("No markdown file here")

    def list_files():
        for path in files:
            if selectable_label_clicked(current.value == str(path), path.stem):
                current.value = str(path)

    scroll_area_vertical(list_files)


# the central view and the docks


def viewer():
    markdown(text(), scrollable=True, id="viewer")


def outline():
    """the headings of the text, indented by level"""
    in_code = False
    for line in text().splitlines():
        if line.startswith("```"):
            in_code = not in_code
        if not in_code and line.startswith("#"):
            level = len(line) - len(line.lstrip("#"))
            title = line[level:].strip()
            if title:

                def row(level=level, title=title):
                    add_space(10 * (level - 1))
                    label(title, truncate=True)

                horizontal(row)


def source():
    code(text())


def infos():
    if not current.value:
        weak("Nothing selected")
        return
    path = Path(current.value)
    body = text()
    label(path.name)
    weak(str(path.parent))
    separator()
    label(f"{len(body.split())} words, {len(body.splitlines())} lines")
    label(f"{path.stat().st_size} bytes")
    label("Modified " + time.strftime("%Y-%m-%d %H:%M", time.localtime(path.stat().st_mtime)))


def topbar():
    def row():
        heading(Path(current.value).stem if current.value else "Markdown viewer")
        weak(f"{len(files)} files")

    horizontal(row)


def bottombar():
    small(f"{FOLDER}")


def scrolling(ui):
    return lambda: scroll_area_vertical(ui)


BLUE = (59, 130, 246, 150)
GREEN = (34, 197, 94, 150)

dock = Dock(
    DockContainer(
        "horizontal",
        [
            DockPane("Viewer", viewer, hide_tab=True),
            DockContainer(
                "vertical",
                [
                    # a blue dock and a green dock: the `sheet` of a pane is the background of the pane and of its tab
                    DockContainer(
                        "tabs",
                        [
                            DockPane("Outline", scrolling(outline), sheet=BLUE),
                            DockPane("Source", scrolling(source), sheet=BLUE),
                        ],
                    ),
                    DockContainer(
                        "tabs",
                        [
                            DockPane("Infos", scrolling(infos), sheet=GREEN),
                            DockPane("Words", scrolling(source), sheet=GREEN),
                        ],
                    ),
                ],
            ),
        ],
    ),
    id="markdown_viewer",
    tab_bar_height=26,
)
layout = dock.save()
layout["shares"] = [3.0, 1.0]  # the viewer takes three quarters of the width
dock.load(layout)


def update_func(ctx):
    if not started.value:
        ctx.set_glass_style()
        ctx.set_system_theme()
        started.value = True

    sidebar_panel(sidebar, default_size=200, min_size=140)
    topbar_panel(topbar)
    bottombar_panel(bottombar)
    dock_area(dock)


if __name__ == "__main__":
    run_native(
        "Markdown viewer",
        update_func,
        inner_width=1100,
        inner_height=700,
        fullsize_content=True,
        backdrop=Backdrop.LIQUID_GLASS if sys.platform == "darwin" else Backdrop.ACRYLIC,
        # the same spacing and corners everywhere; the background is what is seen between the boxes, the
        # sheet what is seen inside them, and the docks are a bit more opaque to stay readable
        layout=dict(
            gap=8,
            rounding=12,
            background=(30, 30, 40, 110),  # seen between the boxes: a dark tint over the backdrop
            sheet=(255, 255, 255),  # seen inside the boxes
            opacity=0.55,
            dock=dict(opacity=0.8),
        ),
    )
