"""A mini Finder written only with the Python API of pyegui.

Sidebar with favorites, toolbar with back / forward, list, icon and tree views, search, a preview
panel and a path bar. The icons are the ones of the Finder on macOS.

Mouse: click to select, double click to open (folders here, files with their app). In the tree
view, the arrow of a folder shows its contents in the tree.
Keyboard: arrows to select, Enter or Cmd+Down to open, Cmd+Up for the parent folder,
Cmd+[ and Cmd+] to go back and forward, Escape to clear the search and the selection.

    python finder.py [folder]
"""

import sys
from datetime import datetime
from pathlib import Path

from pyegui import *

BACKDROP = {"win32": Backdrop.ACRYLIC, "darwin": Backdrop.LIQUID_GLASS}.get(sys.platform)
MACOS = sys.platform == "darwin"
HOME = Path.home()
ICONS_DIR = Path(__file__).resolve().parent / "icons"  # white SVG icons, tinted to the theme

LIST, ICONS, TREE = 0, 1, 2
LIGHT, DARK, SYSTEM = 0, 1, 2
NO_GLASS, PARTIAL, FULL = 0, 1, 2
ACCENTS = {"Blue": "#3b82f6", "Violet": "#8b5cf6", "Green": "#10b981", "Orange": "#f97316"}

TEXT_SUFFIXES = {".py", ".rs", ".md", ".txt", ".toml", ".rst", ".json", ".yml", ".yaml", ".cfg", ".lock", ".sh", ".js", ".css", ".html"}
IMAGE_SUFFIXES = {".png", ".jpg", ".jpeg", ".gif", ".svg", ".webp"}
EMOJIS = {
    ".py": "🐍", ".rs": "🦀", ".md": "📝", ".txt": "📝", ".rst": "📝", ".pdf": "📕",
    ".png": "🖼", ".jpg": "🖼", ".jpeg": "🖼", ".gif": "🖼", ".svg": "🖼", ".webp": "🖼",
    ".zip": "🗜", ".gz": "🗜", ".toml": "⚙", ".json": "⚙", ".yml": "⚙", ".yaml": "⚙", ".lock": "🔒",
}
KINDS = {
    ".py": "Python script", ".rs": "Rust source", ".md": "Markdown", ".txt": "Plain text",
    ".rst": "reStructuredText", ".pdf": "PDF document", ".png": "PNG image", ".jpg": "JPEG image",
    ".jpeg": "JPEG image", ".svg": "SVG image", ".toml": "TOML", ".json": "JSON", ".lock": "Lock file",
    ".zip": "ZIP archive",
}
TAGS = [("Red", "#ef4444"), ("Orange", "#f97316"), ("Green", "#22c55e"), ("Blue", "#3b82f6"), ("Purple", "#a855f7")]

# state

start = Path(sys.argv[1]).expanduser().resolve() if len(sys.argv) > 1 else HOME
history = [start]
position = Int(0)
selected = Str("")  # path of the selected item
view = Int(LIST)
search = Str("")
show_sidebar = Bool(True)
show_preview = Bool(True)
theme = Int(SYSTEM)
glass = Int(PARTIAL)
accent_name = Str("Blue")
icons_per_row = Int(1)
started = Bool(False)
context = []  # the Context, kept on the first frame

listing_cache = {}  # folder -> entries, read once
tree = TreeView("tree", default_open=False, activatable_dirs=True)
tree_ids = {}  # path -> id of the tree node
tree_paths = {}  # id -> path


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
        "kind": "Folder" if is_dir else KINDS.get(suffix, f"{suffix[1:].upper()} file" if suffix else "Document"),
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
        listing_cache[folder] = sorted(entries, key=lambda e: (not e["is_dir"], e["name"].lower()))
    return listing_cache[folder]


def visible_entries():
    query = search.value.lower()
    return [e for e in listing(current()) if query in e["name"].lower()]


def selected_entry():
    return entry(Path(selected.value)) if selected.value else None


def icon(item, size):
    """the icon of the Finder, or an emoji where the platform gives none"""
    uri = file_icon(str(item["path"]), size)
    if uri:
        image(uri, max_width=size, max_height=size)
    else:
        label(item["emoji"], size=size * 0.8)


# navigation

def go(folder):
    folder = Path(folder)
    if folder == current() or not folder.is_dir():
        return
    del history[position.value + 1:]
    history.append(folder)
    position.value = len(history) - 1
    selected.value = ""
    search.value = ""


def back():
    if position.value > 0:
        position.value -= 1
        selected.value = ""


def forward():
    if position.value < len(history) - 1:
        position.value += 1
        selected.value = ""


def open_item(item):
    if item["is_dir"]:
        go(item["path"])
    else:
        context[0].open_url(item["path"].as_uri())  # with the app of the file


def select(item):
    selected.value = str(item["path"])


def handle(item, response):
    if response.clicked:
        select(item)
    if response.double_clicked:
        open_item(item)


def keyboard():
    if key_pressed("Cmd+ArrowUp"):
        go(current().parent)
    if key_pressed("Cmd+OpenBracket"):
        back()
    if key_pressed("Cmd+CloseBracket"):
        forward()
    if key_pressed("Escape", when_typing=True):
        search.value = ""
        selected.value = ""
    if view.value == TREE:
        return  # the tree view has its own keyboard navigation, its Enter opens like the Open button
    item = selected_entry()
    if item and (key_pressed("Enter") or key_pressed("Cmd+ArrowDown")):
        open_item(item)

    entries = visible_entries()
    if not entries:
        return
    paths = [str(e["path"]) for e in entries]
    index = paths.index(selected.value) if selected.value in paths else -1
    per_row = icons_per_row.value if view.value == ICONS else 1
    step = 0
    if key_pressed("ArrowDown"):
        step = per_row
    if key_pressed("ArrowUp"):
        step = -per_row
    if view.value == ICONS and key_pressed("ArrowRight"):
        step = 1
    if view.value == ICONS and key_pressed("ArrowLeft"):
        step = -1
    if step:
        index = min(max(index + step, 0), len(entries) - 1) if index >= 0 else 0
        select(entries[index])


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
        ctx.set_glass_style(accent=ACCENTS[accent_name.value], mode="full" if glass.value == FULL else "partial")
    else:
        ctx.set_accent(ACCENTS[accent_name.value])


def settings_menu():
    before = (theme.value, glass.value, accent_name.value)
    toggle_value(show_sidebar, "Sidebar")
    toggle_value(show_preview, "Preview")
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
        if image_button_clicked(svg(icon_name), name, size=16, tint=(255, 255, 255) if on else accent,
                                selected=on, frame=False):
            go(path)

    weak("Favorites")
    place("home", HOME.name, HOME)
    place("desktop", "Desktop", HOME / "Desktop")
    place("document", "Documents", HOME / "Documents")
    place("download", "Downloads", HOME / "Downloads")
    place("folder", "pyegui", Path(__file__).resolve().parent.parent)
    add_space(12)
    weak("Locations")
    place("drive", "Macintosh HD" if MACOS else "Root", Path(current().anchor))
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
    return path.name or ("Macintosh HD" if MACOS else str(path))


def ancestors_menu():
    """the current folder then its parents, like the title menu of the Finder"""
    for path in [current(), *current().parents]:
        def row(path=path):
            # the check mark of the current folder, an invisible one keeps the others aligned
            colored_label((127, 127, 127, 255) if path == current() else (0, 0, 0, 0), "✓")
            uri = file_icon(str(path), 16)
            clicked = image_button_clicked(uri, place_name(path)) if uri else button_clicked(place_name(path))
            if clicked:
                go(path)
                close_menu()

        horizontal(row)


def toolbar():
    tint = icon_tint()

    def bar():
        if MACOS and not show_sidebar.value:
            add_space(64)  # the window buttons are over the toolbar when the sidebar is hidden
        if image_button_clicked(svg("chevron-left"), size=16, tint=tint):
            back()
        if image_button_clicked(svg("chevron-right"), size=16, tint=tint):
            forward()
        add_space(6)
        menu_button(f"{place_name(current())}  ▾", ancestors_menu, size=14)
        add_space(max(available_width() - 640, 8))
        for mode, name, text in [(LIST, "list", "List"), (ICONS, "grid", "Icons"), (TREE, "tree", "Tree")]:
            on = view.value == mode
            if image_button_clicked(svg(name), text, size=15, tint=(255, 255, 255) if on else tint, selected=on):
                view.value = mode
        text_edit_singleline(search, hint_text="Search")
        menu_image_button(svg("gear"), settings_menu, size=18, tint=tint)

    horizontal(bar)


# views

def list_view():
    entries = visible_entries()

    def cell(row, col):
        item = entries[row]
        if col == 0:
            def name():
                icon(item, 18)
                handle(item, selectable_label_response(selected.value == str(item["path"]), item["name"]))
            horizontal(name)
        elif col == 1:
            weak(item["modified"])
        elif col == 2:
            weak(item["size"])
        else:
            weak(item["kind"])

    table_ui(["Name", "Date Modified", "Size", "Kind"], len(entries), cell,
             id=f"list {current()}", column_widths=[290, 165, 80, "remainder"], striped=True)


def icon_view():
    entries = visible_entries()
    per_row = max(int(available_width() // 120), 1)
    icons_per_row.value = per_row

    def rows():
        for start_index in range(0, len(entries), per_row):
            row = entries[start_index:start_index + per_row]

            def column(i, row=row):
                if i >= len(row):
                    return
                item = row[i]

                def tile():
                    icon(item, 56)
                    name = item["name"] if len(item["name"]) <= 16 else item["name"][:15] + "…"
                    handle(item, selectable_label_response(selected.value == str(item["path"]), name))

                vertical_centered(tile)

            columns(per_row, column)
            add_space(10)

    scroll_area_vertical(rows)


def tree_node(item):
    path = item["path"]
    if path not in tree_ids:
        tree_ids[path] = len(tree_ids)
        tree_paths[tree_ids[path]] = path
    node_id = tree_ids[path]
    # the icon of the Finder, or an emoji where the platform gives none
    icon_uri = file_icon(str(path), 16)
    text = item["name"] if icon_uri else f"{item['emoji']}  {item['name']}"
    if not item["is_dir"]:
        return TreeNode(node_id, text, icon=icon_uri)
    # the children of a folder are read only once it is opened
    children = [tree_node(child) for child in listing(path)] if tree.is_open(node_id) else []
    return TreeNode(node_id, text, children, icon=icon_uri)


def tree_view_ui():
    def nodes():
        query = search.value.lower()
        tree_view(tree, [tree_node(e) for e in listing(current()) if query in e["name"].lower()])
        if tree.selected:
            selected.value = str(tree_paths[tree.selected[0]])
        for node_id in tree.activated:
            # a double click or Enter opens like the Open button: into a folder, a file with its app.
            # The arrow of a folder opens it in the tree
            item = entry(tree_paths[node_id])
            if item:
                open_item(item)

    scroll_area_both(nodes)


def content():
    if not listing(current()) or (search.value and not visible_entries()):
        weak("No items" if not search.value else f"No items match “{search.value}”")
        return
    [list_view, icon_view, tree_view_ui][view.value]()


# preview

def preview():
    item = selected_entry()
    if item is None:
        weak("Select an item to see its preview")
        return

    def head():
        icon(item, 96)
        strong(item["name"])
        weak(f"{item['kind']} · {item['size']}")

    vertical_centered(head)
    add_space(8)
    separator()

    def rows():
        for name, value in [("Kind", item["kind"]), ("Size", item["size"]), ("Modified", item["modified"]),
                            ("Where", str(item["path"].parent).replace(str(HOME), "~"))]:
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


# path bar

def path_bar():
    def crumbs():
        parts = [Path(current().anchor)] + list(reversed(current().parents))[1:] + [current()]
        for i, part in enumerate(parts):
            if i:
                weak("›")
            if link_clicked(("Macintosh HD" if MACOS else "Root") if i == 0 else part.name):
                go(part)
        add_space(max(available_width() - 120, 8))
        count = len(visible_entries())
        weak(f"{count} item{'s' if count != 1 else ''}")

    horizontal(crumbs)


def update_func(ctx):
    if not started.value:
        context.append(ctx)
        apply_style()
        started.value = True

    keyboard()
    left_panel("sidebar", sidebar, expanded=show_sidebar, default_size=215, min_size=200, inner_margin=12,
               show_separator_line=False)
    top_panel("toolbar", toolbar, inner_margin=8, show_separator_line=False)
    bottom_panel("path", path_bar, inner_margin=8, show_separator_line=False)
    right_panel("preview", preview, expanded=show_preview, default_size=280, inner_margin=14,
                show_separator_line=False)
    content()


if __name__ == "__main__":
    run_native("Finder", update_func, inner_width=1180, inner_height=720, backdrop=BACKDROP,
               fullsize_content=True)
