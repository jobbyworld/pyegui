"""File views kept up to date with the disk: a tree in the sidebar, a list or icons in the middle
a preview of the selected text file and a log of the events the system sends (to debug).

    python examples/files.py [folder]

The three views share one FileWatcher: create, rename or remove a file in a terminal or in the
Finder and it shows at once. The preview reads the selected file again when an editor saves it.
"""

import sys
import time
from collections import deque
from pathlib import Path

from pyegui import *

FOLDER = Path(sys.argv[1]).expanduser() if len(sys.argv) > 1 else Path.home()

watcher = FileWatcher()
tree = FileTreeView("tree", FOLDER, watcher=watcher)
files = FileListView("list", FOLDER, watcher=watcher)
icons = FileIconView("icons", FOLDER, watcher=watcher)

view = Int(0)  # 0 list, 1 icons
search = Str("")
hidden = Bool(False)
started = Bool(False)
preview_state = {"path": None, "text": ""}
last_tree_selection = []
events = deque(maxlen=500)  # the events of the watcher, newest first
paused = Bool(False)
icon_size = Float(56)  # the size of the grid of the icon view, set by the slider of the bottom bar


def shown():
    return [files, icons][view.value]


def other():
    return [icons, files][view.value]


# the sidebar: the tree, a folder selected there is shown in the middle


def sidebar():
    weak(FOLDER.name or str(FOLDER))
    scroll_area_both(lambda: file_tree_view(tree, context_menu=menu))
    selected = tree.selected
    if selected != last_tree_selection:  # selected in the tree this frame
        last_tree_selection[:] = selected
        if len(selected) == 1 and Path(selected[0]).is_dir():
            shown().go(selected[0])
    for path in tree.activated:
        context[0].open_url(Path(path).as_uri())


# the bars


def toolbar():
    def row():
        view_now = shown()
        add_enabled(view_now.can_go_back, lambda: button_clicked("◀") and view_now.back())
        add_enabled(view_now.can_go_forward, lambda: button_clicked("▶") and view_now.forward())
        if button_clicked("⬆"):
            view_now.up()
        # the name of the folder; under the mouse a menu of its ancestors, like a Finder window
        chosen = path_combo(view_now.folder, context_menu=path_menu)
        if chosen:
            view_now.go(chosen)
        if segmented_control(view, [(None, "List"), (None, "Icons")]):
            # the other view takes the place of the first one, at the same folder and selection
            shown().go(other().folder)
            shown().selected = other().selected
        text_edit_singleline(search, hint_text="Filter")
        checkbox(hidden, "Hidden files")

    horizontal_centered(row)


def path_menu(path):
    """the menu of an element of the path, on a right click on the breadcrumb or the title"""
    strong(Path(path).name or path)
    separator()
    if button_clicked("Open"):
        go_or_reveal(path)
        close_menu()
    if button_clicked("Show in Finder" if sys.platform == "darwin" else "Show in the file manager"):
        target = path if Path(path).is_dir() else str(Path(path).parent)
        context[0].open_url(Path(target).as_uri())
        close_menu()
    if button_clicked("Copy path"):
        context[0].copy_text(path)
        close_menu()


def go_or_reveal(path):
    """a folder is entered, a file is selected in its folder"""
    if Path(path).is_dir():
        shown().go(path)
    else:
        shown().go(str(Path(path).parent))
        shown().reveal(path)


def statusbar():
    def row():
        view_now = shown()
        # the path bar of the Finder: the selected item if there is one, else the folder
        selected = view_now.selected
        path = selected[-1] if len(selected) == 1 else view_now.folder
        right = 130 + (200 if view.value == 1 else 0)  # the room of what is at the right
        path_breadcrumb(
            path,
            max_width=available_width() - right,
            on_click=go_or_reveal,  # the events of the mouse call functions
            context_menu=path_menu,  # a right click opens this menu
        )
        add_space(max(available_width() - right, 8))
        small(f"{len(view_now.items)} items, {len(selected)} selected")
        if view.value == 1:
            # the size of the grid, at the right like in the Finder
            add_space(12)
            small("Size")
            slider_float(icon_size, 16, 256, "")

    horizontal_centered(row)


# the middle: the files, and a preview of the selected one


def menu(paths):
    if paths and button_clicked("Open"):
        for path in paths:
            context[0].open_url(Path(path).as_uri())
        close_menu()
    if button_clicked("Hide hidden files" if hidden.value else "Show hidden files"):
        hidden.value = not hidden.value
        close_menu()


def files_pane():
    view_now = shown()
    view_now.filter = search.value
    view_now.show_hidden = tree.show_hidden = hidden.value
    if view.value == 0:
        file_list_view(files, context_menu=menu)
    else:
        icons.icon_size = icon_size.value
        file_icon_view(icons, context_menu=menu)
    for path in view_now.activated:
        context[0].open_url(Path(path).as_uri())  # a file opens with its app


def preview_text(path):
    """the text of the file, read again when it changed on the disk"""
    if path != preview_state["path"]:
        if preview_state["path"]:
            watcher.unwatch(preview_state["path"])
        watcher.watch(path)
        watcher.changed(path)  # starts counting
        preview_state.update(path=path, text=None)
    if preview_state["text"] is None or watcher.changed(path):
        try:
            if Path(path).stat().st_size > 200_000:
                preview_state["text"] = "(too large to preview)"
            else:
                preview_state["text"] = Path(path).read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            preview_state["text"] = "(no preview)"
    return preview_state["text"]


def preview():
    selected = shown().selected
    if len(selected) != 1 or Path(selected[0]).is_dir():
        weak("Select a text file")
        return
    path = selected[0]
    strong(Path(path).name)
    separator()
    scroll_area_both(lambda: code(preview_text(path)))


def collect_events():
    """the events of the system since the last frame, read at every frame so none is lost"""
    for event in watcher.take_events():
        if not paused.value:
            events.appendleft(event)


def short(path):
    """a path from the folder shown, to keep the lines short"""
    try:
        return str(Path(path).relative_to(FOLDER)) or "."
    except ValueError:
        return path


def log_pane():
    def bar():
        if button_clicked("Clear"):
            events.clear()
        checkbox(paused, "Pause")
        weak(f"{len(events)} events · {len(watcher.watched)} folders watched")

    horizontal(bar)
    separator()

    def lines():
        for event in events:
            stamp = time.strftime("%H:%M:%S", time.localtime(event["time"]))
            millis = int(event["time"] % 1 * 1000)
            paths = ", ".join(short(p) for p in event["paths"]) or "-"
            monospace(f"{stamp}.{millis:03d} {event['kind']} {paths}")

    scroll_area_both(lines)


dock = Dock(
    DockContainer(
        "horizontal",
        [
            DockPane("Files", files_pane, hide_tab=True),
            DockContainer(
                "vertical",
                [DockPane("Preview", preview, closable=True), DockPane("Log", log_pane, closable=True)],
            ),
        ],
    ),
    id="files_example",
    tab_bar_height=26,
)
layout = dock.save()
layout["shares"] = [2.0, 1.0]
dock.load(layout)

context = [None]


def update_func(ctx):
    context[0] = ctx
    collect_events()
    if not started.value:
        ctx.set_system_theme()
        started.value = True

    sidebar_panel(sidebar, default_size=240, min_size=160)
    topbar_panel(toolbar)
    bottombar_panel(statusbar)
    dock_area(dock)


if __name__ == "__main__":
    run_native(
        "Files",
        update_func,
        inner_width=1150,
        inner_height=720,
        fullsize_content=True,
        layout=dict(gap=4, rounding=12),
    )
