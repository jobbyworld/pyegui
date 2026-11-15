"""A markdown viewer on the model of finder.py: a sidebar with the tree of the markdown files of a
folder, the document in the middle and, in a dock, its outline and its source.

    python examples/markdown_finder.py [folder]

The folder is this repository by default. The layout is the unified one of finder.py, like the
Claude app: the sidebar runs along the whole height of the window, and it and the body are cut in
three bands of the same heights (top, content, bottom). The document is read again when an editor
saves it. The window, the theme, the folder, the open folders and the document are kept between
runs.
"""

import re
import sys
from pathlib import Path
from urllib.parse import unquote, urlsplit

from pyegui import (
    Backdrop,
    Bool,
    Dock,
    DockContainer,
    DockPane,
    FileTreeView,
    FileWatcher,
    Float,
    Int,
    Str,
    add_enabled,
    add_space,
    available_width,
    bottom_panel,
    button_clicked,
    close_menu,
    code,
    dock_area,
    file_tree_view,
    frame,
    horizontal,
    horizontal_centered,
    image,
    image_button_clicked,
    label,
    left_panel,
    markdown,
    menu_image_button,
    path_breadcrumb,
    path_combo,
    run_native,
    scroll_area_vertical,
    scroll_to_cursor,
    segmented_control,
    selectable_label_clicked,
    set_width,
    small,
    strong,
    text_edit_singleline,
    top_panel,
    weak,
)

HERE = Path(__file__).resolve().parent
ICONS_DIR = HERE / "icons"
MACOS = sys.platform == "darwin"
BACKDROP = {"win32": Backdrop.ACRYLIC, "darwin": Backdrop.LIQUID_GLASS}.get(sys.platform)

# the unified layout of finder.py: the bands and the gap
TOP_H, BOTTOM_H = 52, 44  # the heights of the top and bottom bands, in the sidebar and the body
GAP = 8  # around the dock and between its panes
LINE = (1, (127, 127, 127, 70))  # the lines between the bands and between the sidebar and the body
CLEAR = (0, 0, 0, 0)
EDGE = (1, (127, 127, 127, 60))

LIGHT, DARK, SYSTEM = 0, 1, 2
NO_GLASS, PARTIAL, FULL = 0, 1, 2

root = (Path(sys.argv[1]).expanduser() if len(sys.argv) > 1 else HERE.parent).resolve()

# the state: created once, outside the update function
watcher = FileWatcher()
tree = FileTreeView("md_tree", root, watcher=watcher, multi_select=False)
tree.filter = ".md"  # only the markdown files, the folders always
document = Str("")  # the path of the document shown
history = []  # the documents shown, for back and forward
position = Int(-1)
search = Str("")
SCROLL_FRAMES = 6
scroll_target = Int(-1)  # the section to scroll to at the next frame, from a click in the outline
scroll_frames = Int(0)  # how many frames scroll_target is still applied
pending_anchor = Str("")  # a heading to scroll to once the document of a link is loaded
parsed_cache = {}
show_sidebar = Bool(True)
show_dock = Bool(True)
# the look, by default the settings of finder.py
theme = Int(LIGHT)
glass = Int(FULL)
backdrop_blur = Float(32)
started = Bool(False)
context = []
cache = {"path": None, "text": ""}
last_selection = []


def svg(name):
    return (ICONS_DIR / f"{name}.svg").as_uri()


def icon_tint():
    return (225, 227, 233) if context[0].is_dark_theme else (70, 73, 82)


def toggle_tint(on):
    """the icon of a toggle button: white on the accent color when on"""
    return (255, 255, 255) if on else icon_tint()


# the document


def open_document(path, remember=True):
    path = str(path)
    if path == document.value or not path.lower().endswith(".md"):
        return
    if document.value:
        watcher.unwatch(document.value)
    document.value = path
    watcher.watch(path)
    watcher.changed(path)  # starts counting the changes
    cache["path"] = None
    if remember:
        del history[position.value + 1 :]
        history.append(path)
        position.value = len(history) - 1


def move(step):
    """back (-1) or forward (+1) in the documents shown"""
    target = position.value + step
    if 0 <= target < len(history):
        position.value = target
        open_document(history[target], remember=False)
        tree.reveal(history[target])


def text():
    """the text of the document, read again when it changed on the disk"""
    path = document.value
    if not path:
        return ""
    if cache["path"] != path or watcher.changed(path):
        try:
            cache["text"] = Path(path).read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError) as error:
            cache["text"] = f"*Cannot read this file: {error}*"
        cache["path"] = path
    return cache["text"]


def parse(body):
    """the headings of a text, (level, title), outside the code blocks, and the text cut into
    sections: the first one is what comes before the first heading, section i + 1 starts with
    heading i"""
    found, sections, in_code = [], [[]], False
    for line in body.splitlines():
        if line.startswith("```"):
            in_code = not in_code
        elif not in_code and line.startswith("#"):
            level = len(line) - len(line.lstrip("#"))
            title = line[level:].strip()
            if title and level <= 6 and line[level:level + 1] in (" ", ""):
                found.append((level, title))
                sections.append([])
        sections[-1].append(line)
    return found, ["\n".join(lines) for lines in sections]


def parsed():
    """the headings and sections of the document, parsed again only when its text changed"""
    body = text()
    if parsed_cache.get("text") is not body:
        parsed_cache.update(text=body, result=parse(body))
    return parsed_cache["result"]


def headings():
    return parsed()[0]


# the sidebar: a top band with the settings, the tree, a bottom band with the count


def settings_menu():
    def choices(title, items, current, setter):
        small(title)
        for value, name in items:
            if selectable_label_clicked(current == value, name):
                setter(value)
                close_menu()

    choices("Thème", [(LIGHT, "Clair"), (DARK, "Sombre"), (SYSTEM, "Système")], theme.value, set_theme)
    choices("Vitre", [(NO_GLASS, "Sans vitre"), (PARTIAL, "Partielle"), (FULL, "Complète")], glass.value, set_glass)


def set_theme(value):
    theme.value = value
    apply_style()


def set_glass(value):
    glass.value = value
    apply_style()


def sidebar_top():
    def row():
        add_space(max(available_width() - 28, 0))
        menu_image_button(svg("gear"), settings_menu, size=16, tint=icon_tint())

    horizontal_centered(row)


def sidebar_bottom():
    def row():
        image(svg("folder"), size=16, tint=icon_tint())
        strong(root.name or str(root))
        add_space(max(available_width() - 116, 4))  # the width of the control
        # light, dark or the theme of the system, as in finder.py
        if segmented_control(
            theme,
            [(svg("sun"), None), (svg("moon"), None), (svg("theme-system"), None)],
            size=14,
            tint=icon_tint(),
        ):
            apply_style()

    horizontal_centered(row)


def sidebar():
    top_panel("side_top", sidebar_top, exact_size=TOP_H, resizable=False, fill=CLEAR, inner_margin=(12, 0, 12, 0), show_separator_line=False)
    bottom_panel("side_bottom", sidebar_bottom, exact_size=BOTTOM_H, resizable=False, fill=CLEAR, inner_margin=(12, 0, 12, 0), show_separator_line=False)
    tree.filter = search.value or ".md"  # the search narrows the files, else every markdown file
    # the tree scrolls between the two bands when many folders are open
    frame(lambda: scroll_area_vertical(lambda: file_tree_view(tree)), inner_margin=(8, 0, 8, 8))
    # a markdown file selected in the tree is shown at once
    selected = tree.selected
    if selected != last_selection:
        last_selection[:] = selected
        if len(selected) == 1 and Path(selected[0]).is_file():
            open_document(selected[0])


# the body: a top band with the title, the document and its dock, a bottom band with the path


def toolbar():
    tint = icon_tint()

    def row():
        if MACOS and not show_sidebar.value:
            add_space(64)  # the window buttons are over the toolbar when the sidebar is hidden
        add_enabled(position.value > 0, lambda: image_button_clicked(svg("chevron-left"), size=16, tint=tint) and move(-1))
        add_enabled(
            position.value < len(history) - 1,
            lambda: image_button_clicked(svg("chevron-right"), size=16, tint=tint) and move(1),
        )
        add_space(6)
        if document.value:
            # the name of the document; under the mouse a menu of its folders
            chosen = path_combo(document.value, size=15)
            if chosen:
                tree.reveal(chosen)
        else:
            weak("Choisis un document dans la barre latérale")
        add_space(max(available_width() - 240, 4))

        def search_field():
            set_width(220)
            text_edit_singleline(search, hint_text="Filtrer les fichiers")

        horizontal_centered(search_field)

    horizontal_centered(row)


def path_bar():
    def row():
        if image_button_clicked(svg("sidebar"), size=16, tint=toggle_tint(show_sidebar.value), selected=show_sidebar.value):
            show_sidebar.value = not show_sidebar.value
        add_space(4)
        right = 150
        if document.value:
            path_breadcrumb(document.value, max_width=available_width() - right, on_click=reveal_or_open)
        add_space(max(available_width() - right, 4))
        words = len(text().split())
        weak(f"{words} mots")
        add_space(max(available_width() - 36, 4))
        if image_button_clicked(svg("dock"), size=16, tint=toggle_tint(show_dock.value), selected=show_dock.value):
            show_dock.value = not show_dock.value

    horizontal_centered(row)


def reveal_or_open(path):
    """a click in the path: a folder is opened in the tree, a document shown"""
    if Path(path).is_file():
        open_document(path)
    tree.reveal(path)


def viewer():
    if not document.value:
        weak("Aucun document")
        return
    if pending_anchor.value:
        scroll_to_anchor(pending_anchor.value)
        pending_anchor.value = ""

    def sections():
        # one markdown per section, so that a click in the outline can scroll to its heading
        for i, section in enumerate(parsed()[1]):
            if i == scroll_target.value:
                # asked again for a few frames: the first layout of a document may still change
                # the heights of the sections above, and the heading would end up elsewhere
                scroll_to_cursor("top")
                scroll_frames.value -= 1
                if scroll_frames.value <= 0:
                    scroll_target.value = -1
            if section.strip():
                # the links are not opened by the browser: follow_link decides
                url = markdown(section, open_links=False)
                if url:
                    follow_link(url)

    scroll_area_vertical(sections)


def slug(title):
    """the anchor of a heading, like GitHub: lower case, no punctuation, spaces as dashes"""
    title = re.sub(r"[^\w\- ]", "", title.lower(), flags=re.UNICODE)
    return title.strip().replace(" ", "-")


def scroll_to_anchor(anchor):
    """scrolls the document to the heading whose anchor is given; False if there is none"""
    anchor = unquote(anchor).lower()
    for i, (_, title) in enumerate(headings()):
        if slug(title) == anchor:
            scroll_target.value = i + 1
            scroll_frames.value = SCROLL_FRAMES
            return True
    return False


def follow_link(url):
    """a link clicked in the document: a markdown file opens here (at its heading if the link has
    #anchor), #anchor scrolls this document, another file opens with its app, the web in the
    browser"""
    parts = urlsplit(url)
    if parts.scheme in ("http", "https", "mailto"):
        context[0].open_url(url)
        return
    path = unquote(parts.path)
    if parts.scheme == "file":
        target = Path(path)
    elif not path:  # #anchor: in this document
        scroll_to_anchor(parts.fragment)
        return
    else:
        target = (Path(document.value).parent / path).resolve()
    if target.is_dir() and (target / "README.md").is_file():
        target = target / "README.md"
    if target.is_file() and target.suffix.lower() == ".md":
        open_document(target)
        tree.reveal(str(target))
        pending_anchor.value = parts.fragment  # scrolled to once the new text is parsed
    elif target.exists():
        context[0].open_url(target.as_uri())  # with its app


def outline():
    """the headings of the document: a click scrolls the document to it"""
    for i, (level, title) in enumerate(headings()):

        def row(i=i, level=level, title=title):
            add_space(10 * (level - 1))
            # the title as read: without the marks of the markdown (code, bold, italic)
            if selectable_label_clicked(False, re.sub(r"[`*]", "", title)):
                scroll_target.value = i + 1  # section i + 1 starts with heading i
                scroll_frames.value = SCROLL_FRAMES

        horizontal(row)


def source():
    code(text())


dock = Dock(
    DockContainer(
        "horizontal",
        [
            DockPane("Document", viewer, hide_tab=True, central=True),
            DockContainer(
                "vertical",
                [
                    DockPane("Plan", lambda: scroll_area_vertical(outline), closable=True),
                    DockPane("Source", lambda: scroll_area_vertical(source), closable=True),
                ],
            ),
        ],
    ),
    id="markdown_finder",
    tab_bar_height=26,
    pane_margin=8,
    gap_width=GAP,
    pane_stroke=EDGE,
    active_tab_stroke=EDGE,
    inactive_tab_stroke=EDGE,
    resize_stroke=(1, CLEAR),
)
default_layout = dock.save()
default_layout["shares"] = [3.0, 1.0]  # the document takes three quarters of the width
dock.load(default_layout)


# the look and the saved state


def apply_style():
    ctx = context[0]
    {LIGHT: ctx.set_light_theme, DARK: ctx.set_dark_theme, SYSTEM: ctx.set_system_theme}[theme.value]()
    ctx.reset_style()
    if glass.value != NO_GLASS:
        # the glass of finder.py: the document and the docks are clear glass
        ctx.set_glass_style(mode="full" if glass.value == FULL else "partial", opacity=1.2, central=0.3, dock=0.3)
    ctx.set_central_margin(0)  # the gaps are those of the dock and of the frame around it
    ctx.set_backdrop_blur(backdrop_blur.value)


def snapshot():
    return {
        "theme": theme.value,
        "glass": glass.value,
        "show_sidebar": show_sidebar.value,
        "show_dock": show_dock.value,
        "root": str(root),
        "document": document.value,
        "tree_open": tree.open_folders,
        "dock": dock.save(),
    }


def restore(saved):
    if not isinstance(saved, dict):
        return
    theme.value = saved.get("theme", theme.value)
    glass.value = saved.get("glass", glass.value)
    show_sidebar.value = saved.get("show_sidebar", show_sidebar.value)
    show_dock.value = saved.get("show_dock", show_dock.value)
    if saved.get("dock"):
        dock.load(saved["dock"])  # a layout that no longer fits is ignored
    if saved.get("root") == str(root):  # the same folder as the last time: its tree and document
        tree.open_folders = [p for p in saved.get("tree_open", []) if isinstance(p, str)]
        if Path(saved.get("document") or "").is_file():
            open_document(saved["document"])
            tree.reveal(saved["document"])


def first_document():
    """the README of the folder, else its first markdown file"""
    for name in ("README.md", "readme.md", "Readme.md"):
        if (root / name).is_file():
            return root / name
    return next(iter(sorted(root.glob("*.md"))), None)


def update_func(ctx):
    if not started.value:
        context.append(ctx)
        restore(ctx.load_state())
        if not document.value and first_document():
            open_document(first_document())
        apply_style()
        started.value = True

    left_panel(
        "sidebar",
        sidebar,
        expanded=show_sidebar,
        default_size=260,
        min_size=180,
        inner_margin=0,
        outer_margin=0,
        rounding=0,
        show_separator_line=True,
        separator_stroke=LINE,
    )
    top_panel("toolbar", toolbar, exact_size=TOP_H, resizable=False, inner_margin=(8, 0, 12, 0), rounding=0, show_separator_line=True, separator_stroke=LINE)
    bottom_panel("path", path_bar, exact_size=BOTTOM_H, resizable=False, inner_margin=(8, 0, 12, 0), rounding=0, show_separator_line=True, separator_stroke=LINE)
    # one gap everywhere: around the dock (against the lines and the border) and between its panes
    frame(lambda: dock_area(dock) if show_dock.value else frame(viewer, stroke=EDGE, rounding=6, inner_margin=8), outer_margin=GAP)
    ctx.save_state(snapshot())


if __name__ == "__main__":
    run_native(
        "Markdown",
        update_func,
        inner_width=1180,
        inner_height=760,
        backdrop=BACKDROP,
        fullsize_content=True,
        persist_id="pyegui.MarkdownFinder",
    )
