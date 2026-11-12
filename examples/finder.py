"""A mini Finder written only with the Python API of pyegui.

Sidebar with favorites, toolbar with back / forward, list, icon and tree views, search, a preview
panel and a path bar. The icons are the ones of the Finder on macOS.

Mouse: click to select, double click to open (folders here, files with their app). Right-click
opens the action palette. In the tree view, the arrow of a folder shows its contents in the tree.
Keyboard: arrows to select, Enter or Cmd+Down to open, Backspace or Cmd+Up for the parent folder,
Cmd+[ and Cmd+] to go back and forward, Escape to clear the search and the selection,
Space or Cmd+P to open the action palette (search and run an action for the current selection).

    python finder.py [folder]
"""

import os
import shutil
import subprocess
import sys
import time
from datetime import datetime
from pathlib import Path
from urllib.parse import quote

from pyegui import (
    Backdrop,
    Bool,
    Dock,
    DockContainer,
    DockPane,
    Int,
    Str,
    TreeNode,
    TreeView,
    add_enabled,
    add_space,
    available_height,
    available_width,
    bottom_panel,
    button_clicked,
    close_menu,
    code,
    colored_label,
    columns,
    dock_area,
    draw_rect,
    end_row,
    file_icon,
    frame,
    grid,
    horizontal,
    image,
    image_button_clicked,
    interactive,
    key_pressed,
    label,
    left_panel,
    link_clicked,
    menu_button,
    menu_image_button,
    modal,
    modifiers,
    pointer_pos,
    pointer_pressed,
    radio_clicked,
    radio_value,
    right_panel,
    run_native,
    scroll_area_both,
    scroll_area_vertical,
    scroll_to_rect,
    segmented_control,
    selectable_label_response,
    separator,
    set_max_height,
    set_min_height,
    set_min_width,
    set_width,
    small,
    strong,
    table_ui,
    text_edit_singleline,
    toggle_value,
    tooltip,
    top_panel,
    tree_view,
    typed_text,
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
TAGS = [
    ("Red", "#ef4444"),
    ("Orange", "#f97316"),
    ("Green", "#22c55e"),
    ("Blue", "#3b82f6"),
    ("Purple", "#a855f7"),
]

# state

start = Path(sys.argv[1]).expanduser().resolve() if len(sys.argv) > 1 else HOME
history = [start]
position = Int(0)
selected = Str("")  # the item the single-item commands act on: the last one clicked
picked = set()  # paths of all the selected items
anchor = Str("")  # where a range with Shift starts
band = {"start": None, "base": set()}  # the rectangle being drawn with the mouse
tree_picked = set()  # what the tree view showed last frame
typed = {"text": "", "last": 0.0}  # the letters typed to jump to a name
scroll_target = Str("")  # path of the item the view has to scroll to this frame
view = Int(LIST)
search = Str("")
show_sidebar = Bool(True)
ZONES = ["sidebar", "content", "dock"]  # F6 goes through them, left to right
focus_zone = Str("content")  # the part of the window the keyboard acts on, see keyboard()
show_dock = Bool(True)  # the dock on the right: Actions and Preview
folded = set()  # categories of the Actions panel folded shut
renaming = Str("")  # path being renamed, or ""
new_name = Str("")
palette_open = Bool(False)  # the action palette: a searchable list of the actions below
palette_query = Str("")
palette_index = Int(0)  # the highlighted action, moved by the keyboard or the mouse
file_clipboard = {"paths": [], "cut": False}
theme = Int(SYSTEM)
glass = Int(PARTIAL)
accent_name = Str("Blue")
pill_tabs = Bool(False)
icons_per_row = Int(1)
started = Bool(False)
context = []  # the Context, kept on the first frame

listing_cache = {}  # folder -> entries, read once
tree = TreeView("tree", multi_select=True, default_open=False, activatable_dirs=True)
tree_ids = {}  # path -> id of the tree node
tree_paths = {}  # id -> path
tree_to_open = set()  # folders that were open last run: opened as their node is first built


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
    if folder not in listing_cache:
        try:
            paths = [p for p in folder.iterdir() if not p.name.startswith(".")]
        except OSError:
            paths = []
        entries = [e for e in map(entry, paths) if e]
        # folders first, then by name like the Finder
        listing_cache[folder] = sorted(
            entries, key=lambda e: (not e["is_dir"], e["name"].lower())
        )
    return listing_cache[folder]


def visible_entries():
    query = search.value.lower()
    return [e for e in listing(current()) if query in e["name"].lower()]


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


def go(folder):
    folder = Path(folder)
    if folder == current() or not folder.is_dir():
        return
    del history[position.value + 1 :]
    history.append(folder)
    position.value = len(history) - 1
    deselect()
    search.value = ""


def back():
    if position.value > 0:
        position.value -= 1
        deselect()


def up():
    """go to the parent folder and select the folder we come from, like the Finder"""
    child = current()
    go(child.parent)
    if current() != child:
        selected.value = str(child)


def forward():
    if position.value < len(history) - 1:
        position.value += 1
        deselect()


def open_item(item):
    if item["is_dir"]:
        go(item["path"])
    else:
        context[0].open_url(item["path"].as_uri())  # with the app of the file


def select(item):
    select_path(item["path"])


def pick(item, entries):
    """a click on an item: alone, Cmd toggles it, Shift selects the range from the last click"""
    path = str(item["path"])
    held = modifiers()
    if held["command"]:
        if path in picked:
            picked.discard(path)
            selected.value = next(iter(picked), "")
        else:
            picked.add(path)
            selected.value = path
        anchor.value = path
    elif held["shift"] and anchor.value:
        paths = [str(e["path"]) for e in entries]
        if anchor.value in paths and path in paths:
            a, b = sorted((paths.index(anchor.value), paths.index(path)))
            picked.clear()
            picked.update(paths[a : b + 1])
            selected.value = path
        else:
            select(item)
    else:
        select(item)


def open_palette():
    palette_query.value = ""
    palette_index.value = 0
    palette_open.value = True


def handle(item, response, entries):
    if response.clicked:
        pick(item, entries)
    if response.double_clicked:
        open_item(item)
    if response.secondary_clicked:
        if str(item["path"]) not in picked:
            select(item)
        open_palette()


def type_to_select():
    """typing the start of a name selects the first item of the view starting with it"""
    text = typed_text()
    if not text or not text.isprintable() or (not typed["text"] and not text.strip()):
        return
    now = time.monotonic()
    if now - typed["last"] > 1.0:  # a pause starts a new search
        typed["text"] = ""
    typed["text"] += text
    typed["last"] = now
    for item in visible_entries():
        if item["name"].lower().startswith(typed["text"].lower()):
            select(item)
            scroll_target.value = str(item["path"])
            return


def cycle_zone(step):
    zones = [z for z in ZONES if (z != "sidebar" or show_sidebar.value) and (z != "dock" or show_dock.value)]
    index = zones.index(focus_zone.value) if focus_zone.value in zones else zones.index("content")
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
    if palette_open.value or renaming.value:
        # those modals have their own Escape-closes-on-click-outside handling (see modal());
        # Escape uses when_typing=True below to work while a field has focus, so without this
        # guard it would also clear the background search/selection at the same time
        return
    if key_pressed("F6"):
        cycle_zone(1)
    if key_pressed("Shift+F6"):
        cycle_zone(-1)
    if focus_zone.value == "content":
        type_to_select()
    if key_pressed("Space") and not typed["text"]:  # not the space of an active type-to-select
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
    if view.value == TREE:
        return  # the tree view has its own keyboard navigation, its Enter opens like the Open button
    if picked and (key_pressed("Enter") or key_pressed("Cmd+ArrowDown")):
        action_open()

    entries = visible_entries()
    if not entries:
        return
    paths = [str(e["path"]) for e in entries]
    index = paths.index(selected.value) if selected.value in paths else -1
    per_row = icons_per_row.value if view.value == ICONS else 1
    page = per_row * 5
    steps = [
        ("ArrowDown", per_row),
        ("ArrowUp", -per_row),
        ("PageDown", page),
        ("PageUp", -page),
    ]
    if view.value == ICONS:
        steps += [("ArrowRight", 1), ("ArrowLeft", -1)]
    target = None
    for key, step in steps:
        if key_pressed(key):
            target = min(max(index + step, 0), len(entries) - 1) if index >= 0 else 0
    if key_pressed("Home"):
        target = 0
    if key_pressed("End"):
        target = len(entries) - 1
    if target is None:
        return
    if (
        modifiers()["shift"] and anchor.value in paths
    ):  # Shift extends the range from the anchor
        a, b = sorted((paths.index(anchor.value), target))
        picked.clear()
        picked.update(paths[a : b + 1])
        selected.value = paths[target]
    else:
        select(entries[target])
    scroll_target.value = paths[target]  # the view follows the selection


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
        )
    else:
        ctx.set_accent(ACCENTS[accent_name.value])
    dock.set_style(tab_style="pill" if pill_tabs.value else "classic")


def open_folders():
    """the folders open in the tree view, including the ones of the last run not shown yet"""
    shown = {path for path, node_id in tree_ids.items() if tree.is_open(node_id)}
    return shown | (tree_to_open - set(tree_ids))


def snapshot():
    """what the next run starts from: the choices of the user, not the selection"""
    return {
        "theme": theme.value,
        "glass": glass.value,
        "accent": accent_name.value,
        "pill_tabs": pill_tabs.value,
        "view": view.value,
        "show_sidebar": show_sidebar.value,
        "show_dock": show_dock.value,
        "folded": sorted(folded),
        "folder": str(current()),
        "tree_open": sorted(str(p) for p in open_folders()),
        "dock": dock.save(),  # the panes still open, their tabs and sizes
    }


def restore(saved):
    """the reverse of snapshot(); a value that is missing or no longer valid keeps its default"""
    if not isinstance(saved, dict):
        return
    theme.value = saved.get("theme", theme.value)
    glass.value = saved.get("glass", glass.value)
    if saved.get("accent") in ACCENTS:
        accent_name.value = saved["accent"]
    pill_tabs.value = saved.get("pill_tabs", pill_tabs.value)
    view.value = saved.get("view", view.value)
    show_sidebar.value = saved.get("show_sidebar", show_sidebar.value)
    show_dock.value = saved.get("show_dock", show_dock.value)
    folded.update(saved.get("folded", []))
    tree_to_open.update(Path(p) for p in saved.get("tree_open", []) if isinstance(p, str))
    if saved.get("dock"):
        dock.load(saved["dock"])  # a layout that no longer fits is ignored
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


def settings_menu():
    before = (theme.value, glass.value, accent_name.value)
    toggle_value(show_sidebar, "Sidebar")
    toggle_value(show_dock, "Dock")
    for title, ui in PANES.items():  # a pane closed with its tab comes back from here
        if title not in dock.titles and button_clicked(f"Show {title}"):
            dock.add_pane(DockPane(title, ui, closable=True))
    separator()
    small("THEME")
    radio_value(theme, LIGHT, "Light")
    radio_value(theme, DARK, "Dark")
    radio_value(theme, SYSTEM, "System")
    separator()
    small("GLASS")
    radio_value(glass, PARTIAL, "Partial")
    radio_value(glass, FULL, "Full")
    radio_value(glass, NO_GLASS, "Off")
    separator()
    small("ACCENT")
    for accent in ACCENTS:
        if radio_clicked(accent_name.value == accent, accent):
            accent_name.value = accent
    if (theme.value, glass.value, accent_name.value) != before:
        apply_style()


# sidebar


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
    if MACOS:
        add_space(26)  # the window buttons are over the sidebar

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
    weak("Tags")
    for name, color in TAGS:

        def tag(name=name, color=color):
            colored_label(color, "●")
            label(name)

        horizontal(tag)


# toolbar


def svg(name):
    return (ICONS_DIR / f"{name}.svg").as_uri()


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
    return [Path(f"{letter}:/") for i, letter in enumerate(string.ascii_uppercase) if mask & (1 << i)]


def ancestors_menu():
    """the current folder then its parents, like the title menu of the Finder"""
    for path in [current(), *current().parents]:

        def row(path=path):
            # the check mark of the current folder, an invisible one keeps the others aligned
            colored_label(
                (127, 127, 127, 255) if path == current() else (0, 0, 0, 0), "✓"
            )
            uri = file_icon(str(path), 16)
            clicked = (
                image_button_clicked(uri, place_name(path))
                if uri
                else button_clicked(place_name(path))
            )
            if clicked:
                go(path)
                close_menu()

        horizontal(row)


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
        menu_button(f"{place_name(current())}  ▾", ancestors_menu, size=14)
        add_space(max(available_width() - 640, 8))
        segmented_control(
            view,
            [(svg("tree"), None), (svg("list"), None), (svg("grid"), None)],
            size=16,
            tint=tint,
        )
        text_edit_singleline(search, hint_text="Search")

    horizontal(bar)


# views


def list_view():
    entries = visible_entries()

    def cell(row, col):
        item = entries[row]
        if col == 0:

            def name():
                icon(item, 18)
                handle(
                    item,
                    selectable_label_response(
                        str(item["path"]) in picked, item["name"]
                    ),
                    entries,
                )

            horizontal(name)
        elif col == 1:
            weak(item["modified"])
        elif col == 2:
            weak(item["size"])
        else:
            weak(item["kind"])

    paths = [str(e["path"]) for e in entries]
    row = paths.index(scroll_target.value) if scroll_target.value in paths else None
    table_ui(
        ["Name", "Date Modified", "Size", "Kind"],
        len(entries),
        cell,
        id=f"list {current()}",
        column_widths=[290, 165, 80, "remainder"],
        striped=True,
        scroll_to_row=row,
    )


def icon_view():
    entries = visible_entries()
    width, height = available_width(), available_height()
    per_row = max(int(width // 120), 1)
    icons_per_row.value = per_row
    tiles = {}  # path -> where the tile is, for the rectangle of the mouse
    accent = ACCENTS[accent_name.value]

    def rows():
        for start_index in range(0, len(entries), per_row):
            row = entries[start_index : start_index + per_row]

            def column(i, row=row):
                if i >= len(row):
                    return
                item = row[i]

                def tile():
                    set_min_width(available_width())  # every tile as wide as its column
                    icon(item, 56)
                    # not selectable, so the click reaches the tile instead of selecting the text
                    label(item["name"], truncate=True, selectable=False)

                # the icon and the name are one target: a click anywhere on the tile selects it
                response = interactive(lambda: vertical_centered(tile))
                tiles[str(item["path"])] = response.rect
                if str(item["path"]) in picked:
                    draw_rect(
                        response.rect,
                        fill=accent + "50",
                        stroke=(1, accent),
                        rounding=6,
                    )
                elif response.hovered:
                    draw_rect(response.rect, fill=(127, 127, 127, 40), rounding=6)
                handle(item, response, entries)
                if str(item["path"]) == scroll_target.value:
                    scroll_to_rect(response.rect)  # the whole tile comes into view

            columns(per_row, column)
            add_space(10)

    def background():
        set_min_width(width)  # the empty space around the tiles takes the mouse too
        set_min_height(height)
        rows()

    def area():
        response = interactive(background, drag=True)
        if response.clicked and not any(modifiers().values()):
            deselect()
        if response.secondary_clicked:
            open_palette()
        rubber_band(response, tiles, accent)

    scroll_area_vertical(area)


def rubber_band(response, tiles, accent):
    """select the tiles touched by a rectangle drawn with the mouse; Cmd or Shift keeps the selection"""
    here = pointer_pos()
    if response.drag_started and here:
        band["start"] = here
        band["base"] = set(picked) if any(modifiers().values()) else set()
    if response.drag_stopped:
        band["start"] = None
    if not (band["start"] and response.dragged and here):
        return
    (x0, y0), (x1, y1) = band["start"], here
    rect = (min(x0, x1), min(y0, y1), max(x0, x1), max(y0, y1))
    touched = {
        path
        for path, (l, t, r, b) in tiles.items()
        if l < rect[2] and r > rect[0] and t < rect[3] and b > rect[1]
    }
    picked.clear()
    picked.update(band["base"] | touched)
    if selected.value not in picked:
        selected.value = next(iter(touched), "") or next(iter(picked), "")
    draw_rect(rect, fill=accent + "30", stroke=(1, accent))


def tree_node(item):
    path = item["path"]
    if path not in tree_ids:
        tree_ids[path] = len(tree_ids)
        tree_paths[tree_ids[path]] = path
        if path in tree_to_open:  # ids change at every run, so the folders are kept by path
            tree.set_open(tree_ids[path], True)
    node_id = tree_ids[path]
    # the icon of the Finder, or an emoji where the platform gives none
    icon_uri = file_icon(str(path), 16)
    text = item["name"] if icon_uri else f"{item['emoji']}  {item['name']}"
    if not item["is_dir"]:
        return TreeNode(node_id, text, icon=icon_uri)
    # the children of a folder are read only once it is opened
    children = (
        [tree_node(child) for child in listing(path)] if tree.is_open(node_id) else []
    )
    return TreeNode(node_id, text, children, icon=icon_uri)


tree_menu = {"ids": None}  # nodes right-clicked this frame, applied once tree_view returns


def tree_context_menu(ids):
    """a right click on a row of the tree opens the actions panel, like in the other views"""
    close_menu()  # the panel replaces the menu
    tree_menu["ids"] = ids  # tree is borrowed by tree_view here: it is updated afterwards
    open_palette()


def seed_tree_selection():
    """the tree view moves from the selected node, so with none (at the start, after entering a
    folder, after Escape) the first arrow selects the first or the last item, like the other views.
    The tree is left without focus for that frame, or it would also move by one"""
    if picked or focus_zone.value != "content" or palette_open.value or renaming.value:
        return
    down, up = key_pressed("ArrowDown"), key_pressed("ArrowUp")
    entries = visible_entries()
    if (down or up) and entries:
        item = entries[0 if down else -1]
        select(item)
        scroll_target.value = str(item["path"])
        tree.focus(False)


def tree_view_ui():
    seed_tree_selection()

    def nodes():
        query = search.value.lower()
        nodes_ = [
            tree_node(e) for e in listing(current()) if query in e["name"].lower()
        ]
        if (
            picked != tree_picked
        ):  # selected somewhere else since the last frame: the tree follows
            tree.selected = [tree_ids[Path(p)] for p in picked if Path(p) in tree_ids]
        tree_view(
            tree,
            nodes_,
            scroll_to=tree_ids.get(Path(scroll_target.value))
            if scroll_target.value
            else None,
            context_menu=tree_context_menu,
        )
        if tree_menu["ids"] is not None:
            ids, tree_menu["ids"] = tree_menu["ids"], None
            if not all(i in tree.selected for i in ids):
                tree.selected = [i for i in ids if i in tree_paths]
        tree_picked.clear()
        tree_picked.update(str(tree_paths[i]) for i in tree.selected)
        if tree_picked != picked:
            picked.clear()
            picked.update(tree_picked)
            selected.value = str(tree_paths[tree.selected[-1]]) if tree.selected else ""
            anchor.value = selected.value
        for node_id in tree.activated:
            # a double click or Enter opens like the Open button: into a folder, a file with its app.
            # The arrow of a folder opens it in the tree
            item = entry(tree_paths[node_id])
            if item:
                open_item(item)

    scroll_area_both(nodes)


def focus_tree():
    """the tree view keeps the egui focus while the content is the focused zone: a click on a
    button or a blank area, or a widget that grabs the focus, must not cut the arrows. A text field
    being edited (the search) keeps it, and so does a dialog"""
    popup = palette_open.value or renaming.value
    in_content = focus_zone.value == "content" and not popup
    tree.keep_focus = in_content
    if focus_zone.value != "content":
        tree.focus(False)


def content():
    if not listing(current()) or (search.value and not visible_entries()):
        weak("No items" if not search.value else f"No items match “{search.value}”")
        return
    focus_tree()
    [tree_view_ui, list_view, icon_view][view.value]()
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

    suffix = item["path"].suffix.lower()
    if suffix in IMAGE_SUFFIXES:
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
        subprocess.Popen([*command, *map(str, targets())])  # without selection: the current folder


def open_in_claude(mode):
    """Claude Desktop opens the folder in Code or Cowork mode, and asks to confirm it"""
    context[0].open_url(
        f"claude://{mode}/new?folder={quote(str(target_directory()), safe='')}"
    )


def has_selection():
    return bool(chosen())


def one_selected():
    return len(chosen()) == 1


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
    ("Édition", "Copier", "copy", "Cmd+C", has_selection, lambda: copy_to_clipboard(False)),
    ("Édition", "Couper", "cut", "Cmd+X", has_selection, lambda: copy_to_clipboard(True)),
    ("Édition", "Coller", "paste", "Cmd+V", lambda: bool(file_clipboard["paths"]), paste),
    ("Édition", "Dupliquer", "duplicate", "Cmd+D", has_selection, duplicate),
    (
        "Édition",
        "Regrouper dans un dossier",
        "group",
        "Ctrl+Cmd+N" if MACOS else "Cmd+Alt+N",
        can_group,
        group,
    ),
    ("Édition", "Nouveau dossier", "folder-plus", "Cmd+Shift+N", lambda: True, new_folder),
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
        "Afficher dans le Finder" if MACOS else "Afficher dans l'explorateur",
        "reveal",
        None,
        lambda: True,
        reveal,
    ),
    ("Affichage", "Thème clair", "gear", None, lambda: True, lambda: set_theme(LIGHT)),
    ("Affichage", "Thème sombre", "gear", None, lambda: True, lambda: set_theme(DARK)),
    ("Affichage", "Thème système", "gear", None, lambda: True, lambda: set_theme(SYSTEM)),
    ("Affichage", "Vitre partielle", "gear", None, lambda: True, lambda: set_glass(PARTIAL)),
    ("Affichage", "Vitre complète", "gear", None, lambda: True, lambda: set_glass(FULL)),
    ("Affichage", "Mode plat (sans vitre)", "gear", None, lambda: True, lambda: set_glass(NO_GLASS)),
    ("Affichage", "Onglets classiques", "gear", None, lambda: True, lambda: set_tab_style(False)),
    ("Affichage", "Onglets en pilule", "gear", None, lambda: True, lambda: set_tab_style(True)),
    ("Affichage", "Vue arborescence", "tree", None, lambda: True, lambda: set_view(TREE)),
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
    ("Système", "Recharger l'application", "refresh", "Cmd+R", lambda: True, reload_app),
]


def action_keys():
    for _, label_, _, shortcut, enabled, run in ACTIONS:
        if shortcut and enabled() and key_pressed(shortcut):
            run()
            return


# action palette: a searchable popup of the actions above, opened with right click, Space or Cmd+P


palette_pointer = {"pos": None}  # the pointer position of the previous frame of the palette


def palette_actions():
    query = palette_query.value.lower()
    return [a for a in ACTIONS if a[4]() and query in a[1].lower()]


def run_palette_action(action):
    palette_open.value = False
    action[5]()


def palette_ui():
    tint = icon_tint()
    accent = ACCENTS[accent_name.value]

    set_width(460)  # a fixed width, so available_width() stays bounded inside this auto-sized modal
    row_width = available_width()  # measured once here: inside the scroll area below it is unreliable
    text_edit_singleline(palette_query, hint_text="Search an action…", request_focus=True)
    actions = palette_actions()
    if not actions:
        add_space(4)
        weak("No matching action")
        return
    palette_index.value = min(palette_index.value, len(actions) - 1)

    moved_by_key = False
    pointer = pointer_pos()
    pointer_moved = pointer != palette_pointer["pos"]  # a still mouse must not fight the keyboard
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
                palette_index.value = i  # the mouse takes over the highlight from the keyboard
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
                    select_path(path.with_name(new_name.value))
                    refresh(path.parent)
                except OSError:
                    pass
            renaming.value = ""

    if modal("rename", form):
        renaming.value = ""


# path bar


def path_bar():
    tint = icon_tint()

    def crumbs():
        toggle_value(show_sidebar, "◧")
        add_space(4)

        anchor = Path(current().anchor)
        ancestors = list(reversed(current().parents))[1:]
        # current() is the anchor itself at a drive root: do not repeat it as an empty crumb
        parts = [anchor] + ancestors + ([current()] if current() != anchor else [])
        for i, part in enumerate(parts):
            if i:
                weak("›")
            if link_clicked(place_name(part) if i == 0 else part.name):
                go(part)

        add_space(max(available_width() - 190, 8))
        count = len(visible_entries())
        weak(f"{count} item{'s' if count != 1 else ''}")
        add_space(4)
        toggle_value(show_dock, "◨")
        add_space(4)
        menu_image_button(svg("gear"), settings_menu, size=18, tint=tint)

    horizontal(crumbs)


# the dock on the right: panes the user can close, move by their tab, and drop on another side
PANES = {
    "Actions": lambda: scroll_area_vertical(actions_panel),
    "Preview": lambda: scroll_area_vertical(preview),
}
dock = Dock(
    DockContainer(
        "vertical", [DockPane(title, ui, closable=True) for title, ui in PANES.items()]
    ),
    id="inspector",
    tab_bar_height=26,
    pane_margin=8,
)


def in_zone(zone, ui):
    """a click anywhere in the part of the window moves the keyboard focus there"""

    def run():
        if pointer_pressed() and ui_contains_pointer():
            focus_zone.value = zone
        ui()

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
    return (2, (r, g, b, round(255 * alpha)))


def update_func(ctx):
    if not started.value:
        context.append(ctx)
        restore(ctx.load_state())
        apply_style()
        started.value = True

    keyboard()
    action_keys()
    edge = (
        1,
        (127, 127, 127, 60),
    )  # a hairline so the rounded corner reads against the content
    left_panel(
        "sidebar",
        in_zone("sidebar", sidebar),
        expanded=show_sidebar,
        default_size=215,
        min_size=200,
        inner_margin=12,
        show_separator_line=False,
        rounding=12,
        stroke=zone_stroke("sidebar", edge),
    )
    top_panel(
        "toolbar",
        toolbar,
        inner_margin=8,
        show_separator_line=False,
        rounding=12,
        stroke=edge,
    )
    bottom_panel(
        "path",
        path_bar,
        inner_margin=8,
        show_separator_line=False,
        rounding=12,
        stroke=edge,
    )
    right_panel(
        "dock",
        in_zone("dock", lambda: dock_area(dock)),
        expanded=show_dock,
        default_size=300,
        min_size=220,
        inner_margin=6,
        show_separator_line=False,
        stroke=zone_stroke("dock", (0, (0, 0, 0, 0))),
    )
    frame(
        in_zone("content", content),
        inner_margin=16,
        stroke=zone_stroke("content", (0, (0, 0, 0, 0))),
    )
    if renaming.value:
        rename_dialog()
    if palette_open.value:
        action_palette()
    ctx.save_state(snapshot())  # written to disk by pyegui, see persist_id below


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
