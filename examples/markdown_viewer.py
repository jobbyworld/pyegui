"""A minimal app: a sidebar, a markdown viewer in the middle and three docks on the right.

    python examples/markdown_viewer.py [folder]

The sidebar lists the markdown files of a folder (docs/framework of this repository by default).
The viewer shows the selected one; the docks on the right give its outline, its source, a few
facts and an export to PDF (`markdown_to_pdf`: theme, cover, table of contents, page numbers). The docks
can be moved by their tab and closed; they are laid out again at the next run.
"""

import os
import subprocess
import sys
import threading
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


# the export to PDF: its options, and the thread that writes the file (the window never waits)

THEMES = ["light", "dark", "print"]
theme = Int(0)
cover = Bool(True)
subtitle = Str("")
author = Str("")
toc = Bool(True)
numbers = Bool(True)
export = {"state": "", "path": None}  # state: "", "running", "done" or the text of an error


def start_export():
    path = Path(current.value)
    out = path.with_suffix(".pdf")
    options = dict(theme=THEMES[theme.value], toc=toc.value, footer=numbers.value, base_dir=str(path.parent))
    if author.value:
        options["author"] = author.value
    if cover.value:
        options["cover"] = {k: v for k, v in dict(subtitle=subtitle.value, author=author.value).items() if v}
    body = text()

    def run():
        try:
            markdown_to_pdf(body, str(out), **options)
            export.update(state="done", path=out)
        except Exception as error:  # a bad option or a file that cannot be written
            export.update(state=f"error: {error}")

    export.update(state="running", path=None)
    threading.Thread(target=run, daemon=True).start()


def open_file(path):
    if sys.platform == "win32":
        os.startfile(path)
    else:
        subprocess.Popen(["open" if sys.platform == "darwin" else "xdg-open", str(path)])


def export_pane():
    weak("The file is written next to the markdown file.")
    combo_box(theme, [0, 1, 2], ["Light", "Dark", "Print"], "Theme")
    checkbox(cover, "Cover page")
    if cover.value:
        text_edit_singleline(subtitle, hint_text="Subtitle")
    text_edit_singleline(author, hint_text="Author")
    checkbox(toc, "Table of contents")
    checkbox(numbers, "Page numbers")
    separator()
    state = export["state"]
    if state == "running":
        horizontal(lambda: (spinner(), label("Writing the PDF...")))
    elif button_clicked("Export to PDF") and current.value:
        start_export()
    if state == "done":
        label(export["path"].name)
        if button_clicked("Open"):
            open_file(export["path"])
    elif state.startswith("error"):
        label(state)


def scrolling(ui):
    return lambda: scroll_area_vertical(ui)


dock = Dock(
    DockContainer(
        "horizontal",
        [
            DockPane("Viewer", viewer, hide_tab=True),
            DockContainer(
                "vertical",
                [
                    DockPane("Outline", scrolling(outline), closable=True),
                    DockPane("Source", scrolling(source), closable=True),
                    DockPane("Infos", scrolling(infos), closable=True),
                    DockPane("Export PDF", scrolling(export_pane), closable=True),
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
        ctx.set_system_theme()
        started.value = True

    sidebar_panel(sidebar, default_size=200, min_size=140)
    dock_area(dock)


if __name__ == "__main__":
    run_native(
        "Markdown viewer",
        update_func,
        inner_width=1100,
        inner_height=700,
        fullsize_content=True,
        layout=dict(gap=8, rounding=12),  # the same spacing and corners for the sidebar, the docks and the bars
    )
