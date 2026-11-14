"""A terminal in a dock: a sidebar of files, a viewer and, below, a dock with two terminals.

    python examples/terminal.py [folder]

The sidebar lists the files of a folder (this repository by default). A click shows the file in the
viewer. The dock at the bottom holds a shell started in the folder and a Python prompt (the most
recent Python found, for its colors): they are real terminals (colors, vim, htop). The buttons of
the top bar write commands to the shell. The Réglages pane on the right changes the layout rules
(gaps, corners, opacity, margins of each region) and the blur of the backdrop while the app runs.

Click a terminal to type in it. Select with the mouse, copy with Cmd+C and paste with Cmd+V (Ctrl on
Windows and Linux), scroll the history with the wheel.
"""

import shutil
import sys
from pathlib import Path

from pyegui import *

FOLDER = Path(
    sys.argv[1] if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent
).resolve()

files = sorted(
    p for p in FOLDER.iterdir() if p.is_file() and not p.name.startswith(".")
)
current = Str(str(files[0]) if files else "")
started = Bool(False)
cache = {"path": None, "text": Str("")}


def newest_python():
    """the most recent Python found: since 3.13 its prompt has colors, since 3.14 it colors the code"""
    for minor in range(20, sys.version_info.minor, -1):
        found = shutil.which(f"python3.{minor}")
        if found:
            return found
    return sys.executable


# the terminals are states: created once, outside the update function
shell = Terminal(cwd=str(FOLDER))
python = Terminal(cwd=str(FOLDER), shell=newest_python(), args=["-q"])


def text():
    """the text of the selected file, read again only when another one is selected"""
    if current.value != cache["path"]:
        cache["path"] = current.value
        try:
            cache["text"].value = Path(current.value).read_text(encoding="utf-8")[
                :200_000
            ]
        except (OSError, UnicodeDecodeError):
            cache["text"].value = "(not a text file)"
    return cache["text"]


def sidebar():
    weak(FOLDER.name)

    def list_files():
        for path in files:
            if selectable_label_clicked(current.value == str(path), path.name):
                current.value = str(path)

    scroll_area_vertical(list_files)


def topbar():
    def buttons():
        if button_clicked("List the files"):
            shell.run("ls -l")
        if button_clicked("Git status"):
            shell.run("git status")
        if current.value and button_clicked("Count the lines"):
            shell.run(f"wc -l '{Path(current.value).name}'")
        if button_clicked("New shell"):
            shell.restart()
        weak(shell.title)

    horizontal(buttons)


def viewer():
    scroll_area_both(lambda: code(text().value))


# the settings: the layout rules of set_layout, global then region by region. A value of a region
# that is not "fixed" is the global one, as in set_layout

TOKENS = [  # name, title, low, high
    ("gap", "Écart", 0, 40),
    ("rounding", "Arrondi", 0, 32),
    ("opacity", "Opacité", 0, 1),
    ("inner_margin", "Marge intérieure", 0, 32),
    # the outline of the boxes (outline=(width, color) in set_layout): a grey that shows on light and
    # dark sheets, here its width and how opaque it is
    ("outline_width", "Contour : épaisseur", 0, 4),
    ("outline_alpha", "Contour : opacité", 0, 1),
]
OUTLINE = ("outline_width", "outline_alpha")
REGIONS = [("sidebar", "Barre latérale"), ("topbar", "Barre du haut"), ("dock", "Dock")]
rules = {
    name: Float(value)
    for name, value in [
        ("gap", 32),
        ("rounding", 16),
        ("opacity", 1),
        ("inner_margin", 8),
        ("outline_width", 1),  # the default outline of the rules: 1 point of grey at 60/255
        ("outline_alpha", 0.24),
    ]
}
background = Float(0.0)  # the opacity of what is seen between the boxes; 0 shows the backdrop
blur = Float(0.0)  # the blur of the backdrop, macOS
# the modern style: the Inter font and menus of glass; the transparency of the boxes is the one of
# the layout rules alone (opacity, background), so the glass does not add a second layer to them
modern = Bool(True)
# per region: is the value fixed, and which; the starting values are the ones of this example
region_rules = {
    region: {name: (Bool(False), Float(rules[name].value)) for name, *_ in TOKENS} for region, _ in REGIONS
}
for region, name, value in [("sidebar", "gap", 0), ("sidebar", "rounding", 0), ("topbar", "gap", 8)]:
    region_rules[region][name][0].value = True
    region_rules[region][name][1].value = value
dirty = Bool(True)  # the rules must be given again to set_layout


def outline(width, alpha):
    """the outline of set_layout: (width, color)"""
    return (round(width, 2), (127, 127, 127, round(255 * alpha)))


def layout_rules():
    """the dict given to set_layout (and that can be copied into run_native(layout=...))"""
    dark = (20, 22, 30, round(255 * background.value))
    result = {name: round(value.value, 2) for name, value in rules.items() if name not in OUTLINE}
    result["outline"] = outline(rules["outline_width"].value, rules["outline_alpha"].value)
    result["background"] = dark
    for region, _ in REGIONS:
        own = region_rules[region]
        fixed = {
            name: round(value.value, 2)
            for name, (on, value) in own.items()
            if on.value and name not in OUTLINE
        }
        if any(own[name][0].value for name in OUTLINE):
            # the part of the outline that is not fixed in the region is the global one
            width, alpha = (
                own[name][1].value if own[name][0].value else rules[name].value for name in OUTLINE
            )
            fixed["outline"] = outline(width, alpha)
        if fixed:
            result[region] = fixed
    return result


def apply_settings(ctx):
    ctx.set_layout(**layout_rules())
    ctx.reset_style()
    if modern.value:
        # panels=1: the boxes keep an opaque color, which the opacity of the rules then makes clear
        ctx.set_glass_style(accent="#3b82f6", mode="full", panels=1.0)
    else:
        ctx.set_accent("#3b82f6")
    ctx.set_backdrop_blur(blur.value)  # macOS only


def settings():
    changed = False

    def slider_row(title, value, low, high):
        nonlocal changed
        weak(title)
        before = value.value
        slider_float(value, low, high, "")
        changed |= value.value != before

    def global_rows():
        for name, title, low, high in TOKENS:
            slider_row(title, rules[name], low, high)
            end_row()
        slider_row("Fond entre les boîtes", background, 0, 1)
        end_row()
        slider_row("Flou du fond (macOS)", blur, 0, 64)
        end_row()

    before = modern.value
    toggle_value(modern, "Style moderne (police Inter, menus en verre)")
    changed |= modern.value != before
    strong("Global")
    grid("global_rules", global_rows, num_columns=2, spacing=(12, 6))

    for region, title in REGIONS:

        def rows(region=region):
            nonlocal changed
            for name, token_title, low, high in TOKENS:
                on, value = region_rules[region][name]
                weak(token_title)
                before = (on.value, value.value)
                toggle_value(on, "fixé" if on.value else "global")
                add_enabled(on.value, lambda value=value, low=low, high=high: slider_float(value, low, high, ""))
                changed |= (on.value, value.value) != before
                end_row()

        collapsing(title, lambda rows=rows, region=region: grid(f"rules_{region}", rows, num_columns=3, spacing=(12, 6)))

    separator()
    call = f"layout={layout_rules()!r}"
    if small_button_clicked("Copier les règles"):
        context[0].copy_text(call)
    small(call)
    if changed:
        dirty.value = True


dock = Dock(
    DockContainer(
        "vertical",
        [
            DockContainer(
                "horizontal",
                [
                    DockPane("Viewer", viewer, hide_tab=True),
                    DockPane("Réglages", lambda: scroll_area_vertical(settings), closable=True),
                ],
            ),
            DockContainer(
                "tabs",
                [
                    DockPane("Terminal", lambda: terminal(shell), sheet="#ff000050"),
                    DockPane("Python", lambda: terminal(python)),
                ],
            ),
        ],
    ),
    id="terminal_example",
    tab_bar_height=26,
)
layout = dock.save()
layout["shares"] = [3.0, 2.0]
layout["children"][0]["shares"] = [3.0, 2.0]  # the settings take two fifths of the width
dock.load(layout)
context = [None]


def update_func(ctx):
    context[0] = ctx
    if not started.value:
        # ctx.set_system_theme()
        ctx.set_dark_theme()
        shell.focus()
        started.value = True
    if dirty.value:  # the first frame, and when a setting changed
        apply_settings(ctx)
        dirty.value = False

    # the gaps and corners of the regions come from the rules, set in the Réglages pane
    sidebar_panel(sidebar, default_size=200, min_size=140)
    topbar_panel(topbar)
    dock_area(dock)


BACKDROP = {"win32": Backdrop.ACRYLIC, "darwin": Backdrop.LIQUID_GLASS}.get(
    sys.platform
)
if __name__ == "__main__":
    run_native(
        "Terminal",
        update_func,
        inner_width=1100,
        inner_height=720,
        fullsize_content=True,
        layout=layout_rules(),
        # transparent=True,
        backdrop=BACKDROP,
        #
    )
