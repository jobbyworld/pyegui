"""Glass demo: a translucent window with the native macOS backdrop.

    .venv/bin/python examples/glass_demo.py [folder]

Sidebar, top bar, file list, buttons and fields on a glass; the theme (light / dark / system),
the kind of glass and its transparency can be changed live from the sidebar.
"""
import sys
from pyegui import *

folder = sys.argv[1] if len(sys.argv) > 1 else "~/Documents"
files = FileListView("files", folder)

theme = Int(0)         # 0 system, 1 light, 2 dark
mode = Int(1)          # 0 partial glass, 1 full glass
transparency = Float(1.0)   # multiplies the opacity of every surface (below 1: more see-through)
blur = Float(32.0)
name = Str("")
search = Str("")
notify = Bool(True)
accent = Int(0)
applied = [None]

ACCENTS = ["#3b82f6", "#8b5cf6", "#10b981", "#f97316"]


def sidebar():
    heading("Glass")
    separator()
    label("Theme")
    selectable_value(theme, 0, "System")
    selectable_value(theme, 1, "Light")
    selectable_value(theme, 2, "Dark")
    separator()
    label("Glass")
    selectable_value(mode, 0, "Partial (readable content)")
    selectable_value(mode, 1, "Full (everything glass)")
    slider_float(transparency, 0.2, 2.0, "opacity")
    slider_float(blur, 0.0, 80.0, "blur")
    separator()
    label("Accent")
    combo_box(accent, [0, 1, 2, 3], ["Blue", "Violet", "Green", "Orange"], "accent")


def topbar():
    with Layout(LayoutType.Horizontal):
        button_clicked("Back")
        button_clicked("Forward")
        text_edit_singleline(search, hint_text="Search…")
        switch(notify, "Notifications")


def update_func(ctx):
    key = (theme.value, mode.value, round(transparency.value, 2), round(blur.value), accent.value)
    if key != applied[0]:  # style methods: on the first frame and when the user changes something
        applied[0] = key
        {0: ctx.set_system_theme, 1: ctx.set_light_theme, 2: ctx.set_dark_theme}[theme.value]()
        ctx.set_glass_style(accent=ACCENTS[accent.value], mode="full" if mode.value else "partial",
                            opacity=transparency.value)
        ctx.set_backdrop_blur(blur.value)
    sidebar_panel(sidebar, default_size=230)
    topbar_panel(topbar)
    file_list_view(files, columns=["modified", "size", "kind"])
    bottombar_panel(lambda: label(f"{folder}  ·  glass: {'full' if mode.value else 'partial'}"))


run_native("pyegui glass", update_func, backdrop=Backdrop.LIQUID_GLASS, fullsize_content=True,
           inner_width=1000, inner_height=640, layout=dict(gap=8, rounding=12))
