"""A mini Finder written only with the Python API of pyegui.

Sidebar with favorites, toolbar with back / forward, list and icon views, search, a preview panel
and a path bar. Click an item to select it, click a selected folder to open it.

    python finder.py [folder]
"""

import sys
from datetime import datetime
from pathlib import Path

from pyegui import *

BACKDROP = {"win32": Backdrop.ACRYLIC, "darwin": Backdrop.LIQUID_GLASS}.get(sys.platform)
MACOS = sys.platform == "darwin"
HOME = Path.home()

LIST, ICONS = 0, 1
TEXT_SUFFIXES = {".py", ".rs", ".md", ".txt", ".toml", ".rst", ".json", ".yml", ".yaml", ".cfg", ".lock", ".sh", ".js", ".css", ".html"}
IMAGE_SUFFIXES = {".png", ".jpg", ".jpeg", ".gif", ".svg", ".webp"}
ICONS_BY_SUFFIX = {
    ".py": "🐍", ".rs": "🦀", ".md": "📝", ".txt": "📝", ".rst": "📝", ".pdf": "📕",
    ".png": "🖼", ".jpg": "🖼", ".jpeg": "🖼", ".gif": "🖼", ".svg": "🖼", ".webp": "🖼",
    ".zip": "🗜", ".gz": "🗜", ".toml": "⚙", ".json": "⚙", ".yml": "⚙", ".yaml": "⚙", ".lock": "🔒",
    ".mp3": "🎵", ".wav": "🎵", ".mp4": "🎬", ".mov": "🎬",
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
selected = Str("")
view = Int(LIST)
search = Str("")
show_preview = Bool(True)
started = Bool(False)
listing_cache = {}  # folder -> list of entries, read once


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
    return {
        "path": path,
        "name": path.name,
        "is_dir": is_dir,
        "icon": "📁" if is_dir else ICONS_BY_SUFFIX.get(path.suffix.lower(), "📄"),
        "kind": "Folder" if is_dir else KINDS.get(path.suffix.lower(), f"{path.suffix[1:].upper()} file" if path.suffix else "Document"),
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


def click(item):
    # a second click on a selected folder opens it, like a double click
    if selected.value == item["name"] and item["is_dir"]:
        go(item["path"])
    else:
        selected.value = item["name"]


def selected_entry():
    return next((e for e in listing(current()) if e["name"] == selected.value), None)


# sidebar

def sidebar():
    if MACOS:
        add_space(26)  # the window buttons are over the sidebar

    def place(icon, name, path):
        path = Path(path)
        if path.is_dir() and selectable_label_clicked(current() == path, f"{icon}  {name}"):
            go(path)

    weak("Favorites")
    place("🏠", HOME.name, HOME)
    place("🖥", "Desktop", HOME / "Desktop")
    place("📄", "Documents", HOME / "Documents")
    place("⬇", "Downloads", HOME / "Downloads")
    place("🧩", "pyegui", Path(__file__).resolve().parent.parent)
    add_space(12)
    weak("Locations")
    place("💽", "Macintosh HD" if MACOS else "Root", Path(current().anchor))
    add_space(12)
    weak("Tags")
    for name, color in TAGS:
        def tag(name=name, color=color):
            colored_label(color, "●")
            label(name)
        horizontal(tag)


# toolbar

def toolbar():
    def bar():
        if small_button_clicked("‹"):
            back()
        if small_button_clicked("›"):
            forward()
        add_space(6)
        strong(current().name or str(current()))
        add_space(max(available_width() - 330, 8))
        if selectable_label_clicked(view.value == LIST, "☰ List"):
            view.value = LIST
        if selectable_label_clicked(view.value == ICONS, "▦ Icons"):
            view.value = ICONS
        toggle_value(show_preview, "Preview")
        text_edit_singleline(search, hint_text="🔍 Search")

    horizontal(bar)


# views

def list_view():
    entries = visible_entries()

    def cell(row, col):
        item = entries[row]
        if col == 0:
            if selectable_label_clicked(selected.value == item["name"], f"{item['icon']}  {item['name']}"):
                click(item)
        elif col == 1:
            weak(item["modified"])
        elif col == 2:
            weak(item["size"])
        else:
            weak(item["kind"])

    table_ui(["Name", "Date Modified", "Size", "Kind"], len(entries), cell,
             id=f"list {current()}", column_widths=[320, 170, 80, "remainder"], striped=True)


def icon_view():
    entries = visible_entries()
    per_row = max(int(available_width() // 120), 1)

    def rows():
        for start_index in range(0, len(entries), per_row):
            row = entries[start_index:start_index + per_row]

            def column(i, row=row):
                if i >= len(row):
                    return
                item = row[i]

                def tile():
                    heading(item["icon"])
                    name = item["name"] if len(item["name"]) <= 16 else item["name"][:15] + "…"
                    if selectable_label_clicked(selected.value == item["name"], name):
                        click(item)

                vertical_centered(tile)

            columns(per_row, column)
            add_space(10)

    scroll_area_vertical(rows)


def content():
    if not visible_entries():
        weak("No items" if not search.value else f"No items match “{search.value}”")
        return
    if view.value == LIST:
        list_view()
    else:
        icon_view()


# preview

def preview():
    item = selected_entry()
    if item is None:
        weak("Select an item to see its preview")
        return

    def head():
        heading(item["icon"])
        strong(item["name"])
        weak(f"{item['kind']} · {item['size']}")

    vertical_centered(head)
    add_space(8)
    separator()

    def info(name, value):
        weak(name)
        label(value)
        end_row()

    def rows():
        info("Kind", item["kind"])
        info("Size", item["size"])
        info("Modified", item["modified"])
        info("Where", str(item["path"].parent).replace(str(HOME), "~"))

    grid("info", rows, num_columns=2, spacing=(12, 6))
    add_space(8)

    suffix = item["path"].suffix.lower()
    if suffix in IMAGE_SUFFIXES:
        image(f"file://{item['path']}", max_width=260, max_height=220)
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
            go(item["path"])


# path bar

def path_bar():
    def crumbs():
        parts = [Path(current().anchor)] + [p for p in reversed(current().parents)][1:] + [current()]
        for i, part in enumerate(parts):
            if i:
                weak("›")
            name = ("Macintosh HD" if MACOS else "Root") if i == 0 else part.name
            if link_clicked(name):
                go(part)
        add_space(max(available_width() - 120, 8))
        count = len(visible_entries())
        weak(f"{count} item{'s' if count != 1 else ''}")

    horizontal(crumbs)


def update_func(ctx):
    if not started.value:
        ctx.set_glass_style()
        # heading() only draws the big file icons: the Python API has no text size per label
        ctx.set_style(heading_size=40)
        started.value = True

    left_panel("sidebar", sidebar, default_size=215, min_size=200, inner_margin=12, show_separator_line=False)
    top_panel("toolbar", toolbar, inner_margin=8, show_separator_line=False)
    bottom_panel("path", path_bar, inner_margin=8, show_separator_line=False)
    right_panel("preview", preview, expanded=show_preview, default_size=280, inner_margin=14,
                show_separator_line=False)
    content()


if __name__ == "__main__":
    run_native("Finder", update_func, inner_width=1180, inner_height=720, backdrop=BACKDROP,
               fullsize_content=True)
