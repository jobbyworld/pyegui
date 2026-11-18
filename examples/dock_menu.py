"""A menu that manages the panes of a dock: `dock_menu` gives a check box for each pane, to show or
close it, and a button that goes back to the first layout.

    python examples/dock_menu.py
"""

from pyegui import *


def text_pane(title, text):
    return DockPane(title, lambda: label(text), closable=True)


files = text_pane("Files", "The files")
editor = text_pane("Editor", "The editor")
console = text_pane("Console", "The console")
outline = text_pane("Outline", "The outline")
panes = [files, editor, console, outline]

dock = Dock(
    DockContainer(
        "horizontal",
        [files, DockContainer("vertical", [editor, console])],
    )
)
default_layout = dock.save()  # what the button of the menu goes back to


def update_func(ctx):
    def bar():
        menu_button("Panes", lambda: dock_menu(dock, panes, layout=default_layout, reset_text="Réinitialiser la disposition"))
        weak(f"Ouverts: {', '.join(dock.titles) or '—'}")

    topbar_panel(lambda: horizontal(bar))
    dock_area(dock)


run_native("Dock menu", update_func, inner_width=800, inner_height=500)
