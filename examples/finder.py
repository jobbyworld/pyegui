"""A mini Finder written only with the Python API of pyegui.

Sidebar with favorites, toolbar with back / forward, list, icon and tree views, search, a preview
panel and a path bar. The icons are the ones of the Finder on macOS.

The views are the file views of pyegui (FileListView, FileIconView, FileTreeView) sharing one
FileWatcher: what any program changes in the folder shows at once. The title of the toolbar is a
path_combo (a menu of the parent folders) and the path bar a path_breadcrumb.

Mouse: click to select, double click to open (folders here, files with their app). Right-click
opens the action palette. In the tree view, the arrow of a folder shows its contents in the tree.
Keyboard: arrows to select, Enter or Cmd+Down to open, Backspace or Cmd+Up for the parent folder,
Cmd+[ and Cmd+] to go back and forward, Escape to clear the search and the selection,
Space or Cmd+P to open the action palette (search and run an action for the current selection).

    python finder.py [folder]
"""

import os
import shlex
import shutil
import subprocess
import sys
import threading
import time
from datetime import datetime
from pathlib import Path
from urllib.parse import quote, unquote, urlsplit

from finder_tags import TagStore
from finder_tags import key as tag_key
from pyegui import (
    RGB,
    Backdrop,
    Bool,
    Dock,
    DockContainer,
    DockPane,
    FileIconView,
    FileListView,
    FileTreeView,
    FileWatcher,
    Int,
    Str,
    TagTreeView,
    Tags,
    Terminal,
    add_enabled,
    add_space,
    available_height,
    available_width,
    bottom_panel,
    button_clicked,
    close_menu,
    checkbox,
    code,
    code_view,
    colored_label,
    color_edit_button_rgb,
    combo_box,
    dock_area,
    draw_rect,
    end_row,
    file_icon,
    file_icon_view,
    file_list_view,
    file_tree_view,
    frame,
    grid,
    horizontal,
    horizontal_centered,
    image,
    image_button_clicked,
    interactive,
    key_pressed,
    label,
    left_panel,
    markdown,
    menu_image_button,
    modal,
    path_breadcrumb,
    path_combo,
    pointer_pos,
    pointer_pressed,
    run_native,
    scroll_area_both,
    scroll_area_vertical,
    scroll_to_rect,
    segmented_control,
    selectable_label_clicked,
    selectable_label_response,
    separator,
    spinner,
    set_max_height,
    set_min_height,
    set_min_width,
    set_width,
    small,
    small_button_clicked,
    strong,
    tag_tree_view,
    tags_edit,
    terminal,
    text_edit_multiline,
    text_edit_singleline,
    toggle_value,
    tooltip,
    top_panel,
    ui_contains_pointer,
    vertical,
    vertical_centered,
    weak,
)

BACKDROP = {"win32": Backdrop.ACRYLIC, "darwin": Backdrop.LIQUID_GLASS}.get(
    sys.platform
)
MACOS = sys.platform == "darwin"
# the id the taskbar groups the windows of the app by: scripts/make_shortcut_windows.py gives the
# same one to the shortcut, so a pinned Finder opens its window under its own icon
APP_USER_MODEL_ID = "pyegui.Finder"
APP_ICON = Path(__file__).resolve().parent / "app_icon" / "finder_256.png"
HOME = Path.home()
ICONS_DIR = (
    Path(__file__).resolve().parent / "icons"
)  # white SVG icons, tinted to the theme

EDGE = (
    1,
    (127, 127, 127, 60),
)  # the one outline of every box: the same width and color
GAP = 4  # between two boxes
H = GAP // 2  # each box gives half of the gap on the sides that face another box
TREE, LIST, ICONS = 0, 1, 2  # the order of the segmented control of the toolbar
LIGHT, DARK, SYSTEM = 0, 1, 2
NO_GLASS, PARTIAL, FULL = 0, 1, 2
ACCENTS = {
    "Blue": "#3b82f6",
    "Violet": "#8b5cf6",
    "Green": "#10b981",
    "Orange": "#f97316",
}

TEXT_SUFFIXES = {
    ".py",
    ".rs",
    ".md",
    ".txt",
    ".toml",
    ".rst",
    ".json",
    ".yml",
    ".yaml",
    ".cfg",
    ".lock",
    ".sh",
    ".js",
    ".css",
    ".html",
}
IMAGE_SUFFIXES = {".png", ".jpg", ".jpeg", ".gif", ".svg", ".webp"}
EMOJIS = {
    ".py": "🐍",
    ".rs": "🦀",
    ".md": "📝",
    ".txt": "📝",
    ".rst": "📝",
    ".pdf": "📕",
    ".png": "🖼",
    ".jpg": "🖼",
    ".jpeg": "🖼",
    ".gif": "🖼",
    ".svg": "🖼",
    ".webp": "🖼",
    ".zip": "🗜",
    ".gz": "🗜",
    ".toml": "⚙",
    ".json": "⚙",
    ".yml": "⚙",
    ".yaml": "⚙",
    ".lock": "🔒",
}
KINDS = {
    ".py": "Python script",
    ".rs": "Rust source",
    ".md": "Markdown",
    ".txt": "Plain text",
    ".rst": "reStructuredText",
    ".pdf": "PDF document",
    ".png": "PNG image",
    ".jpg": "JPEG image",
    ".jpeg": "JPEG image",
    ".svg": "SVG image",
    ".toml": "TOML",
    ".json": "JSON",
    ".lock": "Lock file",
    ".zip": "ZIP archive",
}
NEW_TAG_COLORS = [
    "#ef4444",
    "#f97316",
    "#22c55e",
    "#3b82f6",
    "#a855f7",
    "#eab308",
    "#14b8a6",
]

# state

tags = (
    TagStore()
)  # the tags and what they are put on, in a SQLite file, see finder_tags.py
tag_filter = Int(0)  # id of the tag whose items are shown instead of a folder, or 0
tag_manager_open = Bool(
    False
)  # the window to create, rename, recolor, move and delete tags
tag_picker_open = Bool(False)  # the window to put tags on the selected items
tag_edit = {
    "id": 0,
    "loaded": 0,
}  # the tag edited in the manager, and the one the fields hold
tag_name = Str("")
tag_color = RGB(0.0, 0.0, 0.0)  # channels between 0 and 1
tag_parent = Int(0)
tag_cache = {
    "version": -1,
    "own": {},
    "effective": {},
    "all": [],
    "by_id": {},
    "noted": set(),
}
tags_query = Str("")  # the search or creation field of the Tags pane
tag_query = Str("")  # what is typed in the window to tag the selection
tag_index = Int(0)  # the highlighted row of that window
tag_picker_state = {"query": "", "pointer": None, "just_opened": False}
find_name = Str("")  # the Recherche pane: part of the name,
find_ext = Str("")  # the extensions (py, md pdf),
find_scope = Int(0)  # 0: from the current folder, 1: everywhere in the index
find_folders = Bool(True)  # folders among the results
find_pages_min = Str("")  # filters on what pypdf read of the PDF: number of pages,
find_pages_max = Str("")
find_protect = Int(0)  # 0: all, 1: encrypted, 2: not encrypted
find_pdf_text = Str("")  # a text of the title, author, subject or keywords
find_auto = Bool(True)  # search by itself once the typing paused, otherwise with the button
find_state = {  # applied: the criteria of the rows shown; typed: those of the fields last frame
    "applied": None, "typed": None, "changed_at": 0.0, "rows": [], "more": False,
    "index": None, "pin": None, "job": 0, "running": False,
}
FIND_LIMIT = 100  # results kept: more is not readable in a pane, refine the criteria instead
FIND_DELAY = 0.3  # seconds without typing before an automatic search
watched = []  # the folders watched for the Activités pane (the watcher below keeps them watched)
activity = []  # what happened in them, newest last: {"time", "kind", "path", "root", "text"}
MAX_ACTIVITY = 2000
SAME_WRITE = 0.5  # seconds: the "modified" events of one write on the disk are told once
last_modified = {}  # path -> time of the last "modified" told
CREATED, MODIFIED, DELETED, RENAMED = "créé", "modifié", "supprimé", "renommé"
WATCH_COLORS = {
    CREATED: (34, 197, 94), MODIFIED: (234, 179, 8), DELETED: (239, 68, 68), RENAMED: (59, 130, 246),
}
WATCH_PANES = ("Dossiers surveillés", "Activités")  # closed until a folder is watched
indexing = {"running": False, "count": 0, "folder": "", "result": None}  # the folder index job
note_text = Str("")  # the text of the Notes pane, for the item it holds
note_of = {"path": ""}  # the item the Notes pane holds

start = Path(sys.argv[1]).expanduser().resolve() if len(sys.argv) > 1 else HOME
history = [start]
position = Int(0)
selected = Str("")  # the item the single-item commands act on: the last one clicked
picked = set()  # paths of all the selected items
anchor = Str("")  # where a range with Shift starts
scroll_target = Str("")  # path of the item the view has to scroll to this frame
view = Int(TREE)
search = Str("")
show_sidebar = Bool(True)
ZONES = ["sidebar", "content", "dock"]  # F6 goes through them, left to right
focus_zone = Str(
    "content"
)  # the part of the window the keyboard acts on, see keyboard()
show_dock = Bool(True)  # the dock around the files: Actions, Preview, Notes and Tags
folded = {"Outils", "Ouvrir", "Système", "Édition"}  # categories of the Actions panel folded shut
renaming = Str("")  # path being renamed, or ""
new_name = Str("")
palette_open = Bool(False)  # the action palette: a searchable list of the actions below
palette_query = Str("")
palette_index = Int(0)  # the highlighted action, moved by the keyboard or the mouse
file_clipboard = {"paths": [], "cut": False}
theme = Int(LIGHT)
glass = Int(FULL)
GLASS_OPACITY = 1.2  # multiplies the opacity of the glass
GLASS_LEVELS = {"central": 0.3, "dock": 0.3}  # the files and the docks are clear glass; the rest is the style's
BACKDROP_BLUR = 32  # the blur of what is behind the window (macOS), in points
# the unified layout, like the window of the Claude app: the sidebar runs along the whole height of
# the window, without gap nor corners; it and the body are cut in three bands of the same heights
# (top, content, bottom) whose lines continue from one to the other. False: rounded boxes with gaps
unified = Bool(True)
UNIFIED_GAP = 8  # the gap of the unified layout, around the dock and between its panes
TOP_H, BOTTOM_H = 52, 44  # the heights of the top and bottom bands, in the sidebar and the body
CLEAR = (0, 0, 0, 0)
LINE = (1, (127, 127, 127, 70))  # the lines between the bands and between the sidebar and the body
# the boxes layout: the margins outside the sidebar (left, top, right, bottom) and its corners
SIDEBAR_MARGIN = (0, 39, 0, 15.5)
SIDEBAR_ROUNDING = 14.7
accent_name = Str("Blue")
pill_tabs = Bool(False)
started = Bool(False)
context = []  # the Context, kept on the first frame

listing_cache = {}  # folder -> (version of the watcher, entries): read again when the disk changed

# the views of the files: state objects of pyegui, sharing one watcher of the disk
watcher = FileWatcher()
reading = Str("")  # a markdown or source file shown in the central view instead of the files, "" for none
reading_cache = {"path": None, "text": ""}
tree = FileTreeView("tree", start, watcher=watcher, activatable_dirs=True)
# navigate=False: a double click or Enter comes back in activated, the Finder opens it itself
files_list = FileListView("list", start, watcher=watcher, navigate=False)
files_icons = FileIconView("icons", start, watcher=watcher, navigate=False)
files_icons.icon_size = 56
tree_menu = {"paths": None}  # the items right-clicked in a view this frame


# files


def current():
    return history[position.value]


def human_size(size):
    for unit in ["bytes", "KB", "MB", "GB"]:
        if size < 1000 or unit == "GB":
            return f"{size:.0f} {unit}" if unit == "bytes" else f"{size:.1f} {unit}"
        size /= 1000


def entry(path):
    try:
        stat = path.stat()
    except OSError:
        return None
    is_dir = path.is_dir()
    suffix = path.suffix.lower()
    return {
        "path": path,
        "name": path.name or str(path),
        "is_dir": is_dir,
        "emoji": "📁" if is_dir else EMOJIS.get(suffix, "📄"),
        "kind": "Folder"
        if is_dir
        else KINDS.get(suffix, f"{suffix[1:].upper()} file" if suffix else "Document"),
        "size": "--" if is_dir else human_size(stat.st_size),
        "modified": datetime.fromtimestamp(stat.st_mtime).strftime("%d %b %Y at %H:%M"),
    }


def listing(folder):
    """the items of a folder, from the watcher: read again only when something changed on the disk"""
    cached = listing_cache.get(folder)
    if cached is None or cached[0] != watcher.version:
        entries = [
            e for e in (entry(Path(d["path"])) for d in watcher.list_dir(folder)) if e
        ]
        # folders first, then by name like the Finder
        listing_cache[folder] = (
            watcher.version,
            sorted(entries, key=lambda e: (not e["is_dir"], e["name"].lower())),
        )
    return listing_cache[folder][1]


def listing_here():
    """what the content shows: the folder, or the items of the tag chosen in the sidebar"""
    if not tag_filter.value:
        return listing(current())
    key = ("tag", tag_filter.value, tags.version, watcher.version)
    if key not in listing_cache:
        entries = [
            e for e in (entry(Path(p)) for p in tags.tagged_with(tag_filter.value)) if e
        ]
        for old in [k for k in listing_cache if isinstance(k, tuple)]:
            del listing_cache[old]  # the views of the tag in its former states
        listing_cache[key] = sorted(
            entries, key=lambda e: (not e["is_dir"], e["name"].lower())
        )
    return listing_cache[key]


def visible_entries():
    query = search.value.lower()
    return [e for e in listing_here() if query in e["name"].lower()]


def selected_entry():
    return entry(Path(selected.value)) if selected.value else None


def chosen():
    """the selected paths that still exist, in name order"""
    return sorted(
        (Path(p) for p in picked if Path(p).exists()), key=lambda p: p.name.lower()
    )


def deselect():
    picked.clear()
    selected.value = anchor.value = ""


def select_path(path):
    picked.clear()
    picked.add(str(path))
    selected.value = anchor.value = str(path)


def icon(item, size):
    """the icon of the Finder, or an emoji where the platform gives none"""
    uri = file_icon(str(item["path"]), size)
    if uri:
        image(uri, size=size)  # the room is kept while the icon loads
    else:
        # not selectable: inside icon_view's interactive() tile, a selectable label would
        # steal the click as a text selection instead of letting the tile's own click through
        label(item["emoji"], size=size * 0.8, selectable=False)


# navigation


def newest_python():
    """the most recent Python found: since 3.13 its prompt has colors, since 3.14 it colors the code"""
    for minor in range(20, sys.version_info.minor, -1):
        found = shutil.which(f"python3.{minor}")
        if found:
            return found
    return sys.executable


def nerd_font():
    """an installed Nerd Font, its Mono variant (icons one cell wide), for the icons of the prompt"""
    folders = [HOME / "Library" / "Fonts", Path("/Library/Fonts"), HOME / ".local" / "share" / "fonts"]
    if sys.platform == "win32":
        folders = [Path(os.environ.get("LOCALAPPDATA", "")) / "Microsoft" / "Windows" / "Fonts", Path(r"C:\Windows\Fonts")]
    for folder in folders:
        found = sorted(folder.glob("*NerdFontMono-Regular.[ot]tf")) if folder.is_dir() else []
        if found:
            return str(found[0])
    return None  # the monospace font of egui


# the colors of the terminals with the light theme and with the dark one (One Light, One Dark):
# the terminal takes those of the theme shown. Without them it is transparent, in the colors of egui
TERMINAL_LIGHT = dict(
    background="#fafafa",
    foreground="#383a42",
    cursor="#526fff",
    selection="#e5e5e6",
    palette=[
        "#383a42", "#e45649", "#50a14f", "#c18401", "#4078f2", "#a626a4", "#0184bc", "#a0a1a7",
        "#696c77", "#e45649", "#50a14f", "#c18401", "#4078f2", "#a626a4", "#0184bc", "#fafafa",
    ],
)
TERMINAL_DARK = dict(
    background="#282c34",
    foreground="#abb2bf",
    cursor="#528bff",
    selection="#3e4451",
    palette=[
        "#3f4451", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#d7dae0",
        "#4f5666", "#be5046", "#98c379", "#d19a66", "#61afef", "#c678dd", "#56b6c2", "#e6e6e6",
    ],
)

# the terminals of the dock at the bottom: states, created once, outside the update function.
# They start in the folder the Finder opens
shell = Terminal(cwd=str(start), font=nerd_font(), light=TERMINAL_LIGHT, dark=TERMINAL_DARK)
python = Terminal(
    cwd=str(start), shell=newest_python(), args=["-q"], font=nerd_font(), light=TERMINAL_LIGHT, dark=TERMINAL_DARK
)
follow_folder = Bool(True)  # the shell goes to the folder shown, with cd
shell_folder = {"path": start}  # where the shell was sent last


def cd_command(folder):
    if sys.platform == "win32":
        return f'cd /d "{folder}"'
    return f" cd {shlex.quote(str(folder))}"  # the leading space keeps it out of the history


def follow_in_shell():
    """the shell follows the folder of the Finder, while the option is on"""
    if follow_folder.value and current() != shell_folder["path"] and not tag_filter.value:
        shell_folder["path"] = current()
        shell.run(cd_command(current()))


def shell_pane():
    def bar():
        toggle_value(follow_folder, "Suivre le dossier")
        if small_button_clicked("cd ici"):
            shell.run(cd_command(current()))
        if small_button_clicked("Redémarrer"):
            shell.restart(cwd=str(current()))

    horizontal(bar)
    terminal(shell)


def python_pane():
    terminal(python)


def go(folder):
    folder = Path(folder)
    close_reader()  # moving to a folder shows its files
    tag_filter.value = 0  # leaving a tag view
    if folder == current() or not folder.is_dir():
        return
    del history[position.value + 1 :]
    history.append(folder)
    position.value = len(history) - 1
    deselect()
    search.value = ""
    tags.opened(folder)


def back():
    if reading.value:  # back from the viewer to the files first
        close_reader()
        return
    if position.value > 0:
        position.value -= 1
        deselect()


def up():
    """go to the parent folder and select the folder we come from, like the Finder"""
    child = current()
    go(child.parent)
    if current() != child:
        select_path(child)
        scroll_target.value = str(child)


def forward():
    if position.value < len(history) - 1:
        position.value += 1
        deselect()


def open_file(path):
    """opens a file with the application the system associates with it (not the browser:
    open_url() hands file:// URIs to the default browser on Windows)"""
    if sys.platform == "win32":
        os.startfile(path)
    elif MACOS:
        subprocess.Popen(["open", str(path)])
    else:
        subprocess.Popen(["xdg-open", str(path)])


def open_item(item):
    if item["is_dir"]:
        go(item["path"])
    else:
        try:
            open_file(item["path"])
        except OSError:
            return  # no application for this file, or it vanished
        tags.opened(item["path"])  # remembered in the base


def open_palette():
    palette_query.value = ""
    palette_index.value = 0
    palette_open.value = True


def popup_open():
    """a dialog is open: it has its own keyboard"""
    return bool(
        palette_open.value
        or renaming.value
        or tag_picker_open.value
        or tag_manager_open.value
    )


def cycle_zone(step):
    zones = [
        z
        for z in ZONES
        if (z != "sidebar" or show_sidebar.value) and (z != "dock" or show_dock.value)
    ]
    index = (
        zones.index(focus_zone.value)
        if focus_zone.value in zones
        else zones.index("content")
    )
    focus_zone.value = zones[(index + step) % len(zones)]


def sidebar_keys():
    """the keyboard of the sidebar: arrows go through the places and open them as they go, like the
    sidebar of the Finder; Enter or Right gives the focus to the content"""
    places = [path for _, _, path in favorites() + locations()]
    here = places.index(current()) if current() in places else -1
    if key_pressed("ArrowDown") and places:
        go(places[min(here + 1, len(places) - 1)])
    if key_pressed("ArrowUp") and places:
        go(places[max(here - 1, 0)])
    if key_pressed("Enter") or key_pressed("ArrowRight"):
        focus_zone.value = "content"


def keyboard():
    if popup_open():
        # those modals have their own Escape-closes-on-click-outside handling (see modal());
        # Escape uses when_typing=True below to work while a field has focus, so without this
        # guard it would also clear the background search/selection at the same time
        return
    if key_pressed("F6", when_typing=True):
        cycle_zone(1)
    if key_pressed("Shift+F6", when_typing=True):
        cycle_zone(-1)
    if shell.has_focus or python.has_focus:
        # the keys are for the shell: Escape (vim, less) must not send the focus back to the files
        return
    if key_pressed("Space"):
        open_palette()
    if key_pressed("Cmd+P"):
        open_palette()
    if key_pressed("Cmd+ArrowUp") or key_pressed("Backspace"):
        up()
    if key_pressed("Cmd+OpenBracket"):
        back()
    if key_pressed("Cmd+CloseBracket"):
        forward()
    if key_pressed("Escape", when_typing=True):
        close_reader()  # back to the files from the viewer
        search.value = ""
        deselect()
        focus_zone.value = "content"  # back to the list, even from the search field
    if focus_zone.value == "sidebar":
        sidebar_keys()
        return
    if focus_zone.value == "dock":
        return  # the panes of the dock have no keyboard of their own yet
    if key_pressed("Cmd+A"):
        picked.update(str(e["path"]) for e in visible_entries())
        selected.value = selected.value or next(iter(picked), "")
    # the arrows, Home, End, Enter and typing a name are handled by the view that has the focus


# style, as the View menu of app.py


def apply_style():
    ctx = context[0]
    if theme.value == LIGHT:
        ctx.set_light_theme()
    elif theme.value == DARK:
        ctx.set_dark_theme()
    else:
        ctx.set_system_theme()
    ctx.reset_style()
    if glass.value != NO_GLASS:
        ctx.set_glass_style(
            accent=ACCENTS[accent_name.value],
            mode="full" if glass.value == FULL else "partial",
            opacity=GLASS_OPACITY,
            **GLASS_LEVELS,
        )
    else:
        ctx.set_accent(ACCENTS[accent_name.value])
    dock.set_style(tab_style="pill" if pill_tabs.value else "classic")
    apply_gaps()
    ctx.set_backdrop_blur(BACKDROP_BLUR)  # macOS only, added to the blur of the backdrop


def apply_gaps():
    """the unified layout lays out the gaps itself: no margin in the central panel, and the gap of
    the dock between its panes; the boxes layout keeps the margin of egui and its own gap"""
    ctx = context[0]
    ctx.set_central_margin(0 if unified.value else 8)
    dock.set_style(gap_width=UNIFIED_GAP if unified.value else GAP)


def snapshot():
    """what the next run starts from: the choices of the user, not the selection"""
    return {
        "theme": theme.value,
        "glass": glass.value,
        "unified": unified.value,
        "accent": accent_name.value,
        "pill_tabs": pill_tabs.value,
        "view": view.value,
        "show_sidebar": show_sidebar.value,
        "show_dock": show_dock.value,
        "folded": sorted(folded),
        "tag_tree_folded": tag_tree.folded,
        "folder": str(current()),
        "tree_open": tree.open_folders,
        "dock": dock.save(),  # the panes still open, their tabs and sizes
        "watched": [str(p) for p in watched],
    }


def restore(saved):
    """the reverse of snapshot(); a value that is missing or no longer valid keeps its default"""
    if not isinstance(saved, dict):
        return
    theme.value = saved.get("theme", theme.value)
    glass.value = saved.get("glass", glass.value)
    unified.value = bool(saved.get("unified", unified.value))
    if saved.get("accent") in ACCENTS:
        accent_name.value = saved["accent"]
    pill_tabs.value = saved.get("pill_tabs", pill_tabs.value)
    view.value = saved.get("view", view.value)
    show_sidebar.value = saved.get("show_sidebar", show_sidebar.value)
    show_dock.value = saved.get("show_dock", show_dock.value)
    folded.update(saved.get("folded", []))
    tag_tree.folded = [p for p in saved.get("tag_tree_folded", []) if isinstance(p, str)]
    tree.open_folders = [p for p in saved.get("tree_open", []) if isinstance(p, str)]
    for folder in saved.get("watched", []):
        if isinstance(folder, str) and Path(folder).is_dir():
            watch_folder_path(Path(folder))
    layout = saved.get("dock")
    if layout:
        # a pane that no longer exists is left out; a layout that no longer fits is ignored
        dock.load(layout)
    folder = Path(saved.get("folder", ""))
    if len(sys.argv) < 2 and folder.is_dir():  # a folder given on the command line wins
        history[:] = [folder]


def set_theme(value):
    theme.value = value
    apply_style()


def set_glass(value):
    glass.value = value
    apply_style()


def set_accent_name(name):
    accent_name.value = name
    apply_style()


def set_tab_style(value):
    pill_tabs.value = value
    apply_style()


def set_view(value):
    view.value = value


# tags


def tag_data():
    """the tags, read again only when they changed: the content asks at every frame"""
    if tag_cache["version"] != tags.version:
        tag_cache.update(
            version=tags.version,
            own={},
            effective={},
            all=tags.all(),
            noted=tags.noted(),
        )
        tag_cache["by_id"] = {t["id"]: t for t in tag_cache["all"]}
    return tag_cache


def tags_of(path):
    """the tags of an item, as [(tag, how)]: own first, then those of its folders, then parents"""
    data = tag_data()
    key = str(path)
    if key not in data["effective"]:
        found = tags.effective(path)
        order = {"own": 0, "folder": 1, "parent": 2}
        data["effective"][key] = sorted(
            ((data["by_id"][t], how) for t, how in found.items() if t in data["by_id"]),
            key=lambda pair: (order[pair[1]], pair[0]["name"].lower()),
        )
    return data["effective"][key]


def dot(color, size=10):
    """a dot of a color, a tinted SVG so that it does not depend on the fonts"""
    image(svg("dot"), size=size, tint=color)


def tag_badges(path):
    """the colors of the tags of an item, drawn as dots by the views: pale when inherited"""
    return [
        hex_color(tag["color"]) + (255 if how == "own" else 110,)
        for tag, how in tags_of(path)
    ]


def notes_panel():
    """the note of the selected item: written as it is typed"""
    path = selected.value
    if not path or not Path(path).exists():
        weak("Sélectionne un fichier ou un dossier pour écrire une note")
        note_of["path"] = ""
        return
    if note_of["path"] != path:  # another item: the field takes its note
        note_of["path"] = path
        note_text.value = tags.note(path)
    strong(Path(path).name)
    weak(str(Path(path).parent))
    add_space(4)
    set_min_height(max(available_height() - 20, 80))
    text_edit_multiline(note_text, hint_text="Note")
    if note_text.value != tags.note(path):
        tags.set_note(path, note_text.value)


def hex_color(color):
    return tuple(int(color[i : i + 2], 16) for i in (1, 3, 5))


def to_hex(rgb):
    """the color of a color picker (RGB holds channels between 0 and 1) as "#rrggbb" """
    return "#{:02x}{:02x}{:02x}".format(
        *(min(max(round(c * 255), 0), 255) for c in (rgb.r, rgb.g, rgb.b))
    )


def tag_header():
    """above a tag view: which tag it is, and the way back to the folder"""
    tag = tag_data()["by_id"].get(tag_filter.value)
    if not tag_filter.value:
        return
    if not tag:  # the tag was deleted
        tag_filter.value = 0
        return

    def line():
        dot(hex_color(tag["color"]) + (255,))
        strong(tag["name"])
        weak(f"{len(listing_here())} élément(s) avec ce tag ou ses sous-tags")
        if image_button_clicked(
            svg("close"), size=12, tint=icon_tint()
        ):  # an icon: the glyph ✕ is missing in some fonts
            tag_filter.value = 0

    horizontal(line)
    separator()


def toggle_tags(paths, tag_id):
    """the tag is on all of the items → off them; otherwise → on all of them"""
    if all(tag_id in tags.own(p) for p in paths):
        for p in paths:
            tags.untag(p, tag_id)
    else:
        for p in paths:
            tags.tag(p, tag_id)


def tag_path(tag):
    """the names from the top level tag down to this one"""
    by_id = tag_data()["by_id"]
    names = []
    while tag in by_id:
        names.append(by_id[tag]["name"])
        tag = by_id[tag]["parent"]
    return names[::-1]


def split_tag_path(text):
    """ "Work/ Invoice" -> ["Work", "Invoice"]: the "/" separate the levels of the tree of tags"""
    return [part.strip() for part in text.split("/") if part.strip()]


def find_tag(parts):
    """the tag at this path from the top level, or None"""
    parent = None
    for part in parts:
        siblings = [
            t
            for t in tag_data()["all"]
            if t["parent"] == parent and t["name"].lower() == part.lower()
        ]
        if not siblings:
            return None
        parent = siblings[0]["id"]
    return parent


def create_tag_path(parts):
    """the tag at this path, made with the levels above it that do not exist yet"""
    parent = None
    for i, part in enumerate(parts):
        existing = find_tag(parts[: i + 1])
        if existing is None:
            color = NEW_TAG_COLORS[len(tag_data()["all"]) % len(NEW_TAG_COLORS)]
            existing = tags.create(part, color, parent)
        parent = existing
    return parent


# the tags of the store as Tags objects of pyegui: tag_tree_view in the sidebar, tags_edit in the
# preview. A tag is a path of names, "Work/Invoice"; the store keeps the ids, the colors and the items

library = Tags()  # every tag of the store, in its color
library_state = {"version": -1, "ids": {}, "counts": {}}  # path -> id, path -> items tagged
tag_tree = TagTreeView()  # the tags of the sidebar: the levels folded and the one selected
item_tags = Tags()  # the own tags of the item of the preview
item_tags_of = {"path": None, "version": -1}


def sync_library():
    """library, the ids of its paths and the counts, read again when the store changed"""
    if library_state["version"] == tags.version:
        return
    by_id = tag_data()["by_id"]
    ids = {"/".join(tag_path(t["id"])): t["id"] for t in tag_data()["all"]}
    library.tags = list(ids)
    library.colors = {path: by_id[i]["color"] for path, i in ids.items()}
    counts = {path: len(tags.tagged_with(i)) for path, i in ids.items()}
    library_state.update(version=tags.version, ids=ids, counts=counts)


def store_colors(widget):
    """the colors changed in a widget (its right click menu), written to the store"""
    sync_library()
    by_id = tag_data()["by_id"]
    levels = {"/".join(t.split("/")[: i + 1]) for t in widget.tags for i in range(t.count("/") + 1)}
    for path in levels:
        tag_id = library_state["ids"].get(path)
        if tag_id is not None and widget.color(path) != by_id[tag_id]["color"].lower():
            tags.recolor(tag_id, widget.color(path))


def tags_tree():
    """the tags of the sidebar: a click shows the items of a tag, a right click recolors it"""
    sync_library()
    paths = {i: path for path, i in library_state["ids"].items()}
    tag_tree.selected = paths.get(tag_filter.value)
    # removable=False: deleting a tag takes it off all its items, that is done in the tag manager
    clicked = tag_tree_view(tag_tree, library, counts=library_state["counts"], removable=False)
    if clicked:
        tag_filter.value = library_state["ids"][clicked]
        deselect()
    store_colors(library)


def edit_own_tags(path, **widget):
    """the own tags of an item in a tags_edit: Enter adds one (made with its levels when new), the
    cross removes it, the right click recolors a level; written to the store at once. The preview
    and the Tags cell of the selected row of the tree share it"""
    sync_library()
    ids = library_state["ids"]
    if item_tags_of["path"] != path or item_tags_of["version"] != tags.version:
        own = tags.own(path)
        item_tags.tags = [p for p, i in ids.items() if i in own]
        item_tags.colors = library.colors
        item_tags_of.update(path=path, version=tags.version)
    before = set(item_tags.tags)
    if tags_edit(item_tags, suggestions=list(ids), **widget):
        now = set(item_tags.tags)
        for added in now - before:
            tags.tag(path, create_tag_path(split_tag_path(added)))
        for removed in before - now:
            if removed in ids:
                tags.untag(path, ids[removed])
        store_colors(item_tags)


def item_tags_ui(path):
    """the tags of the item of the preview: its own ones to edit, then those of its folders"""
    edit_own_tags(path, hint_text="Ajouter un tag", field_width=140, id="preview")
    inherited = ["/".join(tag_path(tag["id"])) for tag, how in tags_of(path) if how == "folder"]
    if inherited:
        add_space(4)
        weak("Hérités des dossiers")
        tags_edit(Tags(inherited, colors=library.colors), editable=False, max_visible=6, muted=inherited, id="inherited")


TAGS_COLUMN = 200  # the width of the Tags column of the tree


def tags_cell(path):
    """the Tags column of the tree and of the list: on the selected row the own tags of the item to edit, on the
    others its tags and those of its folders, small chips"""
    if len(picked) == 1 and path == selected.value:
        # one chip, +N and the field fit in the column; +N shows them all
        edit_own_tags(path, small=True, max_visible=1, field_width=64, hint_text="+ tag", id=f"tree tags {path}")
        return
    sync_library()
    found = [("/".join(tag_path(tag["id"])), how) for tag, how in tags_of(path) if how != "parent"]
    if found:
        cell = Tags([p for p, _ in found], colors=library.colors)
        inherited = [p for p, how in found if how == "folder"]  # desaturated: they come from a folder
        tags_edit(cell, editable=False, small=True, max_visible=2, muted=inherited, id=f"tree tags {path}")


def matching_tags(parts):
    """the tags found by what was typed: each typed level is a part of the name of the matching
    level, the last typed one against the tag itself ("w/inv" finds Work/Invoice)"""
    found = []
    for tag in tag_data()["all"]:
        names = [n.lower() for n in tag_path(tag["id"])]
        if len(parts) <= len(names) and all(
            part.lower() in name
            for part, name in zip(parts, names[len(names) - len(parts) :])
        ):
            found.append(tag)
    return found


def can_create_tag(parts):
    """a tag is offered for creation when the typed path does not exist; a single name that
    already exists somewhere in the tree is found, not offered again"""
    if not parts or find_tag(parts) is not None:
        return False
    return len(parts) > 1 or not any(
        t["name"].lower() == parts[0].lower() for t in tag_data()["all"]
    )


def tag_picker_ui():
    paths = chosen()
    set_width(440)
    names = ", ".join(p.name for p in paths[:3]) + (" …" if len(paths) > 3 else "")
    weak(f"Taguer « {names} »")
    text_edit_singleline(
        tag_query, hint_text="Chercher ou créer un tag", request_focus=True
    )
    parts = split_tag_path(tag_query.value)
    rows = [("tag", t) for t in (matching_tags(parts) if parts else tag_data()["all"])]
    if can_create_tag(parts):
        rows.append(("create", parts))
    if (
        tag_query.value != tag_picker_state["query"]
    ):  # a new search starts at its first row
        tag_picker_state["query"] = tag_query.value
        tag_index.value = 0
    tag_index.value = min(tag_index.value, max(len(rows) - 1, 0))

    moved_by_key = False
    if key_pressed("ArrowDown", when_typing=True):
        tag_index.value = min(tag_index.value + 1, len(rows) - 1)
        moved_by_key = True
    if key_pressed("ArrowUp", when_typing=True):
        tag_index.value = max(tag_index.value - 1, 0)
        moved_by_key = True

    def activate(row):
        kind, value = row
        if kind == "create":
            leaf = create_tag_path(value)
            for p in paths:
                tags.tag(p, leaf)
            tag_query.value = ""
        else:
            toggle_tags(paths, value["id"])

    just_opened, tag_picker_state["just_opened"] = (
        tag_picker_state["just_opened"],
        False,
    )
    if rows and key_pressed("Enter", when_typing=True) and not just_opened:
        activate(rows[tag_index.value])

    add_space(4)
    owned = [tags.own(p) for p in paths]
    accent = ACCENTS[accent_name.value]
    row_width = (
        available_width() - 14
    )  # measured here, inside the scroll area it is unreliable

    def list_ui():
        for i, (kind, value) in enumerate(rows):

            def row(kind=kind, value=value):
                def line():
                    set_width(
                        row_width
                    )  # the highlight covers the whole row, not just the text
                    if kind == "create":
                        label(
                            f"+ Créer le tag « {' / '.join(value)} »", selectable=False
                        )
                        return
                    on = sum(value["id"] in o for o in owned)
                    names = tag_path(value["id"])
                    dot(hex_color(value["color"]) + (255,))
                    mark = "✓" if on == len(paths) else "–" if on else ""

                    def mark_column():
                        set_min_width(
                            12
                        )  # the names stay aligned whether or not they are checked
                        label(mark, selectable=False)

                    vertical(mark_column)
                    label(" / ".join(names), selectable=False)
                    if len(paths) == 1 and any(
                        t["id"] == value["id"] and how != "own"
                        for t, how in tags_of(paths[0])
                    ):
                        weak("hérité")

                horizontal(line)

            response = interactive(row)
            if i == tag_index.value:
                draw_rect(response.rect, fill=accent + "40", rounding=6)
                if moved_by_key:
                    scroll_to_rect(response.rect)
            elif response.hovered:
                draw_rect(response.rect, fill=(127, 127, 127, 30), rounding=6)
            if response.hovered and pointer_pos() != tag_picker_state["pointer"]:
                tag_index.value = i  # the mouse takes over the highlight when it moves
            if response.clicked:
                activate((kind, value))
        tag_picker_state["pointer"] = pointer_pos()

    if not rows:
        weak("Aucun tag : tape un nom pour en créer un")

    def bounded():
        # set_max_height measures from the top of its own ui, so it needs a fresh one here
        set_max_height(300)
        scroll_area_vertical(list_ui)

    vertical(bounded)
    add_space(6)

    def footer():
        if button_clicked("Gérer les tags…"):
            tag_picker_open.value = False
            tag_manager_open.value = True
        weak("Entrée : ajouter ou retirer · Parent/Enfant crée un chemin")

    horizontal(footer)


def dialog_fill():
    """the background of a dialog: nearly opaque, so what is behind does not show through the text"""
    return (30, 31, 36, 245) if context[0].is_dark_theme else (248, 248, 250, 245)


def tags_panel():
    """the dock pane of the tags: a field to search or create a tag, then the tree of tags, each
    with the state for the selected items: on (all of them have it), off (none) or mixed (some
    have it, they diverge)"""
    paths = chosen()
    data = tag_data()
    text_edit_singleline(tags_query, hint_text="Chercher ou créer un tag")
    parts = split_tag_path(tags_query.value)
    searching = bool(parts)
    shown = matching_tags(parts) if searching else data["all"]
    creating = can_create_tag(parts)
    if paths:
        weak(
            paths[0].name if len(paths) == 1 else f"{len(paths)} éléments sélectionnés"
        )
    else:
        weak("Sélectionne des éléments pour les taguer")
    add_space(4)

    def create():
        leaf = create_tag_path(parts)
        for p in paths:
            tags.tag(p, leaf)
        tags_query.value = ""

    if key_pressed("Enter", when_typing=True) and searching:
        if shown:
            if paths:
                toggle_tags(paths, shown[0]["id"])
        elif creating:
            create()

    owned = [tags.own(p) for p in paths]
    accent = ACCENTS[accent_name.value]
    tint = icon_tint()
    for tag in shown:
        on = sum(tag["id"] in o for o in owned)
        state = "off" if not paths or on == 0 else "on" if on == len(paths) else "mixed"
        inherited = len(paths) == 1 and any(
            t["id"] == tag["id"] and how != "own" for t, how in tags_of(paths[0])
        )

        def row(tag=tag, state=state, inherited=inherited):
            add_space(0 if searching else 14 * tag["depth"])
            clicked = image_button_clicked(
                svg(f"checkbox-{state}"),
                size=16,
                tint=tint if state == "off" else accent,
                frame=False,
            )
            if paths and clicked:
                toggle_tags(paths, tag["id"])
            dot(hex_color(tag["color"]) + (255,))
            name = " / ".join(tag_path(tag["id"])) if searching else tag["name"]
            if selectable_label_clicked(False, name) and paths:
                toggle_tags(paths, tag["id"])
            if inherited:
                weak("hérité")

        horizontal(row)
    if not shown and not creating:
        weak(
            "Aucun tag : tape un nom pour en créer un"
            if not searching
            else "Aucun tag trouvé"
        )
    if creating and button_clicked(f"+ Créer le tag « {' / '.join(parts)} »"):
        create()
    add_space(8)
    if button_clicked("Gérer les tags…"):
        tag_manager_open.value = True


def find_root():
    """the folder a search from the current folder covers. After a click on a result the current
    folder changes to the one of that result: the search keeps its root, or the results would
    shrink under the click"""
    pin = find_state["pin"]
    if pin and pin[0] == current():
        return pin[1]
    return current()


def find_params():
    """what the fields ask for now: (name, extensions, root folder or None, with folders, and the
    PDF criteria as (pages min, pages max, protected, text))"""
    root = find_root() if find_scope.value == 0 else None

    def number(text):
        return int(text) if text.strip().isdigit() else None

    pdf = (
        number(find_pages_min.value),
        number(find_pages_max.value),
        {0: None, 1: True, 2: False}[find_protect.value],
        find_pdf_text.value,
    )
    return (find_name.value, find_ext.value, root, find_folders.value, pdf)


def run_find(params):
    name, ext, root, folders, (pages_min, pages_max, protected, text) = params
    pdf = {"pages_min": pages_min, "pages_max": pages_max, "protected": protected, "text": text}
    find_state["job"] += 1
    job = find_state["job"]
    find_state.update(applied=params, index=find_index_key(), running=True)

    def work():
        try:
            rows = tags.find(name, ext, root, folders, limit=FIND_LIMIT + 1, pdf=pdf)
        except Exception:  # base locked or busy: shown as no result
            rows = []
        if find_state["job"] == job:  # a newer search replaced this one: its result is dropped
            find_state.update(rows=rows[:FIND_LIMIT], more=len(rows) > FIND_LIMIT, running=False)

    threading.Thread(target=work, daemon=True).start()


def find_index_key():
    return (indexing["running"], indexing.get("done_at"), tags.version)


def find_update():
    """asks the base when needed. Automatic: once the fields stopped changing for FIND_DELAY
    seconds (a letter typed restarts the wait). Manual: only from the button or Enter. Whatever
    the mode, a change of the index runs again the search already shown. Returns the pending state
    for the pane: True while the fields differ from the search shown."""
    now = time.monotonic()
    params = find_params()
    if params != find_state["typed"]:
        find_state.update(typed=params, changed_at=now)
    if find_state["applied"] is None:
        run_find(params)  # the first frame: everything, or what the folder holds
    elif find_auto.value:
        if params != find_state["applied"]:
            wait = find_state["changed_at"] + FIND_DELAY - now
            if wait <= 0:
                run_find(params)
            else:
                context[0].request_repaint()  # nothing else wakes the window for the timer
    elif key_pressed("Enter", when_typing=True) and params != find_state["applied"]:
        run_find(params)
    if find_state["index"] != find_index_key():
        run_find(find_state["applied"])
    return params != find_state["applied"]


def show_found(path, open_it):
    """goes to the folder of a result and selects it; a double click opens it"""
    path = Path(os.path.realpath(path))  # the base keeps paths in the case of the comparison
    item = entry(path)
    if not item:
        return  # it was deleted since the index
    root = find_root()
    if path.parent != current():
        go(path.parent)
    find_state["pin"] = (current(), root)
    select_path(path)
    scroll_target.value = str(path)
    if open_it:
        open_item(item)


def find_panel():
    """the dock pane of the advanced search in the SQLite index: by name and extension, from the
    current folder or everywhere, automatically (after a pause in the typing) or with a button"""
    text_edit_singleline(find_name, hint_text="Nom contient")
    text_edit_singleline(find_ext, hint_text="Extensions (py md pdf)")
    segmented_control(find_scope, [(None, "Dossier courant"), (None, "Partout")], tint=icon_tint())
    checkbox(find_folders, "Inclure les dossiers")
    add_space(2)
    weak("Filtres PDF (les PDF déjà indexés)")

    def pages_fields():
        for holder, hint in ((find_pages_min, "Pages min"), (find_pages_max, "Pages max")):
            set_width(80)
            text_edit_singleline(holder, hint_text=hint)

    horizontal(pages_fields)
    segmented_control(find_protect, [(None, "Tous"), (None, "Protégés"), (None, "Non protégés")], tint=icon_tint())
    text_edit_singleline(find_pdf_text, hint_text="Titre, auteur, sujet, mots-clés")
    add_space(2)
    checkbox(find_auto, "Recherche automatique")
    stale = find_update()
    if not find_auto.value:
        if button_clicked("Rechercher"):
            run_find(find_params())
            stale = False
    if not tags.indexed_count():
        weak("L'index est vide : lance « Indexer le dossier dans SQLite » (Outils).")
        return
    rows = find_state["rows"]
    count = f"{len(rows)}{'+' if find_state['more'] else ''} résultat{'s' if len(rows) != 1 else ''}"
    if find_state["running"]:
        context[0].request_repaint(after=0.1)  # the thread cannot wake the window

        def searching():
            spinner()
            weak("Recherche en cours…")

        horizontal(searching)
    elif index_status():
        weak(index_status())
    elif stale:
        weak(f"Recherche en cours… ({count})" if find_auto.value else f"{count} · critères modifiés")
    else:
        weak(count + (f" · seuls les {FIND_LIMIT} premiers sont affichés, affine la recherche" if find_state["more"] else ""))
    add_space(4)
    current_key = tag_key(selected.value) if selected.value else None
    for path, is_dir, size, modified, name, pages, encrypted in rows:
        def row(path=path, is_dir=is_dir, size=size, name=name, pages=pages, encrypted=encrypted):
            icon({"path": Path(path), "emoji": "📁" if is_dir else "📄"}, 16)
            response = selectable_label_response(path == current_key, name)
            if response.clicked:
                show_found(path, response.double_clicked)
            if not is_dir:
                weak(human_size(size))
            if pages:
                weak(f"{pages} p.")
            if encrypted:
                weak("protégé")

        horizontal(row)
        weak(str(Path(path).parent))


pane_requests = []  # titles of the panes to bring back, done at the start of the next frame


def show_pane(title):
    """brings a pane of the dock back if it was closed, on the right of the files. It is done at
    the start of the next frame: the layout of the dock cannot be read while one of its panes
    is being drawn"""
    pane_requests.append(title)


def holds_pane(layout, title):
    """True if the layout (as dock.save() gives it) has a pane of this title somewhere"""
    if not isinstance(layout, dict):
        return False
    return layout.get("pane") == title or any(holds_pane(c, title) for c in layout.get("children", []))


def add_pane_right(layout, title):
    """the layout (as dock.save() gives it) with a new pane in the column on the right of the files,
    under the panes already there; the column is made if there is none. Dock.add_pane would put it
    in the tabs of the files"""
    new = {"pane": title}
    horizontal = isinstance(layout, dict) and layout.get("kind") == "horizontal"
    children = layout.get("children") if horizontal else None  # else: wrapped below
    beside = [c for c in (children or []) if isinstance(c, dict) and not holds_pane(c, CENTRAL)]
    if beside:
        column = beside[-1]
        if "pane" in column:  # a single pane: it becomes a column with the new one
            index = children.index(column)
            column = children[index] = {
                "kind": "vertical", "shares": [1.0, 1.0], "children": [column],
            }
            column["children"].append(new)
            return layout
        shares = column.get("shares")
        column.setdefault("children", []).append(new)
        if shares:  # the new pane takes the share of an average one
            shares.append(sum(shares) / len(shares))
        if column.get("kind") == "tabs":
            column["active"] = len(column["children"]) - 1
        return layout
    column = {"kind": "vertical", "shares": [1.0], "children": [new]}
    return {"kind": "horizontal", "shares": [4.0, 1.0], "children": [layout, column]}


def open_requested_panes():
    for title in dict.fromkeys(pane_requests):  # once each, in order
        if title in dock.titles:
            dock.set_active(title)
            continue
        pane = DockPane(title, ALL_PANES[title], closable=True)
        if title in BOTTOM_PANES:  # the terminals live under the files
            dock.add_pane(pane)
            continue
        layout = dock.save()
        if not layout or not dock.load(add_pane_right(layout, title), panes=[pane]):
            dock.add_pane(pane)  # no layout to build on: the tabs of the files, as a last resort
    pane_requests.clear()


def watch_folder_path(path):
    """watches a folder and everything in it: what happens there goes to the Activités pane"""
    if path not in watched:
        watched.append(path)
        watcher.watch(path)  # kept watched even when no view shows it


def watch_folder():
    """watches the selected folder, the folder of the selected file, or the current folder"""
    watch_folder_path(target_directory())
    for title in WATCH_PANES:
        show_pane(title)


def unwatch_folder_path(path):
    if path in watched:
        watched.remove(path)
        watcher.unwatch(path)


def unwatch_folder():
    unwatch_folder_path(target_directory())


def relative_to(root, path):
    try:
        return str(Path(path).relative_to(root))
    except ValueError:
        return str(path)


def collect_activity():
    """moves the events the system sent into the log of the Activités pane (each frame). Only what
    happens inside a watched folder is kept; a write makes several events, told once"""
    events = watcher.take_events()
    if not watched:
        return
    now = time.monotonic()
    for event in events:
        raw = event["kind"]
        if raw.startswith("Create"):
            kind = CREATED
        elif raw.startswith("Remove"):
            kind = DELETED
        elif raw.startswith("Modify(Name"):
            kind = RENAMED
        elif raw.startswith("Modify"):
            kind = MODIFIED
        else:
            continue  # Access...: only reading
        paths = [str(p) for p in event["paths"]]
        for root in watched:
            inside = [p for p in paths if Path(p) == root or root in Path(p).parents]
            if not inside:
                continue
            if kind == MODIFIED:
                if Path(inside[0]).is_dir():
                    continue  # a folder changes with its content
                before = last_modified.get(inside[0])
                last_modified[inside[0]] = now
                if before is not None and now - before < SAME_WRITE:
                    continue
            if kind == RENAMED and len(paths) == 2:
                text = " → ".join(relative_to(root, p) for p in paths)
            else:
                text = ", ".join(relative_to(root, p) for p in inside)
            activity.append(
                {"time": event["time"], "kind": kind, "path": inside[-1], "root": str(root),
                 "text": text, "where": root}
            )
            break
    del activity[:-MAX_ACTIVITY]
    if len(last_modified) > 5000:
        last_modified.clear()


def activity_count(root):
    return sum(1 for e in activity if e["where"] == root)


def watched_panel():
    """the dock pane of the watched folders: click one to go there, the cross stops watching it"""
    if button_clicked("Surveiller le dossier courant"):
        watch_folder_path(current())
        show_pane("Activités")
    add_space(4)
    if not watched:
        weak("Aucun dossier surveillé : les changements de leur contenu seront listés dans Activités.")
    for path in list(watched):
        def row(path=path):
            icon({"path": path, "emoji": "📁"}, 16)
            if selectable_label_clicked(False, path.name or str(path)) and path.is_dir():
                go(path)
            changes = activity_count(path)
            if changes:
                weak(f"{changes} changement{'s' if changes != 1 else ''}")
            if image_button_clicked(svg("close"), size=12, tint=icon_tint()):
                unwatch_folder_path(path)

        horizontal(row)
        weak(str(path))


def activity_panel():
    """the console of the watched folders: one line per change, the newest first. A click on a
    line shows the item in its folder"""
    events = activity[-200:][::-1]

    def header():
        weak(f"{len(events)} événement{'s' if len(events) != 1 else ''}" if events else "Aucune activité")
        if events and small_button_clicked("Effacer"):
            activity.clear()

    horizontal(header)
    if not watched:
        weak("Aucun dossier surveillé : action Outils › Surveiller le dossier.")
    for event in events:
        def line(event=event):
            weak(time.strftime("%H:%M:%S", time.localtime(event["time"])))
            colored_label(WATCH_COLORS[event["kind"]], event["kind"])
            name = Path(event["root"]).name or event["root"]
            if selectable_label_clicked(False, f"[{name}] {event['text']}"):
                path = Path(event["path"])
                if path.exists():
                    show_found(path, False)
                elif path.parent.is_dir():
                    go(path.parent)

        horizontal(line)


def tag_picker():
    if not chosen():
        tag_picker_open.value = False
    elif modal(
        "tag-picker", tag_picker_ui, rounding=12, inner_margin=14, fill=dialog_fill()
    ):
        tag_picker_open.value = False


def new_tag(parent):
    color = NEW_TAG_COLORS[len(tag_data()["all"]) % len(NEW_TAG_COLORS)]
    tag_edit["id"] = tags.create("Nouveau tag", color, parent or None)
    return tag_edit["id"]


def tag_manager_ui():
    set_width(380)
    data = tag_data()
    strong("Tags")
    weak("Un tag s'applique à un élément et à son contenu, et ajoute ses tags parents.")
    add_space(4)

    def tree():
        for tag in data["all"]:

            def row(tag=tag):
                add_space(14 * tag["depth"])
                dot(hex_color(tag["color"]) + (255,))
                if selectable_label_clicked(tag_edit["id"] == tag["id"], tag["name"]):
                    tag_edit["id"] = tag["id"]

            horizontal(row)

    def bounded():
        # set_max_height measures from the top of its own ui, so it needs a fresh one here
        set_max_height(220)
        scroll_area_vertical(tree)

    vertical(bounded)
    add_space(4)
    separator()
    tag = data["by_id"].get(tag_edit["id"])
    if tag is None:
        tag_edit["id"] = 0
    elif (
        tag_edit["loaded"] != tag["id"]
    ):  # the fields take the values of the chosen tag
        tag_edit["loaded"] = tag["id"]
        tag_name.value = tag["name"]
        tag_color.r, tag_color.g, tag_color.b = (
            c / 255 for c in hex_color(tag["color"])
        )
        tag_parent.value = tag["parent"] or 0
    if tag:

        def edit():
            text_edit_singleline(tag_name, hint_text="Nom")
            color_edit_button_rgb(tag_color)

        horizontal(edit)
        under = [t for t in data["all"] if t["id"] not in tags.subtree(tag["id"])]
        combo_box(
            tag_parent,
            [0] + [t["id"] for t in under],
            ["(racine)"] + ["  " * t["depth"] + t["name"] for t in under],
            "Parent",
        )
        if tag_name.value.strip() and tag_name.value != tag["name"]:
            tags.rename(tag["id"], tag_name.value.strip())
        if to_hex(tag_color) != tag["color"]:
            tags.recolor(tag["id"], to_hex(tag_color))
        if (tag_parent.value or None) != tag["parent"]:
            tags.move(tag["id"], tag_parent.value or None)

    def buttons():
        if button_clicked("Nouveau tag"):
            new_tag(0)
        if tag and button_clicked("Nouveau sous-tag"):
            new_tag(tag["id"])
        if tag and button_clicked("Supprimer"):
            tags.delete(tag["id"])
            tag_edit["id"] = 0
            if tag_filter.value == tag["id"]:
                tag_filter.value = 0

    horizontal(buttons)


def tag_manager():
    if modal(
        "tag-manager", tag_manager_ui, rounding=12, inner_margin=14, fill=dialog_fill()
    ):
        tag_manager_open.value = False


def favorites():
    entries = [
        ("home", HOME.name, HOME),
        ("desktop", "Desktop", HOME / "Desktop"),
        ("document", "Documents", HOME / "Documents"),
        ("download", "Downloads", HOME / "Downloads"),
        ("folder", "pyegui", Path(__file__).resolve().parent.parent),
        ("folder", "Developpement", Path(r"C:\developpement")),
        ("folder", "Tech", Path(r"C:\Users\Joachim\tech")),
    ]
    return [e for e in entries if e[2].is_dir()]


def locations():
    return [("drive", place_name(drive), drive) for drive in drives() if drive.is_dir()]


def sidebar():
    if unified.value:
        # three bands, the top and bottom ones as high as the bars of the body; without lines in the
        # sidebar, only the body has them
        top_panel("sidebar_top", sidebar_top, exact_size=TOP_H, resizable=False, fill=CLEAR, inner_margin=(12, 0, 12, 0), show_separator_line=False)
        bottom_panel("sidebar_bottom", sidebar_bottom, exact_size=BOTTOM_H, resizable=False, fill=CLEAR, inner_margin=(12, 0, 12, 0), show_separator_line=False)
        frame(lambda: scroll_area_vertical(sidebar_content), inner_margin=12)
        return
    if MACOS:
        add_space(26)  # the window buttons are over the sidebar
    scroll_area_vertical(
        sidebar_content
    )  # many tags: the sidebar scrolls instead of overflowing


def settings_menu():
    """the menu of the settings button: theme, glass, accent, tabs and layout"""

    def choices(title, items, current_value, setter):
        small(title)
        for value, text in items:
            if selectable_label_clicked(current_value == value, text):
                setter(value)
                close_menu()

    choices("Thème", [(LIGHT, "Clair"), (DARK, "Sombre"), (SYSTEM, "Système")], theme.value, set_theme)
    separator()
    choices("Vitre", [(NO_GLASS, "Sans vitre"), (PARTIAL, "Partielle"), (FULL, "Complète")], glass.value, set_glass)
    separator()
    accents = {"Blue": "Bleu", "Violet": "Violet", "Green": "Vert", "Orange": "Orange"}
    choices("Accent", list(accents.items()), accent_name.value, set_accent_name)
    separator()
    choices("Onglets", [(False, "Classiques"), (True, "En pilule")], pill_tabs.value, set_tab_style)
    separator()
    before_layout = unified.value
    toggle_value(unified, "Disposition unifiée")
    if unified.value != before_layout:
        apply_gaps()


def sidebar_top():
    """the top band of the sidebar: the window buttons, then the settings at the right"""

    def row():
        add_space(max(available_width() - 28, 0))  # at the right of the band
        menu_image_button(svg("gear"), settings_menu, size=16, tint=icon_tint())

    horizontal_centered(row)


def sidebar_bottom():
    """the bottom band of the sidebar: the drive of the folder shown and its free space"""
    try:
        anchor = Path(current().anchor or "/")
        usage = shutil.disk_usage(anchor)
    except OSError:
        return

    def row():
        image(svg("drive"), size=16, tint=icon_tint())
        weak(f"{usage.free / 1e9:.0f} Go libres")
        add_space(max(available_width() - THEME_W, 4))
        # light, dark or the theme of the system, at the right like the settings of the Claude app
        theme_control()

    horizontal_centered(row)


def theme_control():
    """light, dark, system: a segmented control of icons"""
    if segmented_control(
        theme,
        [(svg("sun"), None), (svg("moon"), None), (svg("theme-system"), None)],
        size=14,
        tint=icon_tint(),
    ):
        apply_style()

def sidebar_content():
    accent = ACCENTS[accent_name.value]

    def place(icon_name, name, path):
        # SVG icons in the accent color, like the sidebar of the Finder; white when selected
        path = Path(path)
        if not path.is_dir():
            return
        on = current() == path
        if image_button_clicked(
            svg(icon_name),
            name,
            size=16,
            tint=(255, 255, 255) if on else accent,
            selected=on,
            frame=False,
        ):
            go(path)

    weak("Favorites")
    for icon_name, name, path in favorites():
        place(icon_name, name, path)

    add_space(12)
    weak("Locations")
    for icon_name, name, path in locations():
        place(icon_name, name, path)
    add_space(12)

    weak("Tags")  # created in the preview, the Tags pane and the tag picker ("Gérer les tags…")
    tags_tree()


# toolbar


def svg(name):
    return (ICONS_DIR / f"{name}.svg").as_uri()


def toggle_tint(on):
    """the icon of a toggle button: white on the accent color when on, like a selected item"""
    return (255, 255, 255) if on else icon_tint()


def icon_tint():
    """the color of the icons: light on the dark theme, dark on the light theme"""
    return (225, 227, 233) if context[0].is_dark_theme else (70, 73, 82)


def place_name(path):
    return path.name or ("Macintosh HD" if MACOS else path.drive)


def drives():
    """the mounted drives, for the Locations section of the sidebar: one on macOS, every
    connected drive letter on Windows (local disks and mapped network drives alike)"""
    if MACOS:
        return [Path("/")]
    import ctypes
    import string

    mask = ctypes.windll.kernel32.GetLogicalDrives()
    return [
        Path(f"{letter}:/")
        for i, letter in enumerate(string.ascii_uppercase)
        if mask & (1 << i)
    ]


# what the right side of the bars takes: measured as drawn
widths = {
    "view": 130,
    "panes": 60,
    "count": 110,
    "dock": 50,
}
THEME_W = 116  # the width of the theme control: three icons in a segmented control


def measure(key, draw, last=False):
    """draws something in a row and remembers the width it took, with its spacing. The room left in a
    row is never negative, so a width is only learned when there was room: if the thing did not fit,
    it is given a little more room at the next frame"""
    before = available_width()
    draw()
    if last:
        add_space(-8)  # takes back the spacing a row adds after its last thing
    after = available_width()
    taken = before - after if after > 0.5 else before + 8
    if abs(taken - widths[key]) > 0.5:
        widths[key] = taken
        context[0].request_repaint()  # the next frame is laid out with the new width


SLACK = 1  # room kept in a row, to see that what is at its right grew
SEARCH_WIDTH, SEARCH_MIN = (
    240,
    90,
)  # the search field: its widest, and the least worth showing


def panes_menu():
    """the panes of the dock that were closed with their tab, to bring them back"""
    closed = [title for title in ALL_PANES if title not in dock.titles]
    if not closed:
        weak("Tous les panneaux sont affichés")
    for title in closed:
        if button_clicked(f"Afficher {title}"):
            show_pane(title)
    separator()
    toggle_value(show_dock, "Dock")


def toolbar():
    tint = icon_tint()

    def bar():
        if MACOS and not show_sidebar.value:
            add_space(
                64
            )  # the window buttons are over the toolbar when the sidebar is hidden
        if image_button_clicked(svg("chevron-left"), size=16, tint=tint):
            back()
        if image_button_clicked(svg("chevron-right"), size=16, tint=tint):
            forward()

        def parent_button():
            if image_button_clicked(svg("chevron-up"), size=16, tint=tint):
                go(current().parent)

        add_enabled(
            current().parent != current(), parent_button
        )  # greyed out at the root
        add_space(6)
        # the name of the folder; under the mouse its icon and a menu of its parent folders
        chosen_folder = path_combo(current(), size=15, context_menu=path_menu)
        if chosen_folder:
            go(chosen_folder)
        # the right side: the view, the search, the hidden panes. The search shrinks with the window and
        # disappears when it would be too small. What each part takes is what it took the last time it
        # was drawn, so the right edge does not depend on fonts or on the length of a name
        left_over = available_width()
        view_w, panes_w = widths["view"], widths["panes"]
        search_width = min(left_over - view_w - panes_w - 8 - SLACK, SEARCH_WIDTH)
        show_search = search_width >= SEARCH_MIN
        show_view = left_over >= view_w + panes_w
        used = (
            panes_w
            + (view_w if show_view else 0)
            + (search_width + 8 if show_search else 0)
        )
        add_space(max(left_over - used - SLACK, 4))
        if show_view:
            measure(
                "view",
                lambda: segmented_control(
                    view,
                    [(svg("tree"), None), (svg("list"), None), (svg("grid"), None)],
                    size=16,
                    tint=tint,
                ),
            )
        if show_search:

            def search_field():
                set_width(search_width)
                text_edit_singleline(search, hint_text="Search")

            # in the unified layout the field is centered in the band like the other buttons
            (horizontal_centered if unified.value else vertical)(search_field)
        measure(
            "panes",
            lambda: menu_image_button(svg("panes"), panes_menu, size=18, tint=tint),
            last=True,
        )

    (horizontal_centered if unified.value else horizontal)(bar)


# views


def view_menu(paths):
    """a right click in a view opens the actions panel, like the Space key"""
    close_menu()  # the panel replaces the menu
    tree_menu["paths"] = paths
    open_palette()


# when the items were last opened: the files opened with their app or in the viewer, the folders
# entered. Kept in the base of the tags (finder_tags.py), read again when it changed

opened_cache = {"version": -1, "dates": {}, "recent": []}
RECENT_LIMIT = 30  # the items of the Recent pane
OPENED_COLUMN = 112  # the width of the Last Opened column


def opened_data():
    if opened_cache["version"] != tags.version:
        opened_cache.update(
            version=tags.version,
            dates=tags.opened_dates(),
            recent=tags.recently_opened(RECENT_LIMIT),
        )
    return opened_cache


def short_date(when):
    """a date that fits a narrow column: the hour today, Yesterday, the day this year, else the year"""
    moment, today = datetime.fromtimestamp(when), datetime.now().date()
    if moment.date() == today:
        return moment.strftime("%H:%M")
    if (today - moment.date()).days == 1:
        return "Yesterday"
    if moment.year == today.year:
        return moment.strftime("%d %b")
    return moment.strftime("%d %b %Y")


def opened_cell(path):
    """the Last Opened column of the list and of the tree"""
    when = opened_data()["dates"].get(tag_key(Path(path)))
    if when:
        weak(short_date(when))


def recent_panel():
    """the Recent pane: the items last opened, the latest first. A click shows the item in its
    folder (or enters the folder), the mouse over it gives its path"""
    items = [(Path(p), when) for p, when in opened_data()["recent"] if Path(p).exists()]
    if not items:
        weak("Les fichiers et dossiers ouverts s'afficheront ici")
        return
    for path, when in items:

        def row(path=path, when=when):
            uri = file_icon(str(path), 20)
            if uri:
                image(uri, size=20)

            def text():
                # the name, then when and where under it: a narrow pane cuts nothing important
                if selectable_label_clicked(selected.value == str(path), path.name or str(path)):
                    go_or_select(path)
                small(f"{short_date(when)} · {path.parent.name or path.parent}")

            vertical(text)

        tooltip(str(path).replace(str(HOME), "~"), lambda row=row: horizontal(row))


def note_cell(path):
    """the Notes column of the list: an icon when the item has a note"""
    if tag_key(Path(path)) in tag_data()["noted"]:
        image(svg("note"), size=14, tint=icon_tint())


def file_view():
    return {TREE: tree, LIST: files_list, ICONS: files_icons}[view.value]


def before_view(v):
    """gives the view what the Finder decided since the last frame: the folder, the items of a
    tag, the search, the selection, the scroll and the keyboard focus"""
    if view.value == TREE:
        if v.root != str(current()):
            v.root = current()
    elif v.folder != str(current()):
        v.go(current())
    v.paths = [str(e["path"]) for e in listing_here()] if tag_filter.value else None
    v.filter = search.value
    if set(v.selected) != picked:
        v.selected = sorted(
            picked, key=lambda p: p == selected.value
        )  # the last clicked last
    if scroll_target.value:
        v.scroll_to(scroll_target.value)
    # the view keeps the focus while the content is the zone of the keyboard: a click on a button
    # must not cut the arrows; a text field being edited and a dialog keep it
    v.keep_focus = focus_zone.value == "content" and not popup_open()
    if focus_zone.value != "content":
        v.focus(False)


def after_view(v):
    """what the user did in the view: the selection, the items opened"""
    paths, tree_menu["paths"] = tree_menu["paths"], None
    if paths and not set(paths) <= set(v.selected):
        v.selected = (
            paths  # a right click outside the selection acts on the item clicked
        )
    now = set(v.selected)
    if now != picked:
        picked.clear()
        picked.update(now)
        selected.value = anchor.value = v.selected[-1] if v.selected else ""
    opened = [p for p in (entry(Path(p)) for p in v.activated) if p]
    if len(opened) == 1 and opened[0]["is_dir"]:
        open_item(opened[0])  # into the folder
    elif len(opened) == 1 and (is_markdown(opened[0]["path"]) or is_code(opened[0]["path"])):
        open_reader(opened[0]["path"])  # shown here, in the central view
    elif opened:
        action_open()  # the files with their app, like the Open button


def seed_tree_selection():
    """the tree view moves from the selected node, so with none (at the start, after entering a
    folder, after Escape) the first arrow selects the first or the last item, like the other views.
    The tree is left without focus for that frame, or it would also move by one"""
    if picked or focus_zone.value != "content" or popup_open():
        return
    down, up = key_pressed("ArrowDown"), key_pressed("ArrowUp")
    entries = visible_entries()
    if (down or up) and entries:
        item = entries[0 if down else -1]
        select_path(item["path"])
        scroll_target.value = str(item["path"])
        tree.focus(False)


def is_markdown(path):
    return Path(path).suffix.lower() in (".md", ".markdown")


# the source files shown by the viewer of the central view, with their colors (code_view)
CODE_EXTENSIONS = {".py", ".pyw", ".js", ".mjs", ".cjs", ".ts", ".rs", ".ps1", ".psm1", ".sh", ".bash", ".zsh"}
READER_MAX = 2_000_000  # a larger file opens with its app


def is_code(path):
    return Path(path).suffix.lower() in CODE_EXTENSIONS


def open_reader(path):
    """shows a markdown or source file in the central view, read again when an editor saves it"""
    try:
        if Path(path).stat().st_size > READER_MAX:
            context[0].open_url(Path(path).as_uri())
            return
    except OSError:
        return
    close_reader()
    reading.value = str(path)
    watcher.watch(reading.value)
    watcher.changed(reading.value)  # starts counting the changes
    reading_cache["path"] = None
    tags.opened(Path(path))  # remembered in the base, like a file opened with its app


def close_reader():
    if reading.value:
        watcher.unwatch(reading.value)
        reading.value = ""


def reader_text():
    path = reading.value
    if reading_cache["path"] != path or watcher.changed(path):
        try:
            reading_cache["text"] = Path(path).read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError) as error:
            reading_cache["text"] = f"*Cannot read this file: {error}*"
        reading_cache["path"] = path
    return reading_cache["text"]


def follow_link(url):
    """a link of the markdown viewer: a markdown file opens here, the web in the browser, another
    file with its app"""
    parts = urlsplit(url)
    if parts.scheme in ("http", "https", "mailto"):
        context[0].open_url(url)
        return
    if not parts.path:
        return  # an anchor in the same file: the viewer shows the whole text
    target = Path(unquote(parts.path)) if parts.scheme == "file" else (Path(reading.value).parent / unquote(parts.path)).resolve()
    if target.is_file() and is_markdown(target):
        open_reader(target)
    elif target.exists():
        context[0].open_url(target.as_uri())


def reader():
    """the viewer of the central view: a bar to go back to the files, then the markdown or the code"""
    tint = icon_tint()

    def bar():
        if image_button_clicked(svg("chevron-left"), "Fermer", size=16, tint=tint):
            close_reader()
            return
        add_space(6)
        strong(Path(reading.value).name)
        add_space(max(available_width() - 150, 4))
        if small_button_clicked("Ouvrir avec l'app"):
            context[0].open_url(Path(reading.value).as_uri())

    horizontal(bar)  # a row at the top: horizontal_centered would take the whole height
    if not reading.value:
        return  # closed by the bar
    separator()
    if is_code(reading.value):
        # the colors of the language, from the extension (.ps1 is shown with those of the shell)
        code_view(reader_text(), Path(reading.value).suffix, id=f"reader {reading.value}")
        return
    url = markdown(reader_text(), scrollable=True, id=f"reader {reading.value}", open_links=False)
    if url:
        follow_link(url)


def content():
    if reading.value:
        reader()
        return
    tag_header()
    if not listing_here() or (search.value and not visible_entries()):
        weak("No items" if not search.value else f"No items match “{search.value}”")
        return
    if view.value == TREE:
        seed_tree_selection()
    v = file_view()
    before_view(v)
    if view.value == TREE:
        scroll_area_both(
            lambda: file_tree_view(
                tree,
                extra_columns=[("Tags", TAGS_COLUMN, tags_cell), ("Last Opened", OPENED_COLUMN, opened_cell)],
                context_menu=view_menu,
            )
        )
    elif view.value == LIST:
        file_list_view(
            files_list,
            headers=["Name", "Date Modified", "Size", "Kind"],
            # columns drawn by the Finder: the notes, and the tags as in the tree (editable on the
            # selected row)
            extra_columns=[
                ("Notes", 50, note_cell),
                ("Pages", 55, pages_cell),
                ("Prot.", 45, protected_cell),
                ("Tags", TAGS_COLUMN, tags_cell),
                ("Last Opened", OPENED_COLUMN, opened_cell),
            ],
            context_menu=view_menu,
        )
    else:
        file_icon_view(files_icons, badges=tag_badges, context_menu=view_menu)
    after_view(v)
    scroll_target.value = ""  # scrolled once, the user can scroll away


# preview


def preview():
    item = selected_entry()
    if item is None:
        weak("Select an item to see its preview")
        return
    if len(picked) > 1:
        paths = chosen()
        total = sum(p.stat().st_size for p in paths if p.is_file())
        strong(f"{len(paths)} items selected")
        weak(f"{human_size(total)} in files")
        for path in paths[:12]:
            label(path.name)
        if len(paths) > 12:
            weak(f"and {len(paths) - 12} more")
        return

    def head():
        icon(item, 96)
        strong(item["name"])
        weak(f"{item['kind']} · {item['size']}")

    vertical_centered(head)
    add_space(8)
    separator()

    def rows():
        for name, value in [
            ("Kind", item["kind"]),
            ("Size", item["size"]),
            ("Modified", item["modified"]),
            ("Where", str(item["path"].parent).replace(str(HOME), "~")),
        ]:
            weak(name)
            label(value)
            end_row()

    grid("info", rows, num_columns=2, spacing=(12, 6))
    add_space(8)
    small("Tags")
    item_tags_ui(item["path"])
    add_space(8)

    suffix = item["path"].suffix.lower()
    if suffix == ".pdf":

        def pdf_grid():
            for name, value in pdf_rows(item["path"]):
                weak(name)
                label(value)
                end_row()

        grid("pdf-info", pdf_grid, num_columns=2, spacing=(12, 6))
        add_space(8)
    elif suffix in IMAGE_SUFFIXES:
        image(item["path"].as_uri(), max_width=260, max_height=220)
    elif suffix in TEXT_SUFFIXES:
        try:
            text = item["path"].read_text(errors="replace")[:1200]
        except OSError:
            text = ""
        if text:
            code("\n".join(text.splitlines()[:24]))
    elif item["is_dir"]:
        count = len(listing(item["path"]))
        label(f"{count} item{'s' if count != 1 else ''}")
    if button_clicked("Open"):
        open_item(item)


# actions: one registry read by the panel and by the keyboard, like the Actions dock of pyfinder


def targets():
    """the selected items, or the current folder when nothing is selected"""
    return chosen() or [current()]


def unique(path):
    """path, or "name copy", "name copy 2"... when it exists"""
    candidate, n = path, 1
    while candidate.exists():
        suffix = " copy" if n == 1 else f" copy {n}"
        candidate = path.with_name(f"{path.stem}{suffix}{path.suffix}")
        n += 1
    return candidate


def refresh(*folders):
    for folder in folders:
        listing_cache.pop(folder, None)


def copy_to_clipboard(cut):
    file_clipboard.update(paths=chosen(), cut=cut)


def paste():
    pasted = []
    for source in file_clipboard["paths"]:
        if not source.exists():
            continue
        target = unique(current() / source.name)
        if file_clipboard["cut"]:
            shutil.move(source, target)
            refresh(source.parent)
        elif source.is_dir():
            shutil.copytree(source, target)
        else:
            shutil.copy2(source, target)
        pasted.append(target)
    file_clipboard["paths"] = [] if file_clipboard["cut"] else file_clipboard["paths"]
    refresh(current())
    if pasted:
        picked.clear()
        picked.update(map(str, pasted))
        selected.value = anchor.value = str(pasted[-1])


def duplicate():
    copies = []
    for source in chosen():
        target = unique(source)
        shutil.copytree(source, target) if source.is_dir() else shutil.copy2(
            source, target
        )
        copies.append(target)
    refresh(current())
    picked.clear()
    picked.update(map(str, copies))
    selected.value = anchor.value = str(copies[-1])


def start_rename(path):
    renaming.value, new_name.value = str(path), path.name


def new_folder():
    target = unique(current() / "untitled folder")
    target.mkdir()
    refresh(current())
    select_path(target)
    start_rename(target)


def trash():
    # the Finder's own trash, so the items can be put back
    for path in chosen():
        script = f'tell application "Finder" to delete POSIX file "{path}"'
        subprocess.run(["osascript", "-e", script], capture_output=True)
    refresh(current())
    deselect()


def reveal():
    path = targets()[0]
    if MACOS:
        subprocess.run(["open", "-R", str(path)])
    elif sys.platform == "win32":
        # open_url() hands file:// URIs to the default browser on Windows, not Explorer
        if path.is_dir():
            subprocess.run(["explorer", str(path)])
        else:
            # /select, and the path must be a single command line, not a list of argv:
            # Explorer only understands the quoted path right after the comma
            subprocess.run(f'explorer /select,"{path}"')
    else:
        context[0].open_url((path if path.is_dir() else path.parent).as_uri())


def group():
    """a new folder holding the selected items, ready to be named"""
    paths = chosen()
    folder = unique(paths[0].parent / "New Folder With Items")
    folder.mkdir()
    for path in paths:
        shutil.move(path, folder / path.name)
    refresh(folder.parent)
    select_path(folder)
    start_rename(folder)


def can_group():
    paths = chosen()
    return (
        bool(paths)
        and len({p.parent for p in paths}) == 1
        and paths[0].parent == current()
    )


def show_parent():
    path = Path(selected.value)
    go(path.parent)
    select_path(path)


def action_open():
    items = [entry(p) for p in chosen()]
    items = [item for item in items if item]
    folders = [item for item in items if item["is_dir"]]
    for item in items:
        if not item["is_dir"]:
            open_item(item)
    if folders and len(items) == len(folders):
        open_item(folders[0])  # a folder opens in place, only one at a time


def app_installed(name):
    """a macOS application in /Applications or ~/Applications"""
    return MACOS and any(
        (Path(base) / f"{name}.app").exists()
        for base in ("/Applications", HOME / "Applications")
    )


def zed_command():
    """the command that opens files in Zed, or None when it is not installed"""
    if app_installed("Zed"):
        return ["open", "-a", "Zed"]  # never blocks, the CLI is often not installed
    return next(
        ([found] for found in map(shutil.which, ("zed", "zeditor")) if found), None
    )


def target_directory():
    """the selected folder, the folder of the selected file, or the current folder"""
    path = targets()[0]
    return path if path.is_dir() else path.parent


def open_in_zed():
    command = zed_command()
    if command:
        subprocess.Popen(
            [*command, *map(str, targets())]
        )  # without selection: the current folder


def open_in_claude(mode):
    """Claude Desktop opens the folder in Code or Cowork mode, and asks to confirm it"""
    context[0].open_url(
        f"claude://{mode}/new?folder={quote(str(target_directory()), safe='')}"
    )


def open_tag_picker():
    tag_query.value = ""
    tag_index.value = 0
    tag_picker_state["just_opened"] = (
        True  # the Enter that chose the action must not tag a row
    )
    tag_picker_open.value = True


def has_selection():
    return bool(chosen())


def one_selected():
    return len(chosen()) == 1


def index_folder_action():
    """records the folder (the selected one, or the current one) and all it contains in the SQLite
    base, in a thread so the window stays alive; the progress shows in the path bar"""
    folder = target_directory()
    start_index_job(folder, folder.name or str(folder), walk=True)


def read_pdfs_action():
    """reads with pypdf the PDF already in the index that were never read (indexed before the PDF
    were read, or new), or changed since: no new walk of the disk, for the whole index"""
    start_index_job(None, "l'index", walk=False)


def start_index_job(folder, name, walk):
    """the thread of an indexation: the walk of a folder (if walk), then the reading of its PDF"""
    if indexing["running"]:
        return
    indexing.update(
        running=True, count=0, folder=name, result=None, pdf=None, pdfs=None, walk=walk
    )

    def progress(count):
        indexing["count"] = count

    def pdf_progress(done, total):
        indexing["pdf"] = (done, total)

    def work():
        try:
            indexing["result"] = tags.index_folder(folder, progress) if walk else (0, 0)
            try:  # then what the PDF say of themselves: only the new or changed ones are read
                indexing["pdfs"] = tags.index_pdfs(folder, pdf_progress)
            except ImportError:
                indexing["pdfs"] = "pypdf"  # not installed: the files are indexed all the same
        except Exception as error:  # a base that is locked, a disk that is full...
            indexing["result"] = error
        indexing["done_at"] = time.time()
        indexing["running"] = False

    threading.Thread(target=work, daemon=True).start()


def index_status():
    """the text of the index job for the path bar, or "" when there is nothing to tell"""
    if indexing["running"]:
        if indexing.get("pdf"):
            return f"Lecture des PDF de {indexing['folder']}… {indexing['pdf'][0]}/{indexing['pdf'][1]}"
        return f"Indexation de {indexing['folder']}… {indexing['count']}"
    result = indexing["result"]
    if time.time() - indexing.get("done_at", 0) > 6:
        return ""  # the message of the end stays a few seconds
    if isinstance(result, Exception):
        return f"Indexation impossible : {result}"
    if result:
        pdfs = indexing.get("pdfs")
        if not indexing.get("walk"):  # only the PDF of the index were read
            if pdfs == "pypdf":
                return "PDF non lus : pypdf n'est pas installé (pip install pypdf)"
            read, bad = (pdfs[0], pdfs[1]) if pdfs else (0, 0)
            if not read:
                return "Tous les PDF de l'index sont déjà lus"
            return f"{read} PDF lus" + (f" dont {bad} illisibles" if bad else "")
        text = f"{indexing['folder']} indexé : {result[0]} éléments"
        if pdfs == "pypdf":
            text += " (PDF non lus : pip install pypdf)"
        elif pdfs and pdfs[0]:
            text += f", {pdfs[0]} PDF lus" + (f" dont {pdfs[1]} illisibles" if pdfs[1] else "")
        return text
    return ""


pdf_columns = {"key": None, "info": {}}


def pdf_cell_info(path):
    """(pages, protected) of a PDF of the folder shown, from the index; the folder is asked once"""
    folder = Path(path).parent
    key_ = (folder, indexing.get("done_at"), watcher.version)
    if pdf_columns["key"] != key_:
        pdfs = [str(e["path"]) for e in listing(folder) if not e["is_dir"] and e["name"].lower().endswith(".pdf")]
        pdf_columns.update(key=key_, info=tags.pdf_summary(pdfs) if pdfs else {})
    return pdf_columns["info"].get(str(path), (None, False))


def pages_cell(path):
    """the Pages column of the list: the number of pages of a PDF read in the index"""
    pages = pdf_cell_info(path)[0]
    if pages is not None:
        weak(str(pages))


def protected_cell(path):
    """the Prot. column of the list: a lock when the PDF asks a password or forbids a right"""
    if pdf_cell_info(path)[1]:
        label("🔒")


pdf_cache = {"key": None, "info": None}


def pdf_details(path):
    """what pypdf read of a PDF: from the index, or (not indexed yet) from the file itself, once.
    Cached: the preview asks at every frame"""
    try:
        stamp = path.stat().st_mtime_ns
    except OSError:
        stamp = None
    key_ = (str(path), stamp, indexing.get("done_at"))
    if pdf_cache["key"] != key_:
        info = tags.pdf_info(path)
        if info is None:
            try:
                from finder_pdf import read_pdf_info

                info = read_pdf_info(path)
            except ImportError:
                info = {"error": "pypdf n'est pas installé (pip install pypdf)"}
        pdf_cache.update(key=key_, info=info)
    return pdf_cache["info"]


def pdf_rows(path):
    """the lines of the preview for a PDF: (name, value), what is known only"""
    info = pdf_details(path)
    if info.get("error") and info.get("pages") is None and not info.get("encrypted"):
        return [("PDF", f"illisible : {info['error']}")]
    from finder_pdf import restrictions

    def day(value):
        return datetime.fromtimestamp(value).strftime("%d %b %Y") if value else None

    if info.get("needs_password"):
        protection = "Mot de passe requis pour ouvrir"
    elif info.get("encrypted"):
        forbidden = restrictions(info)
        protection = "Protégé : " + (", ".join(forbidden) if forbidden else "aucune restriction")
    else:
        protection = "Non protégé"
    rows = [
        ("Pages", info.get("pages")),
        ("Version PDF", info.get("pdf_version")),
        ("Protection", protection),
        ("Titre", info.get("title")),
        ("Auteur", info.get("author")),
        ("Sujet", info.get("subject")),
        ("Mots-clés", info.get("keywords")),
        ("Créé avec", info.get("creator")),
        ("Producteur", info.get("producer")),
        ("Créé le", day(info.get("created"))),
        ("Modifié le", day(info.get("modified"))),
    ]
    return [(name, str(value)) for name, value in rows if value not in (None, "")]


def reload_app():
    """restarts the whole process in place, so edits made to this script take effect"""
    os.execv(sys.executable, [sys.executable, *sys.argv])


# category, label, icon, shortcut, enabled, run. "Cmd" is the command key on macOS and ctrl elsewhere
ACTIONS = [
    ("Ouvrir", "Ouvrir", "open", "Cmd+ArrowDown", has_selection, action_open),
    (
        "Ouvrir",
        "Afficher le dossier parent",
        "folder-up",
        None,
        lambda: one_selected() and Path(selected.value).parent != current(),
        show_parent,
    ),
    *(
        [("Ouvrir", "Ouvrir avec Zed", "zed", "Cmd+Shift+Z", lambda: True, open_in_zed)]
        if zed_command()
        else []
    ),
    *(
        [
            (
                "Ouvrir",
                f"Ouvrir avec Claude ({label})",
                "claude",
                None,
                lambda: True,
                lambda mode=mode: open_in_claude(mode),
            )
            for mode, label in (("code", "Code"), ("cowork", "Cowork"))
        ]
        if app_installed("Claude") or sys.platform == "win32"
        else []
    ),
    (
        "Édition",
        "Renommer",
        "rename",
        "F2",
        one_selected,
        lambda: start_rename(Path(selected.value)),
    ),
    ("Édition", "Taguer…", "tag", "Cmd+T", has_selection, open_tag_picker),
    (
        "Édition",
        "Copier",
        "copy",
        "Cmd+C",
        has_selection,
        lambda: copy_to_clipboard(False),
    ),
    (
        "Édition",
        "Couper",
        "cut",
        "Cmd+X",
        has_selection,
        lambda: copy_to_clipboard(True),
    ),
    (
        "Édition",
        "Coller",
        "paste",
        "Cmd+V",
        lambda: bool(file_clipboard["paths"]),
        paste,
    ),
    ("Édition", "Dupliquer", "duplicate", "Cmd+D", has_selection, duplicate),
    (
        "Édition",
        "Regrouper dans un dossier",
        "group",
        "Ctrl+Cmd+N" if MACOS else "Cmd+Alt+N",
        can_group,
        group,
    ),
    (
        "Édition",
        "Nouveau dossier",
        "folder-plus",
        "Cmd+Shift+N",
        lambda: True,
        new_folder,
    ),
    (
        "Édition",
        "Mettre à la corbeille",
        "trash",
        "Cmd+Backspace",
        lambda: MACOS and has_selection(),
        trash,
    ),
    (
        "Outils",
        "Copier le chemin",
        "copy",
        "Cmd+Alt+C",
        lambda: True,
        lambda: context[0].copy_text("\n".join(map(str, targets()))),
    ),
    (
        "Outils",
        "Indexer le dossier dans SQLite",
        "folder-plus",
        None,
        lambda: not indexing["running"],
        index_folder_action,
    ),
    (
        "Outils",
        "Lire les PDF de l'index",
        "refresh",
        None,
        lambda: not indexing["running"],
        read_pdfs_action,
    ),
    (
        "Outils",
        "Surveiller le dossier",
        "refresh",
        None,
        lambda: target_directory() not in watched,
        watch_folder,
    ),
    (
        "Outils",
        "Ne plus surveiller le dossier",
        "close",
        None,
        lambda: target_directory() in watched,
        unwatch_folder,
    ),
    (
        "Outils",
        "Afficher dans le Finder" if MACOS else "Afficher dans l'explorateur",
        "reveal",
        None,
        lambda: True,
        reveal,
    ),
    ("Affichage", "Thème clair", "gear", None, lambda: True, lambda: set_theme(LIGHT)),
    ("Affichage", "Thème sombre", "gear", None, lambda: True, lambda: set_theme(DARK)),
    (
        "Affichage",
        "Thème système",
        "gear",
        None,
        lambda: True,
        lambda: set_theme(SYSTEM),
    ),
    (
        "Affichage",
        "Vitre partielle",
        "gear",
        None,
        lambda: True,
        lambda: set_glass(PARTIAL),
    ),
    (
        "Affichage",
        "Vitre complète",
        "gear",
        None,
        lambda: True,
        lambda: set_glass(FULL),
    ),
    (
        "Affichage",
        "Mode plat (sans vitre)",
        "gear",
        None,
        lambda: True,
        lambda: set_glass(NO_GLASS),
    ),
    (
        "Affichage",
        "Onglets classiques",
        "gear",
        None,
        lambda: True,
        lambda: set_tab_style(False),
    ),
    (
        "Affichage",
        "Onglets en pilule",
        "gear",
        None,
        lambda: True,
        lambda: set_tab_style(True),
    ),
    (
        "Affichage",
        "Vue arborescence",
        "tree",
        None,
        lambda: True,
        lambda: set_view(TREE),
    ),
    ("Affichage", "Vue liste", "list", None, lambda: True, lambda: set_view(LIST)),
    ("Affichage", "Vue icônes", "grid", None, lambda: True, lambda: set_view(ICONS)),
    *[
        (
            "Affichage",
            f"Accent {label}",
            "gear",
            None,
            lambda: True,
            lambda name=name: set_accent_name(name),
        )
        for name, label in {
            "Blue": "bleu",
            "Violet": "violet",
            "Green": "vert",
            "Orange": "orange",
        }.items()
    ],
    *[
        (
            "Affichage",
            f"Afficher le panneau {title}",
            "gear",
            None,
            lambda title=title: title not in dock.titles,
            lambda title=title: show_pane(title),
        )
        for title in (
            "Actions",
            "Preview",
            "Notes",
            "Tags",
            "Recherche",
            *WATCH_PANES,
            "Terminal",
            "Python",
        )  # ALL_PANES is defined further down
    ],
    (
        "Système",
        "Recharger l'application",
        "refresh",
        "Cmd+R",
        lambda: True,
        reload_app,
    ),
]


def action_keys():
    for _, _, _, shortcut, enabled, run in ACTIONS:
        if shortcut and enabled() and key_pressed(shortcut):
            run()
            return


# action palette: a searchable popup of the actions above, opened with right click, Space or Cmd+P


palette_pointer = {
    "pos": None
}  # the pointer position of the previous frame of the palette


def palette_actions():
    query = palette_query.value.lower()
    return [a for a in ACTIONS if a[4]() and query in a[1].lower()]


def run_palette_action(action):
    palette_open.value = False
    action[5]()


def palette_ui():
    tint = icon_tint()
    accent = ACCENTS[accent_name.value]

    set_width(
        460
    )  # a fixed width, so available_width() stays bounded inside this auto-sized modal
    row_width = (
        available_width()
    )  # measured once here: inside the scroll area below it is unreliable
    text_edit_singleline(
        palette_query, hint_text="Search an action…", request_focus=True
    )
    actions = palette_actions()
    if not actions:
        add_space(4)
        weak("No matching action")
        return
    palette_index.value = min(palette_index.value, len(actions) - 1)

    moved_by_key = False
    pointer = pointer_pos()
    pointer_moved = (
        pointer != palette_pointer["pos"]
    )  # a still mouse must not fight the keyboard
    palette_pointer["pos"] = pointer
    if key_pressed("ArrowDown", when_typing=True):
        palette_index.value = min(palette_index.value + 1, len(actions) - 1)
        moved_by_key = True
    if key_pressed("ArrowUp", when_typing=True):
        palette_index.value = max(palette_index.value - 1, 0)
        moved_by_key = True
    if key_pressed("Enter", when_typing=True):
        run_palette_action(actions[palette_index.value])

    add_space(4)

    def rows():
        for i, action in enumerate(actions):
            _, text, icon_name, shortcut, _, _ = action

            def row(icon_name=icon_name, text=text, shortcut=shortcut):
                def line():
                    # a hard cap, not just a floor: the row must not grow past it, otherwise
                    # available_width() below keeps changing and the shortcut lands off-screen
                    set_width(row_width)
                    image(svg(icon_name), size=16, tint=tint)
                    label(text, selectable=False)
                    add_space(max(available_width() - (124 if shortcut else 8), 8))
                    if shortcut:
                        weak(shortcut)

                horizontal(line)

            response = interactive(row)
            if i == palette_index.value:
                draw_rect(response.rect, fill=accent + "40", rounding=6)
                if moved_by_key:
                    scroll_to_rect(response.rect)
            elif response.hovered:
                draw_rect(response.rect, fill=(127, 127, 127, 30), rounding=6)
            if response.hovered and pointer_moved:
                palette_index.value = (
                    i  # the mouse takes over the highlight from the keyboard
                )
            if response.clicked:
                run_palette_action(action)

    def bounded():
        # set_max_height measures from the top of its own ui, so it needs a fresh one here,
        # not the ui the search field already advanced the cursor in
        set_max_height(320)
        scroll_area_vertical(rows)

    vertical(bounded)


def action_palette():
    if modal("palette", palette_ui, rounding=12):
        palette_open.value = False


def actions_panel():
    tint = icon_tint()
    accent = ACCENTS[accent_name.value]
    for category in dict.fromkeys(a[0] for a in ACTIONS):
        opened = category not in folded
        if image_button_clicked(
            svg("chevron-down" if opened else "chevron-right"),
            category.upper(),
            size=12,
            tint=accent,
            frame=False,
        ):
            folded.symmetric_difference_update({category})
        if not opened:
            continue
        for cat, text, icon_name, shortcut, enabled, run in ACTIONS:
            if cat != category:
                continue

            def button(text=text, icon_name=icon_name, run=run):
                if image_button_clicked(
                    svg(icon_name), text, size=15, tint=tint, frame=False
                ):
                    run()

            def row(enabled=enabled, button=button):
                add_enabled(enabled(), button)

            tooltip(f"{text} ({shortcut})" if shortcut else text, row)
        add_space(8)


def rename_dialog():
    """a small window asking the new name of the item"""
    path = Path(renaming.value)

    def form():
        weak(f"Rename “{path.name}”")
        text_edit_singleline(new_name)
        valid = bool(new_name.value) and "/" not in new_name.value
        if (
            key_pressed("Enter", when_typing=True)
            and valid
            or button_clicked("Rename")
            and valid
        ):
            if new_name.value != path.name:
                try:
                    path.rename(path.with_name(new_name.value))
                    tags.moved(path, path.with_name(new_name.value))
                    select_path(path.with_name(new_name.value))
                    refresh(path.parent)
                except OSError:
                    pass
            renaming.value = ""

    if modal("rename", form):
        renaming.value = ""


# path bar


def go_or_select(path):
    """a folder of the path bar is entered, the selected item shown in its folder"""
    path = Path(path)
    if path.is_dir() and path != Path(selected.value):
        go(path)
    else:
        go(path.parent)
        select_path(path)
        scroll_target.value = str(path)


def path_menu(path):
    """the menu of a folder of the path bar or of the title"""
    strong(place_name(Path(path)))
    separator()
    if button_clicked("Ouvrir"):
        go_or_select(path)
        close_menu()
    if button_clicked("Copier le chemin"):
        context[0].copy_text(path)
        close_menu()


def path_bar():
    def crumbs():
        if image_button_clicked(
            svg("sidebar"), size=16, tint=toggle_tint(show_sidebar.value), selected=show_sidebar.value
        ):
            show_sidebar.value = not show_sidebar.value
        add_space(4)

        count = len(visible_entries())
        text = index_status() or f"{count} item{'s' if count != 1 else ''}"
        with_text = (
            available_width() >= widths["count"] + SLACK
        )  # the count goes first when narrow
        key = "count" if with_text else "dock"
        # the folders down to the selected item, the parents shortened when the room is missing
        here = selected.value if len(picked) == 1 else str(current())
        path_breadcrumb(
            here,
            max_width=available_width() - widths[key] - SLACK - 8,
            on_click=go_or_select,
            context_menu=path_menu,
        )
        # no minimum: when the breadcrumb takes all its max_width, what is left is exactly the width
        # of the right side; a few more points would push it out, measure() would widen it, the
        # breadcrumb shrink, and the row would go back and forth at every frame
        add_space(max(available_width() - widths[key] - SLACK, 0))

        def right():
            if with_text:
                weak(text)
                add_space(4)
            if image_button_clicked(
                svg("dock"), size=16, tint=toggle_tint(show_dock.value), selected=show_dock.value
            ):
                show_dock.value = not show_dock.value

        measure(key, right, last=True)

    (horizontal_centered if unified.value else horizontal)(crumbs)


def in_zone(zone, ui):
    """a click anywhere in the part of the window moves the keyboard focus there"""

    def run():
        ui()
        # after the drawing: before it, the ui has no size yet and never contains the pointer
        if pointer_pressed() and ui_contains_pointer():
            focus_zone.value = zone

    return run


FOCUS_HOLD, FOCUS_FADE = 0.6, 0.4  # seconds the outline stays, then takes to fade out
focus_outline = {"zone": None, "since": 0.0}


def zone_stroke(zone, otherwise):
    """the outline of the focused zone in the accent color: it shows where the keyboard acts when
    the focus moves, then fades out"""
    if focus_zone.value != focus_outline["zone"]:  # the focus just moved
        focus_outline.update(zone=focus_zone.value, since=time.monotonic())
    if focus_zone.value != zone:
        return otherwise
    age = time.monotonic() - focus_outline["since"]
    alpha = min(max(1 - (age - FOCUS_HOLD) / FOCUS_FADE, 0.0), 1.0)
    if alpha == 0:
        return otherwise
    context[0].request_repaint()  # the fade runs without any other event
    color = ACCENTS[accent_name.value]
    r, g, b = (int(color[i : i + 2], 16) for i in (1, 3, 5))
    return (EDGE[0], (r, g, b, round(255 * alpha)))


# the dock fills the middle of the window: the files are one of its panes (that cannot be closed), the
# others can be closed, moved by their tab and dropped on any side of the files or of each other
CENTRAL = "Fichiers"


def zone_pane(zone, ui, margin=0):
    """a pane of the dock that is a zone of the keyboard focus: a click in it gives it the focus, and
    its outline shows it when the focus moves"""
    return lambda: frame(
        in_zone(zone, ui),
        inner_margin=margin,
        rounding=6,
        stroke=zone_stroke(zone, (EDGE[0], (0, 0, 0, 0))),
    )


PANES = {
    "Actions": zone_pane("dock", lambda: scroll_area_vertical(actions_panel)),
    "Preview": zone_pane("dock", lambda: scroll_area_vertical(preview)),
    "Notes": zone_pane("dock", notes_panel),
    "Tags": zone_pane("dock", lambda: scroll_area_vertical(tags_panel)),
    "Recent": zone_pane("dock", lambda: scroll_area_vertical(recent_panel)),
    "Recherche": zone_pane("dock", lambda: scroll_area_vertical(find_panel)),
    "Dossiers surveillés": zone_pane("dock", lambda: scroll_area_vertical(watched_panel)),
    "Activités": zone_pane("dock", lambda: scroll_area_vertical(activity_panel)),
}
# the dock at the bottom, under the files
BOTTOM_PANES = {
    "Terminal": zone_pane("dock", shell_pane),
    "Python": zone_pane("dock", python_pane),
}
ALL_PANES = {**PANES, **BOTTOM_PANES}
central_ui = zone_pane("content", content, margin=8)
dock = Dock(
    DockContainer(
        "horizontal",
        [
            DockContainer(
                "vertical",
                [
                    DockPane(CENTRAL, central_ui, hide_tab=True, central=True),  # no tab while the files are alone
                    DockContainer(
                        "tabs",
                        [DockPane(title, ui, closable=True) for title, ui in BOTTOM_PANES.items()],
                    ),
                ],
            ),
            DockContainer(
                "vertical",
                [DockPane(title, ui, closable=True) for title, ui in PANES.items() if title not in WATCH_PANES],
            ),
        ],
    ),
    id="inspector",
    tab_bar_height=26,
    pane_margin=8,
    gap_width=GAP,  # a gap between the panes, as wide as between the other boxes
    pane_stroke=EDGE,
    active_tab_stroke=EDGE,
    inactive_tab_stroke=EDGE,
    resize_stroke=(
        1,
        (0, 0, 0, 0),
    ),  # the gap stays empty: no grey band, the panes still resize
)
default_layout = dock.save()
default_layout["shares"] = [4.0, 1.0]  # the files take most of the width
default_layout["children"][0]["shares"] = [3.0, 1.0]  # and most of the height, over the terminals
dock.load(default_layout)


def update_func(ctx):
    if not started.value:
        context.append(ctx)
        restore(ctx.load_state())
        apply_style()
        started.value = True

    keyboard()
    action_keys()
    open_requested_panes()  # before the dock is drawn
    collect_activity()
    follow_in_shell()
    if unified.value:
        unified_layout()
    else:
        boxes_layout()
    if renaming.value:
        rename_dialog()
    if palette_open.value:
        action_palette()
    if tag_picker_open.value:
        tag_picker()
    if tag_manager_open.value:
        tag_manager()
    ctx.save_state(snapshot())  # written to disk by pyegui, see persist_id below


def unified_layout():
    """like the Claude app: a sidebar along the whole height, a line between it and the body, and in
    both a top band, a content and a bottom band whose lines continue from one to the other"""
    left_panel(
        "sidebar",
        in_zone("sidebar", sidebar),
        expanded=show_sidebar,
        default_size=240,
        min_size=200,
        inner_margin=0,
        outer_margin=0,
        rounding=0,
        show_separator_line=True,  # the line between the sidebar and the body
        separator_stroke=LINE,
    )
    top_panel("toolbar", toolbar, exact_size=TOP_H, resizable=False, inner_margin=(8, 0, 12, 0), rounding=0, show_separator_line=True, separator_stroke=LINE)
    bottom_panel("path", path_bar, exact_size=BOTTOM_H, resizable=False, inner_margin=(8, 0, 12, 0), rounding=0, show_separator_line=True, separator_stroke=LINE)
    # one gap everywhere: a side of the dock that faces a neighbour without gap (the line of the
    # sidebar, of the bars, the border of the window) gets the whole gap, and the dock puts the same
    # gap between its panes. The central panel adds no margin of its own (set_central_margin)
    dock_or_files(outer_margin=UNIFIED_GAP)


def dock_or_files(**kwargs):
    """the panes of the dock, or the files alone when the dock is hidden"""
    frame(lambda: dock_area(dock) if show_dock.value else frame(central_ui, stroke=EDGE, rounding=6), **kwargs)


def boxes_layout():
    """the sidebar, the bars and the dock are rounded boxes with gaps"""
    left_panel(
        "sidebar",
        in_zone("sidebar", sidebar),
        expanded=show_sidebar,
        default_size=215,
        min_size=200,
        inner_margin=12,
        outer_margin=SIDEBAR_MARGIN,
        show_separator_line=False,
        rounding=SIDEBAR_ROUNDING,
        stroke=zone_stroke("sidebar", EDGE),
    )
    bar = dict(inner_margin=8, show_separator_line=False, rounding=12, stroke=EDGE)
    top_panel("toolbar", toolbar, outer_margin=(0, 0, 0, H), **bar)
    bottom_panel("path", path_bar, outer_margin=(0, H, 0, 0), **bar)
    dock_or_files(outer_margin=(0, H, 0, H))  # the same as the bars: the panes line up with them


if __name__ == "__main__":
    if sys.platform == "win32":
        import ctypes

        ctypes.windll.shell32.SetCurrentProcessExplicitAppUserModelID(APP_USER_MODEL_ID)
    run_native(
        "Finder",
        update_func,
        inner_width=1180,
        inner_height=720,
        backdrop=BACKDROP,
        fullsize_content=True,
        icon_path=str(APP_ICON),
        persist_id=APP_USER_MODEL_ID,  # the window and the choices come back at the next run
    )
