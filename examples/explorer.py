"""File explorer with a glass style: native backdrop (acrylic on Windows, liquid glass on macOS), sidebar with a tree view and a dock of tabs.

Double click a file in the tree to open it in a new tab.
"""

import sys
from datetime import datetime
from pathlib import Path

from pyegui import *

ROOT = Path(__file__).resolve().parent.parent
SKIPPED_DIRS = {
    ".git",
    ".venv",
    "venv",
    "target",
    "__pycache__",
    "_build",
    "node_modules",
}
BINARY_SUFFIXES = {
    ".png",
    ".jpg",
    ".jpeg",
    ".gif",
    ".ico",
    ".pyd",
    ".so",
    ".dll",
    ".whl",
}
MAX_FILE_SIZE = 200_000

# acrylic on Windows, liquid glass on macOS, no backdrop elsewhere
BACKDROP = {"win32": "acrylic", "darwin": "liquid_glass"}.get(sys.platform)

LIGHT, DARK, SYSTEM = 0, 1, 2

# state

started = Bool(False)
theme = Int(LIGHT)
show_sidebar = Bool(True)
status = Str("Double click a file to open it")

paths = []  # node id -> path
contents = {}  # path -> Str shown in its tab


# tree


def build_nodes(directory):
    nodes = []
    entries = sorted(directory.iterdir(), key=lambda p: (p.is_file(), p.name.lower()))

    for entry in entries:
        if entry.is_dir() and (
            entry.name in SKIPPED_DIRS or entry.name.startswith(".")
        ):
            continue

        node_id = len(paths)
        paths.append(entry)

        if entry.is_dir():
            nodes.append(TreeNode(node_id, entry.name, build_nodes(entry)))
        else:
            nodes.append(TreeNode(node_id, entry.name))
    return nodes


paths.append(ROOT)
tree_nodes = [TreeNode(0, ROOT.name, build_nodes(ROOT))]
tree = TreeView("files")
# directories are closed, except the root
for node_id, path in enumerate(paths):
    tree.set_open(node_id, path == ROOT)


# dock


def welcome_ui():
    heading("Explorer")
    label("A pyegui demo with a glass style and a native backdrop.")
    add_space(8)

    def help_card():
        strong("How to use")
        label("Double click a file in the sidebar to open it in a tab.")
        label("Drag the tabs to split the dock, close them with the cross.")
        label("Change the theme in the top bar, the backdrop follows.")

    frame(
        help_card,
        fill=(255, 255, 255, 35),
        stroke=(1, (255, 255, 255, 80)),
        rounding=14,
        inner_margin=12,
    )


def info_ui():
    heading("Details")
    selected = [paths[i] for i in tree.selected if i < len(paths)]

    if not selected:
        weak("Select a file or a directory")
        return

    path = selected[0]
    stat = path.stat()

    def rows():
        label("Name")
        strong(path.name)
        end_row()
        label("Type")
        label("Directory" if path.is_dir() else (path.suffix or "File"))
        end_row()
        label("Path")
        label(str(path.relative_to(ROOT.parent)))
        end_row()
        if path.is_file():
            label("Size")
            label(f"{stat.st_size:,} bytes")
            end_row()
        label("Modified")
        label(datetime.fromtimestamp(stat.st_mtime).strftime("%Y-%m-%d %H:%M"))
        end_row()

    grid("details", rows, num_columns=2, spacing=(16, 6))

    if len(selected) > 1:
        weak(f"and {len(selected) - 1} more selected")


dock = Dock(
    DockContainer(
        "horizontal",
        [
            DockContainer("tabs", [DockPane("Welcome", welcome_ui)]),
            DockPane("Details", info_ui),
        ],
    ),
    glass=True,
)


def read_file(path):
    if path.suffix.lower() in BINARY_SUFFIXES:
        return None
    if path.stat().st_size > MAX_FILE_SIZE:
        return None
    return path.read_text(encoding="utf-8", errors="replace")


def editor_ui(path):
    # markdown files are rendered, with a toggle to show their source
    show_source = Bool(path.suffix.lower() != ".md")

    def ui():
        text = contents[path]
        if text is None:
            weak("This file is binary or too large to be shown.")
            return

        def header():
            small(str(path.relative_to(ROOT)))
            if path.suffix.lower() == ".md":
                toggle_value(show_source, "Source")

        horizontal(header)

        if show_source.value:
            scroll_area_vertical(lambda: code_editor(text))
        else:
            markdown(text.value, scrollable=True, id=str(path))

    return ui


def open_file(path):
    title = str(path.relative_to(ROOT)).replace("\\", "/")
    if title in dock.titles:
        status.value = f"{title} is already open"
        return

    text = read_file(path)
    contents[path] = Str(text) if text is not None else None
    dock.add_pane(DockPane(title, editor_ui(path), closable=True))
    status.value = f"Opened {title}"


# theme


def apply_theme(ctx):
    if theme.value == LIGHT:
        ctx.set_light_theme()
    elif theme.value == DARK:
        ctx.set_dark_theme()
    else:
        ctx.set_system_theme()

    # the tint of the acrylic follows the theme of Windows, so it is forced here.
    # Liquid glass follows the theme of the app on its own
    if BACKDROP == "acrylic":
        ctx.set_backdrop(
            "acrylic" + {LIGHT: "_light", DARK: "_dark", SYSTEM: ""}[theme.value]
        )


def setup(ctx):
    ctx.set_glass_style()
    ctx.set_style(selection_color=(59, 130, 246, 150), heading_size=20)
    apply_theme(ctx)


# app


def top_bar(ctx):
    def bar():
        toggle_value(show_sidebar, "Sidebar")
        separator()
        before = theme.value
        selectable_value(theme, LIGHT, "Light")
        selectable_value(theme, DARK, "Dark")
        selectable_value(theme, SYSTEM, "System")
        if theme.value != before:
            apply_theme(ctx)

    horizontal(bar)


def sidebar():
    strong("Files")
    separator()

    def files():
        tree_view(tree, tree_nodes)

        for node_id in tree.activated:
            path = paths[node_id]
            if path.is_file():
                open_file(path)

    scroll_area_vertical(files)


def update_func(ctx):
    if not started.value:
        setup(ctx)
        started.value = True

    top_panel("top", lambda: top_bar(ctx), inner_margin=8, show_separator_line=False)
    bottom_panel(
        "status", lambda: small(status.value), inner_margin=6, show_separator_line=False
    )
    left_panel(
        "sidebar",
        sidebar,
        expanded=show_sidebar,
        default_size=240,
        inner_margin=10,
        fill=(0, 0, 0, 40) if ctx.is_dark_theme else (255, 255, 255, 30),
        show_separator_line=False,
    )

    dock_area(dock)


if __name__ == "__main__":
    run_native(
        "Explorer",
        update_func,
        inner_width=1200,
        inner_height=760,
        backdrop=BACKDROP,
    )
