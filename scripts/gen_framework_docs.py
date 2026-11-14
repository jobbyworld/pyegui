"""Generates docs/framework/*.md, the reference of every public name of pyegui sorted by theme.

The text comes from the docstrings of the compiled module, so the reference follows the code:

    .venv/bin/maturin develop && .venv/bin/python scripts/gen_framework_docs.py

The themes, their introductions and the names they hold are in THEMES below. A public name that
is in no theme stops the script, so a new function cannot be forgotten.
"""

import inspect
import re
import sys
import textwrap
from pathlib import Path

import pyegui

OUT = Path(__file__).resolve().parent.parent / "docs" / "framework"

# file name, title, introduction, names. "Class.member" puts one member of a class in a theme
THEMES = [
    ("app", "Application, window and context",
     "`run_native` opens the window and calls your update function once per frame. The update "
     "function of the top level receives the `Context`, which holds what is global to the app: "
     "the theme, the fonts, the clipboard, opening URLs, the state saved between two runs (`persist_id`, `save_state`, `load_state`). The styling methods of the context are "
     "in [styling.md](styling.md).",
     ["run_native", "Context", "Backdrop", "pixels_per_point"]),
    ("values", "Values that widgets edit",
     "pyegui has no callbacks. A widget that edits a value takes a holder object (`Str`, `Int`, "
     "`Float`, `Bool`, `Date`, `RGB`), reads it each frame and writes `holder.value` when the "
     "user changes it. A holder is created once, outside the update function, and kept between "
     "frames. `Response` is what `label_response`-like functions return.",
     ["Str", "Int", "Float", "Bool", "Date", "RGB", "RGBA", "Response"]),
    ("text", "Text, links and markdown",
     "Functions that show text. They are called every frame, in the order the text appears.",
     ["heading", "label", "small", "strong", "weak", "monospace", "code", "colored_label",
      "badge", "hyperlink", "hyperlink_to", "link_clicked", "label_response", "markdown"]),
    ("inputs", "Buttons and input widgets",
     "Widgets the user acts on. Buttons and toggles return True during the frame they are "
     "clicked, the other widgets write into the holder they were given.",
     ["button_clicked", "small_button_clicked", "checkbox", "radio_clicked", "radio_value",
      "selectable_label_clicked", "selectable_label_response", "selectable_value",
      "toggle_value", "switch", "segmented_control", "combo_box", "slider_int", "slider_float", "drag_int",
      "drag_float", "drag_angle", "text_edit_singleline", "text_edit_multiline", "code_editor",
      "date_picker_button", "color_edit_button_rgb", "color_edit_button_rgba", "progress",
      "spinner"]),
    ("layout", "Layout, sizes and scrolling",
     "Update functions can be nested: a function given to `horizontal`, `group`, `frame`... "
     "draws in a child ui with its own layout and style. Panels must be added before any other "
     "widget, see [panels.md](panels.md).",
     ["horizontal", "horizontal_centered", "horizontal_top", "horizontal_wrapped", "vertical",
      "vertical_centered", "vertical_centered_justified", "centered_and_justified", "columns",
      "grid", "end_row", "group", "Group", "indent", "scope", "Scope", "Layout", "LayoutType",
      "frame", "collapsing", "add_space", "separator", "set_width", "set_min_width",
      "set_max_width", "set_height", "set_min_height", "set_max_height", "available_width",
      "available_height", "scroll_area_vertical", "scroll_area_horizontal", "scroll_area_both",
      "scroll_to_cursor", "scroll_to_rect", "disable", "add_enabled", "is_enabled", "set_invisible", "is_visible",
      "set_opacity"]),
    ("panels", "Panels, windows, menus and popups",
     "Side, top and bottom panels are placed on the borders of the window, the rest is the "
     "central panel. Call the panels first in the update function: a panel added after a widget "
     "is placed inside the current ui. `sidebar_panel`, `topbar_panel` and `bottombar_panel` are the "
     "panels of the layout rules: boxes with the same gap, rounding, outline and sheet, laid out for "
     "you (see `Context.set_layout`).",
     ["sidebar_panel", "topbar_panel", "bottombar_panel", "left_panel", "right_panel", "top_panel",
      "bottom_panel", "window", "modal", "tooltip",
      "tooltip_ui", "context_menu", "menu_bar", "menu_button", "menu_image_button",
      "menu_image_text_button", "close_menu"]),
    ("data", "Tables, tree views and plots",
     "Widgets for collections. Tables only draw the visible rows, so they stay fast with many "
     "rows. A `TreeView` keeps the selection and the opened folders, the nodes are rebuilt each "
     "frame.",
     ["table", "table_ui", "tree_view", "TreeView", "TreeNode", "plot", "PlotLine",
      "PlotPoints", "PlotBars", "PlotRule"]),
    ("docking", "Docking tabs",
     "A `Dock` holds panes shown as tabs that the user can resize, rearrange by dragging and "
     "close. Create it once, add the panes, then show it with `dock_area` every frame.",
     ["Dock", "DockPane", "DockContainer", "DockTabBar", "dock_area"]),
    ("terminal", "Terminal",
     "A `Terminal` is a shell running in a pseudo terminal, drawn by `terminal`: a real terminal "
     "(colors, cursor, full screen programs, history, selection, copy and paste) to put in a "
     "`DockPane` or anywhere else. Create it once, outside the update function; the shell starts "
     "the first time it is drawn.",
     ["Terminal", "terminal"]),
    ("files", "File views and watcher",
     "Ready made views of the disk: `FileListView` (a list with sortable columns), `FileIconView` "
     "(a grid of icons) and `FileTreeView` (a tree of folders), drawn by `file_list_view`, "
     "`file_icon_view` and `file_tree_view`. They are state objects: create them once, outside the "
     "update function. Their folders are listed through a `FileWatcher` that the system notifies of "
     "every change, so a file created, renamed, modified or removed by any program shows at the "
     "next frame. Give the same watcher to all the views of an app. `path_breadcrumb` and "
     "`path_combo` show where a folder is and go to one of its ancestors.",
     ["FileListView", "FileIconView", "FileTreeView", "file_list_view", "file_icon_view",
      "file_tree_view", "FileWatcher", "path_breadcrumb", "path_combo"]),
    ("images", "Images and icons",
     "Images are given as uri: `file://` followed by a path, or an `http(s)` url. SVG icons stay "
     "sharp at any size and can be tinted.",
     ["image", "image_button_clicked", "image_and_text_clicked", "file_icon"]),
    ("interaction", "Keyboard, mouse and drag and drop",
     "Shortcuts are read with `key_pressed`; what the user did to a widget is read from its "
     "`Response` or from the value the function returns.",
     ["key_pressed", "typed_text", "modifiers", "pointer_pos", "pointer_pressed", "ui_contains_pointer", "interactive", "draw_rect",
      "dnd_drag_source", "dnd_drop_zone"]),
    ("styling", "Themes, colors, fonts and glass",
     "Everything that changes the look is a method of the `Context`, given to the update "
     "function of the top level. Call them from the first frame; `reset_style` goes back to the "
     "defaults before a new style is applied. `set_layout` (or `run_native(layout=dict(...))`) gives the "
     "spacing, corners, outline, background, sheet and opacity of the whole window once, and a region "
     "(`sidebar`, `topbar`, `bottombar`, `central`, `dock`) or a call overrides them.",
     ["Context.is_dark_theme", "Context.is_light_theme", "Context.set_dark_theme",
      "Context.set_light_theme", "Context.set_system_theme", "Context.set_style",
      "Context.reset_style", "Context.set_layout", "Context.set_accent", "Context.set_glass_style",
      "Context.set_backdrop", "Context.set_font"]),
]

PRIVATE = {"pyegui"}


def section(title, level=2):
    return f"{'#' * level} {title}\n"


def to_markdown(doc):
    """Turns a docstring written in reStructuredText style into markdown."""
    lines = inspect.cleandoc(doc or "").splitlines()
    out, i = [], 0
    while i < len(lines):
        line = lines[i]
        stripped = line.strip()
        if stripped.endswith("::"):
            title = stripped[:-2].strip()
            if title:
                out += [f"**{title}:**", ""]
            i += 1
            while i < len(lines) and not lines[i].strip():
                i += 1
            block = []
            while i < len(lines) and (not lines[i].strip() or lines[i].startswith(" ")):
                block.append(lines[i])
                i += 1
            while block and not block[-1].strip():
                block.pop()
            out += ["```python", *textwrap.dedent("\n".join(block)).splitlines(), "```", ""]
            continue
        if re.fullmatch(r"[A-Z][A-Za-z ]*:", stripped) and not line.startswith(" "):
            out += ["", f"**{stripped[:-1]}**", ""]
            i += 1
            while i < len(lines) and (not lines[i].strip() or lines[i].startswith(" ")):
                item = lines[i]
                if not item.strip():
                    i += 1
                    continue
                if len(item) - len(item.lstrip()) <= 4:
                    match = re.match(r"\s*(\w+)\s*(\([^)]*\))?\s*:\s*(.*)", item)
                    if match:
                        name, kind, text = match.groups()
                        out.append(f"- `{name}`{' ' + kind if kind else ''}: {text}".rstrip())
                    else:
                        out.append(f"- {item.strip()}")
                else:
                    out[-1] += " " + item.strip()
                i += 1
            out.append("")
            continue
        out.append(line)
        i += 1
    text = "\n".join(out)
    return re.sub(r"\n{3,}", "\n\n", text).strip() + "\n"


def signature(obj):
    try:
        text = str(inspect.signature(obj))
    except (TypeError, ValueError):
        return ""
    return re.sub(r"^\(self(, /)?(, )?", "(", text)


def describe_function(name, func):
    return f"{section(f'`{name}{signature(func)}`')}\n{to_markdown(func.__doc__)}\n"


def members(cls):
    return [m for m in dir(cls) if not m.startswith("_")]


def describe_class(name, cls, only=None):
    out = [section(f"`{name}`"), "", to_markdown(cls.__doc__), ""]
    for member in only or members(cls):
        attr = getattr(cls, member)
        kind = "method" if callable(attr) and not isinstance(attr, property) else "attribute"
        if inspect.isclass(attr) or isinstance(getattr(attr, "__self__", None), type):
            kind = "attribute"
        head = f"`{name}.{member}{signature(attr) if kind == 'method' else ''}`"
        doc = to_markdown(getattr(attr, "__doc__", "") or "")
        out += [f"- {head} ({kind}){': ' + doc.strip().splitlines()[0] if doc.strip() else ''}"]
        extra = doc.strip().splitlines()[1:]
        if extra:
            out += ["", *["  " + line if line else "" for line in extra], ""]
    return "\n".join(out) + "\n"


def describe_member(cls_name, member):
    cls = getattr(pyegui, cls_name)
    attr = getattr(cls, member)
    kind = "method" if callable(attr) and not isinstance(attr, property) else "attribute"
    title = f"`{cls_name}.{member}{signature(attr) if kind == 'method' else ''}`"
    return f"{section(title)}\n{to_markdown(attr.__doc__)}\n"


def main():
    public = {n for n in dir(pyegui) if not n.startswith("_")} - PRIVATE
    placed = set()
    OUT.mkdir(parents=True, exist_ok=True)
    index = ["# pyegui reference by theme", "",
             "Generated from the docstrings by `scripts/gen_framework_docs.py`, do not edit by hand.", ""]

    for file_name, title, intro, names in THEMES:
        parts = [f"# {title}", "", intro, ""]
        for name in names:
            if "." in name:
                cls_name, member = name.split(".")
                parts.append(describe_member(cls_name, member))
                placed.add(name)
                continue
            if name not in public:
                sys.exit(f"{name} of theme {file_name} is not in pyegui")
            obj = getattr(pyegui, name)
            placed.add(name)
            if inspect.isclass(obj):
                shown = [m for m in members(obj) if f"{name}.{m}" not in sum((t[3] for t in THEMES), [])]
                parts.append(describe_class(name, obj, shown))
            else:
                parts.append(describe_function(name, obj))
        (OUT / f"{file_name}.md").write_text("\n".join(parts).rstrip() + "\n")
        index.append(f"- [{title}]({file_name}.md): {len(names)} names")

    missing = public - placed
    if missing:
        sys.exit(f"names in no theme: {sorted(missing)}")
    uncovered = [f"Context.{m}" for m in members(pyegui.Context)
                 if f"Context.{m}" not in placed and m not in ("copy_text", "open_url", "load_state", "save_state", "request_repaint")]
    if uncovered:
        sys.exit(f"members of Context in no theme: {uncovered}")
    (OUT / "README.md").write_text("\n".join(index) + "\n")
    print(f"{len(THEMES)} files in {OUT}")


if __name__ == "__main__":
    main()
